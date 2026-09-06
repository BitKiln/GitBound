use crate::{error::GitBoundError, git::Git, paths::display_path, process::Runner};
use std::{env, fs, path::PathBuf};

/// Marker written into hooks GitBound creates. It is deliberately
/// version-free: `uninstall` and `status` recognise a hook by this line, so a
/// version-stamped marker would orphan every hook written by an earlier
/// release, which wrote `"# Managed by GitBound v0.1"` — a string this marker
/// is a prefix of.
const MARKER: &str = "# Managed by GitBound";
/// The marker written before the rename. Recognised, but never written: a hook
/// carrying it is unambiguously ours, and refusing to see it would leave the
/// user a hook that `uninstall` will not remove and `status` calls absent.
const LEGACY_MARKER: &str = "# Managed by GitPersona";

fn is_managed(contents: &str) -> bool {
    contents.contains(MARKER) || contents.contains(LEGACY_MARKER)
}

pub struct HookManager<'a> {
    git: Git<'a>,
}

/// Whether each managed hook is in place, for a caller that renders rather than
/// prints — the desktop app, which cannot use the human string below.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct HookState {
    pub pre_commit: String,
    pub pre_push: String,
}

impl<'a> HookManager<'a> {
    pub fn new(runner: &'a dyn Runner) -> Self {
        Self {
            git: Git::new(runner),
        }
    }

    /// A manager for a repository other than the current directory, which is
    /// how every caller outside the CLI reaches one.
    pub fn at(runner: &'a dyn Runner, repository: &std::path::Path) -> Self {
        Self {
            git: Git::at(runner, repository),
        }
    }

    fn paths(&self) -> Result<(PathBuf, PathBuf), GitBoundError> {
        if self.git.get("core.hooksPath", false)?.is_some() {
            return Err(GitBoundError::usage(
                "core.hooksPath is configured; GitBound will not modify or chain that hook setup",
            ));
        }
        let hooks = self.git.common_dir()?.join("hooks");
        Ok((hooks.join("pre-commit"), hooks.join("pre-push")))
    }

    pub fn install(&self) -> Result<(), GitBoundError> {
        let (commit, push) = self.paths()?;
        for path in [&commit, &push] {
            if path.exists() {
                return Err(GitBoundError::usage(format!(
                    "hook already exists; refusing to replace {}",
                    path.display()
                )));
            }
        }
        let parent = commit.parent().expect("hook has parent");
        fs::create_dir_all(parent).map_err(|e| {
            GitBoundError::dependency(format!("could not create hooks directory: {e}"))
        })?;
        let exe = resolve_executable()?;
        let pre_commit = format!("#!/bin/sh\n{MARKER}\nexec {exe} check --hook pre-commit\n");
        let pre_push = format!(
            "#!/bin/sh\n{MARKER}\nexec {exe} check --hook pre-push --remote \"${{1:-origin}}\"\n"
        );
        write_hook(&commit, &pre_commit)?;
        if let Err(error) = write_hook(&push, &pre_push) {
            let _ = fs::remove_file(&commit);
            return Err(error);
        }
        Ok(())
    }

    pub fn state(&self) -> Result<HookState, GitBoundError> {
        let (commit, push) = self.paths()?;
        Ok(HookState {
            pre_commit: hook_state(&commit).into(),
            pre_push: hook_state(&push).into(),
        })
    }

    pub fn status(&self) -> Result<String, GitBoundError> {
        let state = self.state()?;
        Ok(format!(
            "pre-commit: {}\npre-push:   {}",
            state.pre_commit, state.pre_push
        ))
    }

    pub fn uninstall(&self) -> Result<(), GitBoundError> {
        let (commit, push) = self.paths()?;
        for path in [&commit, &push] {
            if path.exists() {
                let contents = fs::read_to_string(path).map_err(|e| {
                    GitBoundError::dependency(format!("could not read {}: {e}", path.display()))
                })?;
                if !is_managed(&contents) {
                    return Err(GitBoundError::usage(format!(
                        "{} is not a GitBound-managed hook; refusing to remove it",
                        path.display()
                    )));
                }
            }
        }
        for path in [&commit, &push] {
            if path.exists() {
                fs::remove_file(path).map_err(|e| {
                    GitBoundError::dependency(format!("could not remove {}: {e}", path.display()))
                })?;
            }
        }
        Ok(())
    }
}

fn write_hook(path: &std::path::Path, contents: &str) -> Result<(), GitBoundError> {
    // `create_new` closes the gap between the caller's `exists()` check and
    // this write: a hook that appears in between must never be overwritten.
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::AlreadyExists {
                GitBoundError::usage(format!(
                    "hook already exists; refusing to replace {}",
                    path.display()
                ))
            } else {
                GitBoundError::dependency(format!("could not write {}: {e}", path.display()))
            }
        })?;
    std::io::Write::write_all(&mut file, contents.as_bytes()).map_err(|e| {
        GitBoundError::dependency(format!("could not write {}: {e}", path.display()))
    })?;
    drop(file);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).map_err(|e| {
            GitBoundError::dependency(format!("could not make {} executable: {e}", path.display()))
        })?;
    }
    Ok(())
}

fn hook_state(path: &std::path::Path) -> &'static str {
    match fs::read_to_string(path) {
        Ok(contents) if is_managed(&contents) => "installed",
        Ok(_) => "occupied by another hook",
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => "not installed",
        Err(_) => "unreadable",
    }
}

/// The name of the command-line binary, without any platform extension.
const CLI_NAME: &str = "gitbound";

/// The `gitbound` CLI a hook should invoke.
///
/// `current_exe` is the right answer for the CLI itself and the wrong one for
/// the desktop app: a hook running `gitbound-desktop` would open a window on
/// every commit rather than check anything. So the running executable is used
/// only when it *is* the CLI; otherwise the CLI is looked for beside it and
/// then on `PATH`, and if it is nowhere the install fails and says so, which is
/// far better than writing a hook that cannot work.
fn resolve_executable() -> Result<String, GitBoundError> {
    let exe = env::current_exe().map_err(|e| {
        GitBoundError::dependency(format!(
            "could not determine the gitbound executable path: {e}"
        ))
    })?;
    if exe.file_stem().is_some_and(|stem| stem == CLI_NAME) {
        return Ok(quote(&std::fs::canonicalize(&exe).unwrap_or(exe)));
    }
    let filename = format!("{CLI_NAME}{}", env::consts::EXE_SUFFIX);
    if let Some(sibling) = exe.parent().map(|dir| dir.join(&filename))
        && sibling.is_file()
    {
        return Ok(quote(&std::fs::canonicalize(&sibling).unwrap_or(sibling)));
    }
    if let Some(path) = env::var_os("PATH")
        && let Some(found) = env::split_paths(&path)
            .map(|dir| dir.join(&filename))
            .find(|candidate| candidate.is_file())
    {
        return Ok(quote(&std::fs::canonicalize(&found).unwrap_or(found)));
    }
    Err(GitBoundError::dependency(format!(
        "could not find the {CLI_NAME} command-line tool, which the hooks run. \
         Install it alongside this application or on PATH, then try again."
    )))
}

/// Shell-escape a path for use in the `sh` script a hook is.
///
/// The path has been canonicalized, so on Windows it arrives verbatim —
/// `\\?\C:\...`. Git for Windows' `sh` happens to run that, but no other shell
/// is obliged to, and it is unreadable to anyone who opens the hook to see what
/// it does, so the prefix goes before the quoting.
fn quote(path: &std::path::Path) -> String {
    format!("'{}'", display_path(path).replace('\'', "'\\''"))
}
