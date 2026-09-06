use crate::{
    config::{Profile, expand_path},
    error::GitBoundError,
    process::Runner,
};
use regex::Regex;
use std::{ffi::OsString, time::Duration};

#[derive(Debug, PartialEq, Eq)]
pub enum SshIdentity {
    Verified(String),
    Rejected(String),
    Unavailable(String),
}

pub fn verify(runner: &dyn Runner, profile: &Profile) -> Result<SshIdentity, GitBoundError> {
    let key = profile
        .ssh_key
        .as_ref()
        .ok_or_else(|| GitBoundError::usage("profile has no SSH key"))?;
    let key = expand_path(key)?;
    let args = vec![
        OsString::from("-T"),
        OsString::from("-o"),
        OsString::from("BatchMode=yes"),
        OsString::from("-o"),
        OsString::from("ConnectTimeout=10"),
        OsString::from("-o"),
        OsString::from("IdentitiesOnly=yes"),
        OsString::from("-i"),
        key.into_os_string(),
        OsString::from(format!("git@{}", profile.ssh_host())),
    ];
    let output = runner.run("ssh", &args, Duration::from_secs(15))?;
    let combined = output.combined();
    static PATTERN: std::sync::LazyLock<Regex> =
        std::sync::LazyLock::new(|| Regex::new(r"(?i)Hi\s+([^!\s]+)!").expect("valid regex"));
    if let Some(captures) = PATTERN.captures(&combined) {
        let user = captures[1].to_string();
        return if user.eq_ignore_ascii_case(&profile.github_user) {
            Ok(SshIdentity::Verified(user))
        } else {
            Ok(SshIdentity::Rejected(user))
        };
    }
    Ok(SshIdentity::Unavailable(combined.trim().to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::process::ProcessOutput;

    struct Fake;
    impl Runner for Fake {
        fn run(
            &self,
            _: &str,
            _: &[OsString],
            _: Duration,
        ) -> Result<ProcessOutput, GitBoundError> {
            Ok(ProcessOutput { code: Some(1), stdout: String::new(), stderr: "Hi Alice! You've successfully authenticated, but GitHub does not provide shell access.".into() })
        }
    }

    #[test]
    fn github_success_message_beats_nonzero_exit() {
        let dir = tempfile::tempdir().unwrap();
        let key = dir.path().join("key");
        std::fs::write(&key, "x").unwrap();
        let profile = Profile {
            github_user: "alice".into(),
            git_name: "A".into(),
            git_email: "a@x".into(),
            hostname: "github.com".into(),
            ssh_host: None,
            ssh_key: Some(key),
            allowed_owners: vec![],
            signing_key: None,
            signing_format: crate::config::SigningFormat::Openpgp,
            require_signing: false,
        };
        assert_eq!(
            verify(&Fake, &profile).unwrap(),
            SshIdentity::Verified("Alice".into())
        );
    }
}
