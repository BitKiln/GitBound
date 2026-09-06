// Auditing the authorship of a revision range.
//
// Every other check in GitBound asks "is this working copy configured
// correctly *right now*". That is the right question on a developer's machine
// and the wrong one in a pipeline, where the working copy is a fresh clone with
// no bindings and the interesting evidence is the commits themselves.
//
// So this module asks the complementary question: over a range of commits, was
// each one authored by somebody this repository accepts, by the same person who
// committed it, and signed where a signature is required.
//
// It produces a `CheckReport` like everything else, so `src/report.rs` renders
// it into SARIF, JUnit, or GitHub annotations for free.

use crate::{
    check::{CheckItem, CheckReport, CheckStatus},
    config::Profile,
    error::GitBoundError,
    policy::Policy,
    process::Runner,
};
use std::{ffi::OsString, path::Path, time::Duration};

const TIMEOUT: Duration = Duration::from_secs(30);

/// Field and record separators. Unit separator and record separator are chosen
/// because Git will not emit them from a name, an address, or a subject line,
/// so the output cannot be confused by an author who puts a comma or a newline
/// in their display name.
const FIELD: char = '\u{1f}';
const RECORD: char = '\u{1e}';

const FORMAT: &str = "--format=%H%x1f%an%x1f%ae%x1f%cn%x1f%ce%x1f%G?%x1f%s%x1e";

/// Upper bound on how many commits are examined, so an accidental `--range
/// HEAD` on a large repository cannot turn into an unbounded report.
pub const DEFAULT_MAX_COMMITS: usize = 1000;

pub struct AuditOptions<'a> {
    /// A Git revision range, e.g. `origin/main..HEAD`.
    pub range: &'a str,
    pub max_commits: usize,
    /// Fail when a commit carries no valid signature. Independent of policy so
    /// that `--enforce-signing` can turn it on for one run.
    pub require_signature: bool,
}

