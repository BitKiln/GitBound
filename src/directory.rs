use crate::{
    cli::DirectoryCommand,
    config::{ConfigStore, DirectoryRule, Profile, expand_path},
    error::GitBoundError,
    git::Git,
    paths::display_path,
    process::{Runner, os_args},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    fs,
    io::Write,
    path::{Path, PathBuf},
    time::Duration,
};

const TIMEOUT: Duration = Duration::from_secs(15);
/// Marker written at the head of every generated profile fragment, and the
/// only signal that authorises deleting one. Version-free on purpose: fragments
/// written by earlier releases begin `"# Managed by GitBound v0.4"`, which
/// still matches this prefix, so they remain removable.
const MARKER: &str = "# Managed by GitBound";
/// The marker written before the rename. Recognised, but never written. An
/// unrecognised fragment is not left alone harmlessly: it stays on disk with
/// the `includeIf` rule still pointing at it, so it keeps applying an identity
/// for a profile the user has already removed.
const LEGACY_MARKER: &str = "# Managed by GitPersona";

/// Whether this file is one GitBound generated, and so is ours to delete.
fn is_generated(contents: &str) -> bool {
    contents.starts_with(MARKER) || contents.starts_with(LEGACY_MARKER)
}

/// One directory rule as it is worth showing: the profile it applies, the
/// folder it applies to, and the two Git artefacts that make it work — so a
/// user can check GitBound's claim against `git config --global --list` rather
/// than taking it on trust.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleView {
    pub profile: String,
    pub path: String,
    pub condition: String,
    pub include: String,
}

pub fn execute(
    command: DirectoryCommand,
    store: &ConfigStore,
    runner: &dyn Runner,
) -> Result<u8, GitBoundError> {
    match command {
        DirectoryCommand::Add { profile, path } => add(&profile, &path, store, runner),
        DirectoryCommand::List { json } => list(json, store),
        DirectoryCommand::Sync { profile } => {
            if let Some(profile) = profile {
                sync_profile(&profile, store)?;
                println!("Directory fragment for profile '{profile}' synchronized.");
            } else {
                sync_all(store)?;
                println!("All directory profile fragments synchronized.");
            }
            Ok(0)
        }
        DirectoryCommand::Remove { path } => remove(&path, store, runner),
    }
}

pub fn sync_profile(name: &str, store: &ConfigStore) -> Result<(), GitBoundError> {
    let config = store.load()?;
    if !config.directories.iter().any(|rule| rule.profile == name) {
        return Ok(());
    }
    let profile = config
        .profiles
        .get(name)
        .ok_or_else(|| GitBoundError::usage(format!("profile '{name}' does not exist")))?;
    write_fragment(store, name, profile)
}

pub fn rename_profile(
    old_name: &str,
    new_name: &str,
    store: &ConfigStore,
    runner: &dyn Runner,
) -> Result<(), GitBoundError> {
    let old_fragment = fragment_path(store, old_name)?;
    let new_fragment = fragment_path(store, new_name)?;
    let old_value = git_path(&old_fragment);
    let new_value = git_path(&new_fragment);

    // The caller (`GitBoundService::rename_profile`) has already committed the
    // rename to the config, so the rules to repoint are the ones now carrying
    // the NEW name. Filtering on `old_name` here matched nothing, which left the
    // stale include pointing at a fragment that was then never deleted.
    let config = store.load()?;
    let rules_to_update: Vec<DirectoryRule> = config
        .directories
        .iter()
        .filter(|rule| rule.profile == new_name)
        .cloned()
        .collect();

    for rule in &rules_to_update {
        let key = include_key(&rule.path);
        let _ = global_remove(runner, &key, &old_value);
        global_add(runner, &key, &new_value)?;
    }

    if !rules_to_update.is_empty()
        && old_fragment.exists()
        && let Ok(contents) = fs::read_to_string(&old_fragment)
        && is_generated(&contents)
    {
        let _ = fs::remove_file(&old_fragment);
    }
    sync_profile(new_name, store)?;
    Ok(())
}

fn sync_all(store: &ConfigStore) -> Result<(), GitBoundError> {
    let config = store.load()?;
    let names = config
        .directories
        .iter()
        .map(|rule| rule.profile.as_str())
        .collect::<BTreeSet<_>>();
    for name in names {
        let profile = config.profiles.get(name).ok_or_else(|| {
            GitBoundError::usage(format!(
                "directory rule references missing profile '{name}'"
            ))
        })?;
        write_fragment(store, name, profile)?;
    }
    Ok(())
}

fn add(
    profile_name: &str,
    path: &Path,
    store: &ConfigStore,
    runner: &dyn Runner,
) -> Result<u8, GitBoundError> {
    let path = add_rule(profile_name, path, store, runner)?;
    println!(
        "Directory {} now uses profile '{}' through Git includeIf.",
        display_path(&path),
        profile_name
    );
    Ok(0)
}

