use crate::{
    cli::{CloneArgs, CloneProtocol},
    config::ConfigStore,
    error::GitBoundError,
    git::Git,
    github::GitHub,
    process::Runner,
    remote::{RemoteProtocol, parse_repository},
};
use std::{
    env,
    ffi::OsString,
    path::{Path, PathBuf},
    time::Duration,
};

const CLONE_TIMEOUT: Duration = Duration::from_secs(300);

/// What a clone needs, independent of how it was asked for.
///
/// `CloneArgs` is a clap type and carries clap's conventions; the desktop app
/// has neither. Splitting the request out is what lets both reach the same
/// code instead of the app growing a second, subtly different clone.
pub struct CloneRequest<'a> {
    pub profile: &'a str,
    pub repository: &'a str,
    pub directory: Option<&'a Path>,
    pub protocol: Option<RemoteProtocol>,
    pub remote: &'a str,
    /// Leave the active GitHub CLI account alone. The clone then only succeeds
    /// if that account already matches the profile, which is checked below.
    pub no_switch: bool,
}

pub fn execute(
    args: CloneArgs,
    store: &ConfigStore,
    runner: &dyn Runner,
) -> Result<u8, GitBoundError> {
    let absolute = clone(
        &CloneRequest {
            profile: &args.profile,
            repository: &args.repository,
            directory: args.directory.as_deref(),
            protocol: args.protocol.map(|value| match value {
                CloneProtocol::Ssh => RemoteProtocol::Ssh,
                CloneProtocol::Https => RemoteProtocol::Https,
            }),
            remote: &args.remote,
            no_switch: args.no_switch,
        },
        store,
        runner,
    )?;
    println!(
        "Cloned to {} and bound it to profile '{}'.",
        absolute.display(),
        args.profile
    );
    Ok(0)
}

/// Clone a repository and bind it, answering with the absolute path cloned to.
pub fn clone(
    request: &CloneRequest<'_>,
    store: &ConfigStore,
    runner: &dyn Runner,
) -> Result<PathBuf, GitBoundError> {
    let config = store.load()?;
    let profile = config.profiles.get(request.profile).ok_or_else(|| {
        GitBoundError::usage(format!("profile '{}' does not exist", request.profile))
    })?;
    let source = parse_repository(request.repository, &profile.hostname)?;
    // A shorthand `owner/repo` is parsed against the real host, but an explicit
    // URL may legitimately name either the real host or the profile's SSH
    // alias, so both are accepted here.
    if !source.hostname.eq_ignore_ascii_case(&profile.hostname)
        && !source.hostname.eq_ignore_ascii_case(profile.ssh_host())
    {
        return Err(GitBoundError::usage(format!(
            "repository host '{}' does not match profile host '{}'",
            source.hostname, profile.hostname
        )));
    }
    if !profile.allowed_owners.is_empty()
        && !profile
            .allowed_owners
            .iter()
            .any(|owner| owner.eq_ignore_ascii_case(&source.owner))
    {
        return Err(GitBoundError::check(format!(
            "repository owner '{}' is not allowed by profile '{}'",
            source.owner, request.profile
        )));
    }

    let protocol = match request.protocol {
        Some(protocol) => protocol,
        // No preference: SSH when the profile has a key to offer, since that is
        // the transport its identity is strongest on.
        None if profile.ssh_key.is_some() => RemoteProtocol::Ssh,
        None => RemoteProtocol::Https,
    };
    // Computed here so an unusable key fails before anything is switched or
    // written, and kept so the clone itself can use it — see below.
    let ssh_command = match protocol {
        RemoteProtocol::Ssh => Some(Git::expected_ssh_command(profile)?),
        _ => None,
    };
    validate_remote_name(request.remote)?;

    let destination = match request.directory {
        Some(path) => path.to_path_buf(),
        None => PathBuf::from(&source.repository),
    };
    if destination.exists() {
        return Err(GitBoundError::usage(format!(
            "clone destination already exists: {}",
            destination.display()
        )));
    }
    let github = GitHub::new(runner);
    let previous = github.active_account(&profile.hostname)?;
    if request.no_switch {
        if protocol == RemoteProtocol::Https
            && !previous
                .as_deref()
                .is_some_and(|user| user.eq_ignore_ascii_case(&profile.github_user))
        {
            return Err(GitBoundError::check(format!(
                "HTTPS clone requires GitHub CLI to be active as {}; omit --no-switch to switch explicitly",
                profile.github_user
            )));
        }
    } else {
        github.switch(&profile.hostname, &profile.github_user)?;
    }

    // SSH goes through the alias so the user's SSH config picks the right key;
    // HTTPS must use the real host, which is what the credential helper and the
    // GitHub CLI are keyed by.
    let clone_url = match protocol {
        RemoteProtocol::Ssh => source.as_url(protocol, profile.ssh_host()),
        _ => source.as_url(protocol, &profile.hostname),
    };
    let clone_args = clone_arguments(
        ssh_command.as_deref(),
        request.remote,
        &clone_url,
        &destination,
    );
    let output = runner.run_git(&clone_args, CLONE_TIMEOUT)?;
    if !output.success() {
        restore_account(
            &github,
            &profile.hostname,
            previous.as_deref(),
            request.no_switch,
        )?;
        return Err(GitBoundError::dependency(format!(
            "git clone failed: {}",
            output.combined().trim()
        )));
    }

    let absolute = if destination.is_absolute() {
        destination
    } else {
        env::current_dir()
            .map_err(|error| {
                GitBoundError::dependency(format!("could not resolve clone destination: {error}"))
            })?
            .join(destination)
    };
    let git = Git::at(runner, &absolute);
    let remote = git.remote(request.remote)?;
    if let Err(error) = git.bind(request.profile, profile, remote.as_ref(), false, None) {
        let _ = std::fs::remove_dir_all(&absolute);
        restore_account(
            &github,
            &profile.hostname,
            previous.as_deref(),
            request.no_switch,
        )?;
        return Err(GitBoundError::dependency(format!(
            "binding to profile '{}' failed; the cloned directory has been removed: {error}",
            request.profile
        )));
    }

    Ok(absolute)
}

