// Repository-committed policy.
//
// The user configuration in `src/config.rs` answers "who am I on this machine".
// This file answers a different question — "who is this repository willing to
// accept commits from" — and it belongs to the repository, not to the developer,
// which is why it is a separate file with a separate schema.
//
// Three rules keep this inside the product's safety boundary:
//
//   1. Policy only. Allowed addresses, hosts, owners, signing requirements.
//      Never a token, never key material, never a path outside the repository.
//   2. Read-only. Nothing in GitBound writes `.gitbound.toml`. A repository
//      that is checked out is inspected, never modified, by policy evaluation.
//   3. Opt-in. No file means behaviour is exactly what it was before this
//      module existed.
//
// Like `Config`, it fails closed on a schema version it does not understand
// rather than guessing at a future format.

use crate::{
    check::{CheckItem, CheckReport, CheckStatus},
    config::SigningFormat,
    error::GitBoundError,
};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// The file name looked for at the root of an inspected repository.
pub const POLICY_FILE: &str = ".gitbound.toml";

/// The only schema this build understands.
pub const CURRENT_POLICY_SCHEMA: u32 = 1;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    #[serde(default)]
    pub identity: IdentityPolicy,
    #[serde(default)]
    pub remote: RemotePolicy,
    #[serde(default)]
    pub signing: SigningPolicy,
}