impl Default for AuditOptions<'_> {
    fn default() -> Self {
        Self {
            range: "HEAD",
            max_commits: DEFAULT_MAX_COMMITS,
            require_signature: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitRecord {
    pub sha: String,
    pub author_name: String,
    pub author_email: String,
    pub committer_name: String,
    pub committer_email: String,
    /// Git's `%G?` verification code.
    pub signature: char,
    pub subject: String,
}

impl CommitRecord {
    pub fn short(&self) -> &str {
        let end = self.sha.len().min(8);
        &self.sha[..end]
    }

    /// `G` good, `U` good but the key is untrusted — both mean a signature was
    /// present and verified. Everything else (`N` none, `B` bad, `X`/`Y`
    /// expired, `R` revoked, `E` unverifiable) is treated as unsigned, because
    /// a signature that cannot be trusted is not evidence of anything.
    pub fn is_signed(&self) -> bool {
        matches!(self.signature, 'G' | 'U')
    }

    fn signature_reason(&self) -> &'static str {
        match self.signature {
            'N' => "is not signed",
            'B' => "has a bad signature",
            'X' => "has a signature made by an expired key",
            'Y' => "has a signature made by a key that expired",
            'R' => "has a signature made by a revoked key",
            'E' => "has a signature that could not be verified",
            _ => "has no usable signature",
        }
    }
}

/// Read the commits in `range`. The range is validated before it reaches Git:
/// anything that looks like an option is refused, so a range cannot smuggle in
/// `--output` or `--exec`.
pub fn commits(
    runner: &dyn Runner,
    repository: &Path,
    options: &AuditOptions<'_>,
) -> Result<Vec<CommitRecord>, GitBoundError> {
    let range = options.range.trim();
    if range.is_empty() {
        return Err(GitBoundError::usage("a revision range is required"));
    }
    if range.starts_with('-') {
        return Err(GitBoundError::usage(format!(
            "'{range}' is not a revision range"
        )));
    }

    let args = vec![
        OsString::from("log"),
        OsString::from("--no-color"),
        OsString::from(format!("--max-count={}", options.max_commits)),
        OsString::from(FORMAT),
        OsString::from(range),
        OsString::from("--"),
    ];
    let output = runner.run_git_in(&args, repository, TIMEOUT)?;
    if !output.success() {
        return Err(GitBoundError::usage(format!(
            "cannot read the range '{range}': {}",
            output.stderr.trim()
        )));
    }
    Ok(parse(&output.stdout))
}

fn parse(stdout: &str) -> Vec<CommitRecord> {
    stdout
        .split(RECORD)
        // `%x1e` leaves the newline that terminated the previous record
        // attached to the front of the next one.
        .map(|record| record.trim_start_matches(['\r', '\n']))
        .filter(|record| !record.trim().is_empty())
        .filter_map(|record| {
            let mut fields = record.split(FIELD);
            Some(CommitRecord {
                sha: fields.next()?.trim().to_string(),
                author_name: fields.next()?.to_string(),
                author_email: fields.next()?.to_string(),
                committer_name: fields.next()?.to_string(),
                committer_email: fields.next()?.to_string(),
                signature: fields.next()?.chars().next().unwrap_or('N'),
                subject: fields.next().unwrap_or_default().to_string(),
            })
        })
        .collect()
}

/// Audit a range and return a report.
///
/// `policy` is the repository's committed policy, when it has one. `profile` is
/// the locally bound profile, when there is one; its address is treated as
/// implicitly allowed, so a developer running this on their own machine does
/// not have to also list themselves in the policy file.
pub fn audit(
    runner: &dyn Runner,
    repository: &Path,
    policy: Option<&Policy>,
    profile: Option<&Profile>,
    options: &AuditOptions<'_>,
) -> Result<CheckReport, GitBoundError> {
    let commits = commits(runner, repository, options)?;
    let empty = Policy::default();
    let policy = policy.unwrap_or(&empty);
    let mut checks = Vec::new();

    if commits.is_empty() {
        checks.push(CheckItem {
            id: "commit_range".into(),
            status: CheckStatus::Ok,
            expected: Some(options.range.to_string()),
            actual: Some("0 commits".into()),
            message: format!("no commits in '{}'; nothing to audit", options.range),
        });
        return Ok(report(repository, profile, checks));
    }

    let mut offenders = 0usize;
    for commit in &commits {
        let permitted = policy.allows_email(&commit.author_email).is_ok()
            || profile.is_some_and(|profile| {
                profile
                    .git_email
                    .eq_ignore_ascii_case(commit.author_email.trim())
            });
        if !permitted {
            offenders += 1;
            checks.push(CheckItem {
                id: "commit_author".into(),
                status: CheckStatus::Failure,
                expected: expectation(policy, profile),
                actual: Some(format!("{} <{}>", commit.short(), commit.author_email)),
                message: format!(
                    "{} \"{}\" was authored by {} <{}>, which this repository does not accept",
                    commit.short(),
                    commit.subject,
                    commit.author_name,
                    commit.author_email
                ),
            });
        }

        if policy.identity.require_author_matches_committer
            && !commit
                .author_email
                .eq_ignore_ascii_case(&commit.committer_email)
        {
            offenders += 1;
            checks.push(CheckItem {
                id: "commit_committer".into(),
                status: CheckStatus::Failure,
                expected: Some(commit.author_email.clone()),
                actual: Some(commit.committer_email.clone()),
                message: format!(
                    "{} was authored by {} but committed by {}",
                    commit.short(),
                    commit.author_email,
                    commit.committer_email
                ),
            });
        }

        if (options.require_signature || policy.signing.require) && !commit.is_signed() {
            offenders += 1;
            checks.push(CheckItem {
                id: "commit_signature".into(),
                status: CheckStatus::Failure,
                expected: Some("a verified signature".into()),
                actual: Some(commit.signature.to_string()),
                message: format!(
                    "{} \"{}\" {}",
                    commit.short(),
                    commit.subject,
                    commit.signature_reason()
                ),
            });
        }
    }

    if offenders == 0 {
        checks.push(CheckItem {
            id: "commit_range".into(),
            status: CheckStatus::Ok,
            expected: Some(options.range.to_string()),
            actual: Some(format!("{} commits", commits.len())),
            message: format!(
                "all {} commits in '{}' are within policy",
                commits.len(),
                options.range
            ),
        });
    }

    if commits.len() == options.max_commits {
        checks.push(CheckItem {
            id: "commit_range".into(),
            status: CheckStatus::Warning,
            expected: Some(format!("at most {} commits", options.max_commits)),
            actual: Some(format!("{} commits", commits.len())),
            message: format!(
                "the range was truncated at {} commits; narrow it or raise --max-commits to audit the rest",
                options.max_commits
            ),
        });
    }

    Ok(report(repository, profile, checks))
}

fn expectation(policy: &Policy, profile: Option<&Profile>) -> Option<String> {
    let mut parts: Vec<String> = Vec::new();
    parts.extend(policy.identity.allowed_emails.iter().cloned());
    parts.extend(
        policy
            .identity
            .allowed_email_domains
            .iter()
            .map(|domain| format!("*@{domain}")),
    );
    if let Some(profile) = profile {
        parts.push(profile.git_email.clone());
    }
    if parts.is_empty() {
        None
    } else {
        parts.sort();
        parts.dedup();
        Some(parts.join(", "))
    }
}

fn report(repository: &Path, profile: Option<&Profile>, checks: Vec<CheckItem>) -> CheckReport {
    CheckReport {
        repository: repository.display().to_string(),
        profile: profile.map(|profile| profile.github_user.clone()),
        remote: None,
        overall: CheckReport::overall_of(&checks),
        checks,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{policy::IdentityPolicy, process::ProcessOutput};
    use std::sync::Mutex;

    struct FakeRunner {
        stdout: String,
        code: i32,
        seen: Mutex<Vec<Vec<OsString>>>,
    }

    impl FakeRunner {
        fn ok(stdout: &str) -> Self {
            Self {
                stdout: stdout.into(),
                code: 0,
                seen: Mutex::new(Vec::new()),
            }
        }
    }

    impl Runner for FakeRunner {
        fn run(
            &self,
            _program: &str,
            args: &[OsString],
            _timeout: Duration,
        ) -> Result<ProcessOutput, GitBoundError> {
            self.seen.lock().expect("lock").push(args.to_vec());
            Ok(ProcessOutput {
                code: Some(self.code),
                stdout: self.stdout.clone(),
                stderr: String::new(),
            })
        }

        fn run_in(
            &self,
            program: &str,
            args: &[OsString],
            _cwd: &Path,
            timeout: Duration,
        ) -> Result<ProcessOutput, GitBoundError> {
            self.run(program, args, timeout)
        }
    }

    fn line(sha: &str, email: &str, signature: char) -> String {
        format!(
            "{sha}\u{1f}Alice\u{1f}{email}\u{1f}Alice\u{1f}{email}\u{1f}{signature}\u{1f}a change\u{1e}\n"
        )
    }

    fn company_policy() -> Policy {
        Policy {
            identity: IdentityPolicy {
                allowed_email_domains: vec!["company.example".into()],
                ..IdentityPolicy::default()
            },
            ..Policy::default()
        }
    }

    #[test]
    fn parses_every_field_of_every_record() {
        let stdout = format!(
            "{}{}",
            line("aaaaaaaaaaaa", "alice@company.example", 'G'),
            line("bbbbbbbbbbbb", "bob@personal.example", 'N')
        );
        let parsed = parse(&stdout);
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].short(), "aaaaaaaa");
        assert_eq!(parsed[0].author_email, "alice@company.example");
        assert!(parsed[0].is_signed());
        assert_eq!(parsed[1].author_email, "bob@personal.example");
        assert!(!parsed[1].is_signed());
    }

    #[test]
    fn a_disallowed_author_fails_the_range() {
        let runner = FakeRunner::ok(&format!(
            "{}{}",
            line("aaaaaaaaaaaa", "alice@company.example", 'G'),
            line("bbbbbbbbbbbb", "bob@personal.example", 'G')
        ));
        let policy = company_policy();
        let report = audit(
            &runner,
            Path::new("/tmp/project"),
            Some(&policy),
            None,
            &AuditOptions {
                range: "origin/main..HEAD",
                ..AuditOptions::default()
            },
        )
        .expect("audit");
        assert!(!report.enforceable());
        let offenders: Vec<_> = report
            .checks
            .iter()
            .filter(|check| check.id == "commit_author")
            .collect();
        assert_eq!(offenders.len(), 1);
        assert!(offenders[0].actual.as_deref().unwrap().contains("bbbbbbbb"));
    }

    #[test]
    fn a_clean_range_reports_one_summary_check() {
        let runner = FakeRunner::ok(&line("aaaaaaaaaaaa", "alice@company.example", 'G'));
        let policy = company_policy();
        let report = audit(
            &runner,
            Path::new("/tmp/project"),
            Some(&policy),
            None,
            &AuditOptions::default(),
        )
        .expect("audit");
        assert!(report.enforceable());
        assert_eq!(report.checks.len(), 1);
        assert_eq!(report.checks[0].id, "commit_range");
    }

    #[test]
    fn the_bound_profile_is_implicitly_allowed() {
        let runner = FakeRunner::ok(&line("aaaaaaaaaaaa", "alice@personal.example", 'G'));
        let policy = company_policy();
        let profile = Profile {
            github_user: "alice".into(),
            git_name: "Alice".into(),
            git_email: "alice@personal.example".into(),
            hostname: "github.com".into(),
            ssh_host: None,
            ssh_key: None,
            allowed_owners: Vec::new(),
            signing_key: None,
            signing_format: crate::config::SigningFormat::default(),
            require_signing: false,
        };
        let report = audit(
            &runner,
            Path::new("/tmp/project"),
            Some(&policy),
            Some(&profile),
            &AuditOptions::default(),
        )
        .expect("audit");
        assert!(report.enforceable(), "{:?}", report.checks);
    }

    #[test]
    fn required_signatures_are_enforced_per_commit() {
        let runner = FakeRunner::ok(&line("aaaaaaaaaaaa", "alice@company.example", 'N'));
        let policy = company_policy();
        let report = audit(
            &runner,
            Path::new("/tmp/project"),
            Some(&policy),
            None,
            &AuditOptions {
                require_signature: true,
                ..AuditOptions::default()
            },
        )
        .expect("audit");
        assert!(!report.enforceable());
        assert!(report.checks.iter().any(|c| c.id == "commit_signature"));
    }

    #[test]
    fn an_option_shaped_range_is_refused_before_reaching_git() {
        let runner = FakeRunner::ok("");
        let error = commits(
            &runner,
            Path::new("/tmp/project"),
            &AuditOptions {
                range: "--output=/tmp/pwned",
                ..AuditOptions::default()
            },
        )
        .expect_err("must refuse");
        assert!(error.to_string().contains("not a revision range"));
        assert!(
            runner.seen.lock().expect("lock").is_empty(),
            "git must not have been invoked"
        );
    }

    #[test]
    fn an_empty_range_is_reported_rather_than_passing_silently() {
        let runner = FakeRunner::ok("");
        let report = audit(
            &runner,
            Path::new("/tmp/project"),
            None,
            None,
            &AuditOptions::default(),
        )
        .expect("audit");
        assert!(report.enforceable());
        assert!(report.checks[0].message.contains("nothing to audit"));
    }
}