/// The `git` arguments for the clone itself.
///
/// `bind` sets `core.sshCommand` on the repository, but that happens after the
/// clone, so the fetch would otherwise authenticate with whatever key the
/// ambient SSH config offers for this host — which, for a user whose profiles
/// exist precisely because they hold several accounts, is as likely to be the
/// wrong one as the right one. Passing it here means the first network call is
/// already made as the profile, and a mismatched key fails loudly instead of
/// quietly cloning as somebody else.
fn clone_arguments(
    ssh_command: Option<&str>,
    remote: &str,
    url: &str,
    destination: &std::path::Path,
) -> Vec<OsString> {
    let mut args = Vec::new();
    if let Some(command) = ssh_command {
        args.push(OsString::from("-c"));
        args.push(OsString::from(format!("core.sshCommand={command}")));
    }
    args.extend([
        OsString::from("clone"),
        OsString::from("--origin"),
        OsString::from(remote),
        OsString::from(url),
        destination.to_path_buf().into_os_string(),
    ]);
    args
}

fn restore_account(
    github: &GitHub<'_>,
    hostname: &str,
    previous: Option<&str>,
    no_switch: bool,
) -> Result<(), GitBoundError> {
    if !no_switch && let Some(previous) = previous {
        github.switch(hostname, previous)?;
    }
    Ok(())
}

fn validate_remote_name(name: &str) -> Result<(), GitBoundError> {
    if !name.is_empty()
        && name
            .chars()
            .all(|value| value.is_ascii_alphanumeric() || matches!(value, '.' | '_' | '-'))
    {
        Ok(())
    } else {
        Err(GitBoundError::usage(
            "remote name may contain only letters, numbers, '.', '_' and '-'",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_remote_names() {
        assert!(validate_remote_name("origin").is_ok());
        assert!(validate_remote_name("bad name").is_err());
    }

    /// `-c` has to precede the subcommand: `git clone -c ...` is not the same
    /// command, and the profile's key would go unused.
    #[test]
    fn an_ssh_clone_authenticates_as_the_profile() {
        let args = clone_arguments(
            Some("ssh -i 'key'"),
            "origin",
            "git@work.github.com:org/repo.git",
            std::path::Path::new("repo"),
        );
        assert_eq!(args[0], OsString::from("-c"));
        assert_eq!(args[1], OsString::from("core.sshCommand=ssh -i 'key'"));
        assert_eq!(args[2], OsString::from("clone"));
    }

    #[test]
    fn an_https_clone_overrides_nothing() {
        let args = clone_arguments(
            None,
            "origin",
            "https://github.com/org/repo.git",
            std::path::Path::new("repo"),
        );
        assert_eq!(args[0], OsString::from("clone"));
        assert!(!args.iter().any(|arg| arg == "-c"));
    }
}