fn default_schema_version() -> u32 {
    CURRENT_POLICY_SCHEMA
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct IdentityPolicy {
    /// Bare domains, matched case-insensitively against the part after `@`.
    #[serde(default)]
    pub allowed_email_domains: Vec<String>,
    /// Whole addresses, matched case-insensitively. Combined with the domain
    /// list as a union: an address passes if either list admits it.
    #[serde(default)]
    pub allowed_emails: Vec<String>,
    /// Reject GitHub's `users.noreply.github.com` addresses, which hide who
    /// actually authored a commit behind an account alias.
    #[serde(default)]
    pub deny_noreply: bool,
    /// Require that a commit's author and committer are the same person.
    #[serde(default)]
    pub require_author_matches_committer: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RemotePolicy {
    #[serde(default)]
    pub allowed_hosts: Vec<String>,
    #[serde(default)]
    pub allowed_owners: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SigningPolicy {
    #[serde(default)]
    pub require: bool,
    #[serde(default)]
    pub format: Option<SigningFormat>,
}

impl Policy {
    /// Look for a policy file at the root of `repository`. `Ok(None)` means the
    /// repository declares no policy, which is not an error.
    pub fn discover(repository: &Path) -> Result<Option<Self>, GitBoundError> {
        let path = repository.join(POLICY_FILE);
        if !path.is_file() {
            return Ok(None);
        }
        Self::load(&path).map(Some)
    }

    /// Read a policy file from an explicit path. Used by `--policy`, where a
    /// missing file *is* an error because the user named it.
    pub fn load(path: &Path) -> Result<Self, GitBoundError> {
        let text = std::fs::read_to_string(path).map_err(|error| {
            GitBoundError::usage(format!("cannot read {}: {error}", path.display()))
        })?;
        let policy: Self = toml::from_str(&text).map_err(|error| {
            GitBoundError::usage(format!("cannot parse {}: {error}", path.display()))
        })?;
        if policy.schema_version == 0 || policy.schema_version > CURRENT_POLICY_SCHEMA {
            return Err(GitBoundError::usage(format!(
                "{} declares schema_version {}, but this build understands {}. Upgrade GitBound rather than ignoring the policy.",
                path.display(),
                policy.schema_version,
                CURRENT_POLICY_SCHEMA
            )));
        }
        policy.validate(path)?;
        Ok(policy)
    }

    fn validate(&self, path: &Path) -> Result<(), GitBoundError> {
        let complain = |what: &str| {
            Err(GitBoundError::usage(format!(
                "{} contains an empty {what}",
                path.display()
            )))
        };
        if self
            .identity
            .allowed_email_domains
            .iter()
            .any(|value| is_blank(value))
        {
            return complain("identity.allowed_email_domains entry");
        }
        if self
            .identity
            .allowed_emails
            .iter()
            .any(|value| is_blank(value))
        {
            return complain("identity.allowed_emails entry");
        }
        if self
            .remote
            .allowed_hosts
            .iter()
            .any(|value| is_blank(value))
        {
            return complain("remote.allowed_hosts entry");
        }
        if self
            .remote
            .allowed_owners
            .iter()
            .any(|value| is_blank(value))
        {
            return complain("remote.allowed_owners entry");
        }
        Ok(())
    }

    /// True when this policy places no constraint at all, which lets callers
    /// skip reporting on it.
    pub fn is_empty(&self) -> bool {
        self.identity.allowed_email_domains.is_empty()
            && self.identity.allowed_emails.is_empty()
            && !self.identity.deny_noreply
            && !self.identity.require_author_matches_committer
            && self.remote.allowed_hosts.is_empty()
            && self.remote.allowed_owners.is_empty()
            && !self.signing.require
            && self.signing.format.is_none()
    }

    /// Whether an author address satisfies the identity policy. An empty
    /// allowlist means "no restriction", not "allow nothing" — the same
    /// convention `Profile::allowed_owners` already uses in `src/check.rs`.
    pub fn allows_email(&self, email: &str) -> Result<(), String> {
        let email = email.trim();
        if email.is_empty() {
            return Err("commit has no author email".into());
        }
        if self.identity.deny_noreply && is_noreply(email) {
            return Err(format!("{email} is a GitHub noreply address"));
        }
        let identity = &self.identity;
        if identity.allowed_emails.is_empty() && identity.allowed_email_domains.is_empty() {
            return Ok(());
        }
        if identity
            .allowed_emails
            .iter()
            .any(|allowed| allowed.eq_ignore_ascii_case(email))
        {
            return Ok(());
        }
        if let Some((_, domain)) = email.rsplit_once('@')
            && identity
                .allowed_email_domains
                .iter()
                .any(|allowed| allowed.eq_ignore_ascii_case(domain))
        {
            return Ok(());
        }
        Err(format!("{email} is not permitted by {POLICY_FILE}"))
    }

    /// Additional check items derived from this policy and an already-computed
    /// report. Deliberately reads the report rather than re-running git: the
    /// facts it needs — the effective author email, the remote host and owner,
    /// the signing state — have all been established once already.
    /// Extend `report` with the verdict of the policy the inspected repository
    /// commits, if it commits one.
    ///
    /// Every caller that produces a report for a human or a gate has to do
    /// this, and doing it in two places is how the desktop app came to show a
    /// repository as clean while `gitbound check` on the same repository failed
    /// on its policy. `report.repository` is the resolved worktree root, which
    /// is where a committed policy lives — not necessarily the directory the
    /// caller pointed at.
    pub fn apply_committed(report: &mut CheckReport) -> Result<(), GitBoundError> {
        let Some(policy) = Self::discover(Path::new(&report.repository))? else {
            return Ok(());
        };
        let verdict = policy.evaluate(report);
        report.extend(verdict);
        Ok(())
    }

    pub fn evaluate(&self, report: &CheckReport) -> Vec<CheckItem> {
        let mut checks = Vec::new();

        if !self.identity.allowed_emails.is_empty()
            || !self.identity.allowed_email_domains.is_empty()
            || self.identity.deny_noreply
        {
            let actual = actual_of(report, "git_email");
            let expected = self.email_expectation();
            match actual {
                Some(email) => match self.allows_email(email) {
                    Ok(()) => checks.push(pass(
                        "policy_email",
                        expected,
                        Some(email.to_string()),
                        "author email is permitted by repository policy",
                    )),
                    Err(reason) => checks.push(fail(
                        "policy_email",
                        expected,
                        Some(email.to_string()),
                        reason,
                    )),
                },
                None => checks.push(unverified(
                    "policy_email",
                    expected,
                    "no Git author email is configured, so repository policy cannot be applied",
                )),
            }
        }

        if !self.remote.allowed_hosts.is_empty() {
            let expected = Some(self.remote.allowed_hosts.join(", "));
            match report.remote.as_ref() {
                Some(remote) => {
                    let allowed = self
                        .remote
                        .allowed_hosts
                        .iter()
                        .any(|host| host.eq_ignore_ascii_case(&remote.hostname));
                    let actual = Some(remote.hostname.clone());
                    if allowed {
                        checks.push(pass(
                            "policy_host",
                            expected,
                            actual,
                            "remote host is permitted by repository policy",
                        ));
                    } else {
                        checks.push(fail(
                            "policy_host",
                            expected,
                            actual,
                            "remote host is not permitted by repository policy",
                        ));
                    }
                }
                None => checks.push(unverified(
                    "policy_host",
                    expected,
                    "no remote is configured, so the host policy cannot be applied",
                )),
            }
        }

        if !self.remote.allowed_owners.is_empty() {
            let expected = Some(self.remote.allowed_owners.join(", "));
            match report.remote.as_ref() {
                Some(remote) => {
                    let allowed = self
                        .remote
                        .allowed_owners
                        .iter()
                        .any(|owner| owner.eq_ignore_ascii_case(&remote.owner));
                    let actual = Some(remote.owner.clone());
                    if allowed {
                        checks.push(pass(
                            "policy_owner",
                            expected,
                            actual,
                            "remote owner is permitted by repository policy",
                        ));
                    } else {
                        checks.push(fail(
                            "policy_owner",
                            expected,
                            actual,
                            "remote owner is not permitted by repository policy",
                        ));
                    }
                }
                None => checks.push(unverified(
                    "policy_owner",
                    expected,
                    "no remote is configured, so the owner policy cannot be applied",
                )),
            }
        }

        if self.signing.require {
            let actual = actual_of(report, "commit_signing").map(str::to_string);
            let enabled = actual.as_deref() == Some("true");
            let expected = Some("true".to_string());
            if enabled {
                checks.push(pass(
                    "policy_signing",
                    expected,
                    actual,
                    "commit signing is enabled as repository policy requires",
                ));
            } else {
                checks.push(fail(
                    "policy_signing",
                    expected,
                    actual,
                    "repository policy requires commit signing, which is not enabled",
                ));
            }
        }

        if let Some(format) = self.signing.format {
            let expected = Some(format.as_git_value().to_string());
            let actual = actual_of(report, "signing_format").map(str::to_string);
            match &actual {
                Some(value) if value.eq_ignore_ascii_case(format.as_git_value()) => {
                    checks.push(pass(
                        "policy_signing_format",
                        expected,
                        actual.clone(),
                        "commit signing format matches repository policy",
                    ));
                }
                Some(_) => checks.push(fail(
                    "policy_signing_format",
                    expected,
                    actual.clone(),
                    "commit signing format does not match repository policy",
                )),
                None => checks.push(unverified(
                    "policy_signing_format",
                    expected,
                    "no commit signing format is configured, so repository policy cannot be applied",
                )),
            }
        }

        checks
    }

    fn email_expectation(&self) -> Option<String> {
        let mut parts = Vec::new();
        parts.extend(self.identity.allowed_emails.iter().cloned());
        parts.extend(
            self.identity
                .allowed_email_domains
                .iter()
                .map(|domain| format!("*@{domain}")),
        );
        if parts.is_empty() {
            None
        } else {
            Some(parts.join(", "))
        }
    }
}

const NOREPLY_SUFFIX: &str = "users.noreply.github.com";

/// Whether the address is a GitHub noreply address.
///
/// Matched on the domain, not on the whole string: a plain suffix test also
/// catches `a@notusers.noreply.github.com`, a domain GitHub does not own and
/// anyone could register, and rejecting it would mean rejecting a real address
/// with a message naming the wrong reason.
fn is_noreply(email: &str) -> bool {
    let Some((_, domain)) = email.rsplit_once('@') else {
        return false;
    };
    let domain = domain.to_ascii_lowercase();
    domain == NOREPLY_SUFFIX || domain.ends_with(&format!(".{NOREPLY_SUFFIX}"))
}

fn is_blank(value: &str) -> bool {
    value.trim().is_empty()
}

fn actual_of<'a>(report: &'a CheckReport, id: &str) -> Option<&'a str> {
    report
        .checks
        .iter()
        .find(|check| check.id == id)
        .and_then(|check| check.actual.as_deref())
}

fn pass(
    id: &str,
    expected: Option<String>,
    actual: Option<String>,
    message: impl Into<String>,
) -> CheckItem {
    CheckItem {
        id: id.into(),
        status: CheckStatus::Ok,
        expected,
        actual,
        message: message.into(),
    }
}

fn fail(
    id: &str,
    expected: Option<String>,
    actual: Option<String>,
    message: impl Into<String>,
) -> CheckItem {
    CheckItem {
        id: id.into(),
        status: CheckStatus::Failure,
        expected,
        actual,
        message: message.into(),
    }
}

fn unverified(id: &str, expected: Option<String>, message: impl Into<String>) -> CheckItem {
    CheckItem {
        id: id.into(),
        status: CheckStatus::Unverified,
        expected,
        actual: None,
        message: message.into(),
    }
}

/// Where a policy file would live for a repository, for messages and for the
/// SARIF anchor.
pub fn policy_path(repository: &Path) -> PathBuf {
    repository.join(POLICY_FILE)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::check::OverallStatus;
    use std::io::Write;

    fn write(text: &str) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join(POLICY_FILE);
        let mut file = std::fs::File::create(&path).expect("create");
        file.write_all(text.as_bytes()).expect("write");
        (dir, path)
    }

    #[test]
    fn an_absent_file_is_not_an_error() {
        let dir = tempfile::tempdir().expect("temp dir");
        assert_eq!(Policy::discover(dir.path()).expect("discover"), None);
    }

    #[test]
    fn a_future_schema_fails_closed() {
        let (_dir, path) = write("schema_version = 99\n");
        let error = Policy::load(&path).expect_err("must refuse");
        assert!(error.to_string().contains("schema_version 99"));
    }

    #[test]
    fn a_zero_schema_fails_closed() {
        let (_dir, path) = write("schema_version = 0\n");
        assert!(Policy::load(&path).is_err());
    }

    #[test]
    fn an_unknown_key_is_rejected_rather_than_silently_ignored() {
        let (_dir, path) = write("schema_version = 1\nallow_everything = true\n");
        assert!(Policy::load(&path).is_err());
    }

    #[test]
    fn domain_and_address_allowlists_are_a_union() {
        let policy = Policy {
            schema_version: 1,
            identity: IdentityPolicy {
                allowed_email_domains: vec!["company.example".into()],
                allowed_emails: vec!["bot@ci.example".into()],
                ..IdentityPolicy::default()
            },
            ..Policy::default()
        };
        assert!(policy.allows_email("alice@COMPANY.example").is_ok());
        assert!(policy.allows_email("BOT@ci.example").is_ok());
        assert!(policy.allows_email("alice@personal.example").is_err());
    }

    #[test]
    fn an_empty_allowlist_permits_everything() {
        let policy = Policy::default();
        assert!(policy.allows_email("anyone@anywhere.example").is_ok());
    }

    #[test]
    fn deny_noreply_applies_even_with_an_empty_allowlist() {
        let policy = Policy {
            identity: IdentityPolicy {
                deny_noreply: true,
                ..IdentityPolicy::default()
            },
            ..Policy::default()
        };
        assert!(
            policy
                .allows_email("1234+alice@users.noreply.github.com")
                .is_err()
        );
        assert!(policy.allows_email("alice@company.example").is_ok());
        // Subdomains of the real host count; look-alikes that merely end in the
        // same characters are somebody else's domain and must not be blamed on
        // GitHub.
        assert!(
            policy
                .allows_email("a@eu.users.noreply.github.com")
                .is_err()
        );
        assert!(policy.allows_email("a@notusers.noreply.github.com").is_ok());
        assert!(policy.allows_email("users.noreply.github.com").is_ok());
    }

    #[test]
    fn evaluate_reads_the_existing_report_rather_than_rerunning_git() {
        let policy = Policy {
            identity: IdentityPolicy {
                allowed_email_domains: vec!["company.example".into()],
                ..IdentityPolicy::default()
            },
            ..Policy::default()
        };
        let report = CheckReport {
            repository: "/tmp/project".into(),
            profile: Some("work".into()),
            remote: None,
            overall: OverallStatus::Ok,
            checks: vec![CheckItem {
                id: "git_email".into(),
                status: CheckStatus::Ok,
                expected: None,
                actual: Some("alice@personal.example".into()),
                message: "Git author email".into(),
            }],
        };
        let checks = policy.evaluate(&report);
        assert_eq!(checks.len(), 1);
        assert_eq!(checks[0].id, "policy_email");
        assert_eq!(checks[0].status, CheckStatus::Failure);
    }

    #[test]
    fn a_policy_with_no_rules_is_empty() {
        assert!(Policy::default().is_empty());
    }
}
