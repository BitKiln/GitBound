use crate::{
    config::{Config, Profile},
    error::GitBoundError,
    git::Git,
    github::GitHub,
    process::Runner,
    remote::{RemoteInfo, RemoteProtocol},
    ssh::{self, SshIdentity},
};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum CheckStatus {
    Ok,
    Warning,
    Failure,
    Unverified,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CheckItem {
    pub id: String,
    pub status: CheckStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actual: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum OverallStatus {
    Ok,
    Warning,
    Failure,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckReport {
    pub repository: String,
    pub profile: Option<String>,
    pub remote: Option<RemoteInfo>,
    pub overall: OverallStatus,
    pub checks: Vec<CheckItem>,
}

impl CheckReport {
    /// The roll-up rule, in one place: any failure fails, anything unresolved
    /// warns, otherwise ok.
    pub fn overall_of(checks: &[CheckItem]) -> OverallStatus {
        if checks
            .iter()
            .any(|check| check.status == CheckStatus::Failure)
        {
            OverallStatus::Failure
        } else if checks
            .iter()
            .any(|check| matches!(check.status, CheckStatus::Warning | CheckStatus::Unverified))
        {
            OverallStatus::Warning
        } else {
            OverallStatus::Ok
        }
    }

    /// Append checks produced outside the core engine — repository policy, for
    /// instance — and re-derive the verdict. Callers must not set `overall`
    /// themselves; the roll-up rule lives here so every producer agrees.
    pub fn extend(&mut self, extra: impl IntoIterator<Item = CheckItem>) {
        self.checks.extend(extra);
        self.overall = Self::overall_of(&self.checks);
    }

    /// Whether this report permits the operation it gates.
    ///
    /// `Unverified` blocks, because a check that could not be completed is not
    /// evidence that anything is correct. The single exception is an
    /// `unverified` **binding**, which only `verify` produces: a binding is a
    /// property of the developer's machine, and a pipeline checkout has none.
    /// Saying so is not the same as failing to perform a check that was needed,
    /// and treating it as fatal made `verify` impossible to pass in the place
    /// it was written for. Every other unverified check stays fatal.
    pub fn enforceable(&self) -> bool {
        !self.checks.iter().any(|check| match check.status {
            CheckStatus::Failure => true,
            CheckStatus::Unverified => check.id != BINDING,
            CheckStatus::Ok | CheckStatus::Warning => false,
        })
    }

    pub fn render_human(&self) -> String {
        let mut output = format!(
            "Repository: {}\nProfile:    {}\n",
            self.repository,
            self.profile.as_deref().unwrap_or("(none)")
        );
        if let Some(remote) = &self.remote {
            output.push_str(&format!(
                "Remote:     {} ({}/{})\n",
                remote.url, remote.owner, remote.repository
            ));
        }
        output.push('\n');
        for check in &self.checks {
            output.push_str(&format!(
                "[{:<10}] {}\n",
                format!("{:?}", check.status).to_uppercase(),
                check.message
            ));
        }
        output.push_str(&format!("\nStatus: {:?}\n", self.overall).to_uppercase());
        output
    }
}

/// The id of the binding check, named because [`CheckReport::enforceable`]
/// singles it out.
const BINDING: &str = "binding";

pub struct CheckOptions<'a> {
    pub remote_name: &'a str,
    pub network: bool,
    /// Whether a repository-local binding is something this caller can expect.
    ///
    /// True on a developer's machine, where an unbound repository is a
    /// misconfiguration. False for `verify`, which runs against a pipeline
    /// checkout that has no user configuration to bind against — there, the
    /// question the run exists to answer is the repository's committed policy,
    /// and an absent binding is reported rather than treated as a fault.
    pub expect_binding: bool,
}

pub fn inspect(
    runner: &dyn Runner,
    config: &Config,
    options: CheckOptions<'_>,
) -> Result<CheckReport, GitBoundError> {
    inspect_with_git(runner, config, Git::new(runner), options)
}

pub fn inspect_at(
    runner: &dyn Runner,
    config: &Config,
    path: &Path,
    options: CheckOptions<'_>,
) -> Result<CheckReport, GitBoundError> {
    inspect_with_git(runner, config, Git::at(runner, path), options)
}

fn inspect_with_git(
    runner: &dyn Runner,
    config: &Config,
    git: Git<'_>,
    options: CheckOptions<'_>,
) -> Result<CheckReport, GitBoundError> {
    let root = git.ensure_repo()?;
    // Read the effective (not just repository-local) profile binding.
    // Directory-scoped includeIf fragments set gitbound.profile, so
    // status/check must honour those to report accurate state. The lookup also
    // falls back to the pre-rename key, so a repository bound by GitPersona
    // reads as bound rather than silently unbound.
    let bound = git.binding_profile(false)?;
    let remote = git.remote(options.remote_name)?;
    let mut checks = Vec::new();

    let Some(profile_name) = bound.clone() else {
        if options.expect_binding {
            checks.push(item(
                BINDING,
                CheckStatus::Failure,
                None,
                None,
                "repository is not bound to a GitBound profile",
            ));
        } else {
            checks.push(item(
                BINDING,
                CheckStatus::Unverified,
                None,
                None,
                "repository is not bound to a GitBound profile; a pipeline checkout has none, so identity is judged against the committed policy alone",
            ));
        }
        // Report what is on disk even with no profile to compare it against.
        // Repository policy is written in absolute terms — an allowed domain, an
        // allowed host — so it can still be applied here, and `policy.evaluate`
        // reads these very items. Returning early instead left it with nothing
        // to read, so `policy_email` reported "no Git author email is
        // configured" for a repository that had one and was violating the rule.
        observe(
            &mut checks,
            &git,
            "git_email",
            "user.email",
            "Git author email",
        )?;
        observe(
            &mut checks,
            &git,
            "git_name",
            "user.name",
            "Git author name",
        )?;
        observe(
            &mut checks,
            &git,
            "signing_format",
            "gpg.format",
            "commit signing format",
        )?;
        observe(
            &mut checks,
            &git,
            "commit_signing",
            "commit.gpgSign",
            "commit signing",
        )?;
        if let Some(remote) = &remote {
            check_transport(&mut checks, remote);
        }
        return Ok(finish(root.display().to_string(), None, remote, checks));
    };
    checks.push(item(
        "binding",
        CheckStatus::Ok,
        Some(profile_name.clone()),
        Some(profile_name.clone()),
        "repository binding is present",
    ));

    let Some(profile) = config.profiles.get(&profile_name) else {
        checks.push(item(
            "profile",
            CheckStatus::Failure,
            Some(profile_name.clone()),
            None,
            "bound profile does not exist in the user configuration",
        ));
        return Ok(finish(
            root.display().to_string(),
            Some(profile_name),
            remote,
            checks,
        ));
    };
    check_equal(
        &mut checks,
        "git_name",
        &profile.git_name,
        git.get("user.name", false)?.as_deref(),
        "Git author name",
    );
    check_equal(
        &mut checks,
        "git_email",
        &profile.git_email,
        git.get("user.email", false)?.as_deref(),
        "Git author email",
    );
    if let Some(signing_key) = &profile.signing_key {
        check_equal(
            &mut checks,
            "signing_key",
            signing_key,
            git.get("user.signingKey", false)?.as_deref(),
            "commit signing key",
        );
        check_equal(
            &mut checks,
            "signing_format",
            profile.signing_format.as_git_value(),
            git.get("gpg.format", false)?.as_deref(),
            "commit signing format",
        );
        if profile.require_signing {
            check_equal(
                &mut checks,
                "commit_signing",
                "true",
                git.get("commit.gpgSign", false)?.as_deref(),
                "required commit signing",
            );
        }
    }

    match &remote {
        None => checks.push(item(
            "remote",
            CheckStatus::Warning,
            Some(options.remote_name.into()),
            None,
            "configured remote is not available",
        )),
        Some(remote) => inspect_remote(runner, &git, profile, remote, options.network, &mut checks),
    }
    Ok(finish(
        root.display().to_string(),
        Some(profile_name),
        remote,
        checks,
    ))
}

fn inspect_remote(
    runner: &dyn Runner,
    git: &Git<'_>,
    profile: &Profile,
    remote: &RemoteInfo,
    network: bool,
    checks: &mut Vec<CheckItem>,
) {
    // An SSH remote is expected to carry the profile's SSH alias, and every
    // other transport the real host. Comparing both against a single field is
    // what made alias users look permanently misconfigured.
    let expected_host = match remote.protocol {
        RemoteProtocol::Ssh => profile.ssh_host(),
        RemoteProtocol::Https | RemoteProtocol::Http => profile.hostname.as_str(),
    };
    check_equal_case_insensitive(
        checks,
        "hostname",
        expected_host,
        Some(&remote.hostname),
        "remote hostname",
    );
    check_transport(checks, remote);
    if profile.allowed_owners.is_empty() {
        checks.push(item(
            "owner",
            CheckStatus::Ok,
            None,
            Some(remote.owner.clone()),
            "remote owner is reported; this profile has no owner restriction",
        ));
    } else if profile
        .allowed_owners
        .iter()
        .any(|owner| owner.eq_ignore_ascii_case(&remote.owner))
    {
        checks.push(item(
            "owner",
            CheckStatus::Ok,
            Some(profile.allowed_owners.join(", ")),
            Some(remote.owner.clone()),
            "remote owner is allowed",
        ));
    } else {
        checks.push(item(
            "owner",
            CheckStatus::Failure,
            Some(profile.allowed_owners.join(", ")),
            Some(remote.owner.clone()),
            "remote owner is not allowed by this profile",
        ));
    }

    if matches!(remote.protocol, RemoteProtocol::Ssh) {
        match Git::expected_ssh_command(profile) {
            Ok(expected) => check_equal(
                checks,
                "ssh_command",
                &expected,
                git.get("core.sshCommand", false).ok().flatten().as_deref(),
                "repository SSH command",
            ),
            Err(error) => checks.push(item(
                "ssh_command",
                CheckStatus::Failure,
                None,
                None,
                error.to_string(),
            )),
        }
    }

    check_credential_helper(checks, git, profile, remote);

    if !network {
        return;
    }
    let github = GitHub::new(runner);
    match github.active_account(&profile.hostname) {
        Ok(Some(active)) if active.eq_ignore_ascii_case(&profile.github_user) => checks.push(item(
            "github_cli",
            CheckStatus::Ok,
            Some(profile.github_user.clone()),
            Some(active),
            "GitHub CLI account matches the profile",
        )),
        Ok(Some(active)) => checks.push(item(
            "github_cli",
            CheckStatus::Failure,
            Some(profile.github_user.clone()),
            Some(active),
            "GitHub CLI account does not match the profile",
        )),
        Ok(None) => checks.push(item(
            "github_cli",
            CheckStatus::Unverified,
            Some(profile.github_user.clone()),
            None,
            "GitHub CLI has no active account for this host",
        )),
        Err(error) => checks.push(item(
            "github_cli",
            CheckStatus::Unverified,
            Some(profile.github_user.clone()),
            None,
            format!("could not verify GitHub CLI identity: {error}"),
        )),
    }

    match remote.protocol {
        RemoteProtocol::Ssh => match ssh::verify(runner, profile) {
            Ok(SshIdentity::Verified(user)) => checks.push(item(
                "ssh_identity",
                CheckStatus::Ok,
                Some(profile.github_user.clone()),
                Some(user),
                "SSH authenticates as the expected GitHub user",
            )),
            Ok(SshIdentity::Rejected(user)) => checks.push(item(
                "ssh_identity",
                CheckStatus::Failure,
                Some(profile.github_user.clone()),
                Some(user),
                "SSH authenticates as a different GitHub user",
            )),
            Ok(SshIdentity::Unavailable(reason)) => checks.push(item(
                "ssh_identity",
                CheckStatus::Unverified,
                Some(profile.github_user.clone()),
                None,
                format!("could not verify SSH identity: {reason}"),
            )),
            Err(error) => checks.push(item(
                "ssh_identity",
                CheckStatus::Unverified,
                Some(profile.github_user.clone()),
                None,
                format!("could not verify SSH identity: {error}"),
            )),
        },
        RemoteProtocol::Https | RemoteProtocol::Http => {}
    }
}

/// Whether pushing over HTTPS would authenticate as the profile.
///
/// This reads `git config`, so unlike the two checks above it costs nothing and
/// works offline — which is why it sits outside the network gate. Leaving it
/// behind that gate meant `--offline` and the pre-commit hook dropped a check
/// they could perfectly well have made.
fn check_credential_helper(
    checks: &mut Vec<CheckItem>,
    git: &Git<'_>,
    profile: &Profile,
    remote: &RemoteInfo,
) {
    let scheme = match remote.protocol {
        RemoteProtocol::Http => "http",
        RemoteProtocol::Https => "https",
        RemoteProtocol::Ssh => return,
    };
    match git.has_compatible_credential_helper(scheme, &profile.hostname) {
        Ok(true) => checks.push(item(
            "credential_helper",
            CheckStatus::Ok,
            Some("gh auth git-credential".into()),
            Some("configured".into()),
            "GitHub CLI credential helper is configured",
        )),
        Ok(false) => checks.push(item(
            "credential_helper",
            CheckStatus::Unverified,
            Some("gh auth git-credential".into()),
            None,
            "HTTPS push identity is unverified; run gh auth setup-git for this host",
        )),
        Err(error) => checks.push(item(
            "credential_helper",
            CheckStatus::Unverified,
            None,
            None,
            format!("could not inspect Git credential helper: {error}"),
        )),
    }
}

/// Cleartext HTTP is wrong regardless of which profile a repository is bound
/// to, or whether it is bound at all, so this is shared by both paths.
fn check_transport(checks: &mut Vec<CheckItem>, remote: &RemoteInfo) {
    if !matches!(remote.protocol, RemoteProtocol::Http) {
        return;
    }
    checks.push(item(
        "transport",
        CheckStatus::Failure,
        Some("https or ssh".into()),
        Some("http".into()),
        format!(
            "remote uses cleartext HTTP; credentials and commits would travel unencrypted. Repoint it at {}",
            remote.as_url(RemoteProtocol::Https, &remote.hostname)
        ),
    ));
}

/// Record a Git setting without judging it.
///
/// Used only where there is no profile to compare against. The status is `ok`
/// because nothing was asserted and nothing failed — the value is evidence for
/// whatever repository policy has to say about it, in the same way an
/// unrestricted `owner` check reports the owner it found. A setting that is
/// absent is omitted entirely rather than reported as an empty string, so
/// policy can tell "no value" from "a value you dislike".
fn observe(
    checks: &mut Vec<CheckItem>,
    git: &Git<'_>,
    id: &str,
    key: &str,
    label: &str,
) -> Result<(), GitBoundError> {
    if let Some(value) = git.get(key, false)? {
        checks.push(item(
            id,
            CheckStatus::Ok,
            None,
            Some(value),
            format!("{label} is reported; no profile is bound to compare it against"),
        ));
    }
    Ok(())
}

fn check_equal(
    checks: &mut Vec<CheckItem>,
    id: &str,
    expected: &str,
    actual: Option<&str>,
    label: &str,
) {
    if actual == Some(expected) {
        checks.push(item(
            id,
            CheckStatus::Ok,
            Some(expected.into()),
            actual.map(str::to_string),
            format!("{label} matches the profile"),
        ));
    } else {
        checks.push(item(
            id,
            CheckStatus::Failure,
            Some(expected.into()),
            actual.map(str::to_string),
            format!("{label} does not match the profile"),
        ));
    }
}

fn check_equal_case_insensitive(
    checks: &mut Vec<CheckItem>,
    id: &str,
    expected: &str,
    actual: Option<&str>,
    label: &str,
) {
    if actual.is_some_and(|value| value.eq_ignore_ascii_case(expected)) {
        checks.push(item(
            id,
            CheckStatus::Ok,
            Some(expected.into()),
            actual.map(str::to_string),
            format!("{label} matches the profile"),
        ));
    } else {
        checks.push(item(
            id,
            CheckStatus::Failure,
            Some(expected.into()),
            actual.map(str::to_string),
            format!("{label} does not match the profile"),
        ));
    }
}

fn item(
    id: &str,
    status: CheckStatus,
    expected: Option<String>,
    actual: Option<String>,
    message: impl Into<String>,
) -> CheckItem {
    CheckItem {
        id: id.into(),
        status,
        expected,
        actual,
        message: message.into(),
    }
}

fn finish(
    repository: String,
    profile: Option<String>,
    remote: Option<RemoteInfo>,
    checks: Vec<CheckItem>,
) -> CheckReport {
    let overall = CheckReport::overall_of(&checks);
    CheckReport {
        repository,
        profile,
        remote,
        overall,
        checks,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unverified_is_not_enforceable() {
        let report = finish(
            "repo".into(),
            None,
            None,
            vec![item(
                "network",
                CheckStatus::Unverified,
                None,
                None,
                "offline",
            )],
        );
        assert!(!report.enforceable());
        assert_eq!(report.overall, OverallStatus::Warning);
    }
}