/// Assign `path` to a profile, answering with the canonical directory the rule
/// was actually written for — which is not always the one asked for, and is
/// what a caller has to show or store.
///
/// Split out of [`add`] so the desktop app can do the same thing without a
/// command-line's licence to print.
pub fn add_rule(
    profile_name: &str,
    path: &Path,
    store: &ConfigStore,
    runner: &dyn Runner,
) -> Result<PathBuf, GitBoundError> {
    let path = canonical_directory(path)?;
    let config = store.load()?;
    let profile = config
        .profiles
        .get(profile_name)
        .ok_or_else(|| GitBoundError::usage(format!("profile '{profile_name}' does not exist")))?;
    if let Some(rule) = config
        .directories
        .iter()
        .find(|rule| paths_equal(&rule.path, &path))
    {
        return if rule.profile == profile_name {
            Err(GitBoundError::usage("directory rule already exists"))
        } else {
            Err(GitBoundError::usage(format!(
                "directory is already assigned to profile '{}'",
                rule.profile
            )))
        };
    }

    let fragment = fragment_path(store, profile_name)?;
    write_fragment(store, profile_name, profile)?;
    let key = include_key(&path);
    let value = git_path(&fragment);
    let existing = global_values(runner, &key)?;
    if existing.iter().any(|item| item != &value) {
        return Err(GitBoundError::usage(format!(
            "Git already has a different includeIf rule for {}",
            display_path(&path)
        )));
    }
    let added = existing.is_empty();
    if added {
        global_add(runner, &key, &value)?;
    }
    if let Err(error) = store.update(|config| {
        config.directories.push(DirectoryRule {
            profile: profile_name.to_string(),
            path: path.clone(),
        });
        Ok(())
    }) {
        if added {
            let _ = global_remove(runner, &key, &value);
        }
        return Err(error);
    }
    Ok(path)
}

/// Every configured rule. Shared by `directory list` and the desktop app.
pub fn rules(store: &ConfigStore) -> Result<Vec<RuleView>, GitBoundError> {
    let config = store.load()?;
    config
        .directories
        .iter()
        .map(|rule| {
            let include = fragment_path(store, &rule.profile)?;
            Ok(RuleView {
                profile: rule.profile.clone(),
                path: display_path(&rule.path),
                condition: include_key(&rule.path),
                include: display_path(&include),
            })
        })
        .collect()
}

fn list(json: bool, store: &ConfigStore) -> Result<u8, GitBoundError> {
    let views = rules(store)?;
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&views).map_err(|error| {
                GitBoundError::dependency(format!("could not serialize directory rules: {error}"))
            })?
        );
    } else if views.is_empty() {
        println!("No directory rules configured.");
    } else {
        for view in views {
            println!("{}\t{}", view.profile, view.path);
        }
    }
    Ok(0)
}

fn remove(path: &Path, store: &ConfigStore, runner: &dyn Runner) -> Result<u8, GitBoundError> {
    let path = remove_rule(path, store, runner)?;
    println!("Directory rule for {} removed.", display_path(&path));
    Ok(0)
}

/// Drop the rule for `path`, answering with the canonical directory removed.
///
/// Removing the `includeIf` entry comes first and the config update second, so
/// a failure between them leaves a rule GitBound still knows about rather than
/// one Git is still applying invisibly.
pub fn remove_rule(
    path: &Path,
    store: &ConfigStore,
    runner: &dyn Runner,
) -> Result<PathBuf, GitBoundError> {
    let path = canonical_directory(path)?;
    let config = store.load()?;
    let rule = config
        .directories
        .iter()
        .find(|rule| paths_equal(&rule.path, &path))
        .cloned()
        .ok_or_else(|| GitBoundError::usage("directory rule does not exist"))?;
    let key = include_key(&rule.path);
    let fragment = fragment_path(store, &rule.profile)?;
    let value = git_path(&fragment);
    global_remove(runner, &key, &value)?;
    if let Err(error) = store.update(|config| {
        config
            .directories
            .retain(|candidate| !paths_equal(&candidate.path, &path));
        Ok(())
    }) {
        let _ = global_add(runner, &key, &value);
        return Err(error);
    }
    let remaining = store
        .load()?
        .directories
        .iter()
        .any(|candidate| candidate.profile == rule.profile);
    if !remaining && fragment.exists() {
        let contents = fs::read_to_string(&fragment).map_err(|error| {
            GitBoundError::dependency(format!("could not read {}: {error}", fragment.display()))
        })?;
        if is_generated(&contents) {
            fs::remove_file(&fragment).map_err(|error| {
                GitBoundError::dependency(format!(
                    "could not remove {}: {error}",
                    fragment.display()
                ))
            })?;
        }
    }
    Ok(path)
}

fn write_fragment(store: &ConfigStore, name: &str, profile: &Profile) -> Result<(), GitBoundError> {
    let path = fragment_path(store, name)?;
    let parent = path.parent().expect("fragment has parent");
    fs::create_dir_all(parent).map_err(|error| {
        GitBoundError::dependency(format!("could not create {}: {error}", parent.display()))
    })?;
    let rendered = render_fragment(name, profile)?;
    let mut temp = tempfile::NamedTempFile::new_in(parent).map_err(|error| {
        GitBoundError::dependency(format!("could not create profile fragment: {error}"))
    })?;
    temp.write_all(rendered.as_bytes())
        .and_then(|_| temp.as_file().sync_all())
        .map_err(|error| {
            GitBoundError::dependency(format!("could not write profile fragment: {error}"))
        })?;
    temp.persist(&path).map_err(|error| {
        GitBoundError::dependency(format!(
            "could not persist profile fragment: {}",
            error.error
        ))
    })?;
    Ok(())
}

fn render_fragment(name: &str, profile: &Profile) -> Result<String, GitBoundError> {
    let mut output = format!(
        "{MARKER}\n[user]\n\tname = {}\n\temail = {}\n[gitbound]\n\tprofile = {}\n\tversion = 3\n",
        quote(&profile.git_name),
        quote(&profile.git_email),
        quote(name)
    );
    if let Some(key) = &profile.signing_key {
        output.push_str(&format!("[user]\n\tsigningKey = {}\n", quote(key)));
        output.push_str(&format!(
            "[gpg]\n\tformat = {}\n",
            quote(profile.signing_format.as_git_value())
        ));
        if profile.require_signing {
            output.push_str("[commit]\n\tgpgSign = true\n");
        }
    }
    if profile.ssh_key.is_some() {
        output.push_str(&format!(
            "[core]\n\tsshCommand = {}\n",
            quote(&Git::expected_ssh_command(profile)?)
        ));
    }
    Ok(output)
}

fn quote(value: &str) -> String {
    format!(
        "\"{}\"",
        value
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('\r', "\\r")
            .replace('\n', "\\n")
            .replace('\t', "\\t")
    )
}

fn fragment_path(store: &ConfigStore, name: &str) -> Result<PathBuf, GitBoundError> {
    let parent = store
        .path()
        .parent()
        .ok_or_else(|| GitBoundError::dependency("config path has no parent directory"))?;
    Ok(parent.join("profiles").join(format!("{name}.gitconfig")))
}

fn canonical_directory(path: &Path) -> Result<PathBuf, GitBoundError> {
    let path = expand_path(path)?;
    let path = fs::canonicalize(&path).map_err(|error| {
        GitBoundError::usage(format!(
            "could not resolve directory {}: {error}",
            path.display()
        ))
    })?;
    if !path.is_dir() {
        return Err(GitBoundError::usage(format!(
            "directory rule target is not a directory: {}",
            path.display()
        )));
    }
    Ok(path)
}

fn include_key(path: &Path) -> String {
    let prefix = if cfg!(windows) {
        "gitdir/i:"
    } else {
        "gitdir:"
    };
    format!(
        "includeIf.{prefix}{}/.path",
        git_path(path).trim_end_matches('/')
    )
}

fn git_path(path: &Path) -> String {
    display_path(path).replace('\\', "/")
}

fn paths_equal(left: &Path, right: &Path) -> bool {
    if cfg!(windows) {
        left.to_string_lossy()
            .eq_ignore_ascii_case(&right.to_string_lossy())
    } else {
        left == right
    }
}

fn global_values(runner: &dyn Runner, key: &str) -> Result<Vec<String>, GitBoundError> {
    let output = runner.run_git(&os_args(&["config", "--global", "--get-all", key]), TIMEOUT)?;
    match output.code {
        Some(0) => Ok(output.stdout.lines().map(str::to_string).collect()),
        Some(1) => Ok(Vec::new()),
        _ => Err(GitBoundError::dependency(format!(
            "could not inspect global Git includeIf rule: {}",
            output.stderr.trim()
        ))),
    }
}

fn global_add(runner: &dyn Runner, key: &str, value: &str) -> Result<(), GitBoundError> {
    let output = runner.run_git(
        &os_args(&["config", "--global", "--add", key, value]),
        TIMEOUT,
    )?;
    if output.success() {
        Ok(())
    } else {
        Err(GitBoundError::dependency(format!(
            "could not add global Git includeIf rule: {}",
            output.stderr.trim()
        )))
    }
}

fn global_remove(runner: &dyn Runner, key: &str, value: &str) -> Result<(), GitBoundError> {
    let output = runner.run_git(
        &os_args(&[
            "config",
            "--global",
            "--fixed-value",
            "--unset-all",
            key,
            value,
        ]),
        TIMEOUT,
    )?;
    if matches!(output.code, Some(0 | 5)) {
        Ok(())
    } else {
        Err(GitBoundError::dependency(format!(
            "could not remove exact global Git includeIf rule: {}",
            output.stderr.trim()
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::SigningFormat;

    #[test]
    fn fragment_quotes_identity_values() {
        let profile = Profile {
            github_user: "alice".into(),
            git_name: "Alice \"Dev\"".into(),
            git_email: "alice@example.com".into(),
            hostname: "github.com".into(),
            ssh_host: None,
            ssh_key: None,
            allowed_owners: vec![],
            signing_key: Some("ABC123".into()),
            signing_format: SigningFormat::Openpgp,
            require_signing: true,
        };
        let rendered = render_fragment("work", &profile).unwrap();
        assert!(rendered.contains("Alice \\\"Dev\\\""));
        assert!(rendered.contains("gpgSign = true"));
    }
}
