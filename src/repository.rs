use crate::{
    api::{RepositoryLocalStatus, RepositoryScanEvent, RepositorySummary},
    config::Config,
    error::GitBoundError,
    git::Git,
    process::Runner,
};
use std::{
    collections::{BTreeSet, HashSet},
    fs,
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};

/// Bounds on a single root's walk. Without them a deep or pathological tree
/// stalls the desktop scan, which only the cancel flag can interrupt.
const MAX_SCAN_DEPTH: usize = 24;
const MAX_SCANNED_DIRECTORIES: usize = 20_000;

const SKIPPED_DIRECTORIES: &[&str] = &[
    ".git",
    "node_modules",
    "target",
    "dist",
    "build",
    "vendor",
    ".cache",
];

pub fn summarize(
    runner: &dyn Runner,
    config: &Config,
    path: &Path,
) -> Result<RepositorySummary, GitBoundError> {
    let git = Git::at(runner, path);
    let root = git.ensure_repo()?;
    let bound_profile = git.binding_profile(true)?;
    let remote = git.remote("origin")?;
    let (status, detail) = match bound_profile.as_deref() {
        None => (RepositoryLocalStatus::Unbound, None),
        Some(name) => match config.profiles.get(name) {
            None => (
                RepositoryLocalStatus::MissingProfile,
                Some(format!("bound profile '{name}' does not exist")),
            ),
            Some(profile) => match git.binding_drifted(profile, remote.as_ref()) {
                Ok(true) => (
                    RepositoryLocalStatus::Drifted,
                    Some("managed Git settings differ from the profile".into()),
                ),
                Ok(false) => (RepositoryLocalStatus::Bound, None),
                Err(error) => (RepositoryLocalStatus::Unavailable, Some(error.to_string())),
            },
        },
    };
    Ok(RepositorySummary {
        name: root.file_name().map_or_else(
            || root.display().to_string(),
            |name| name.to_string_lossy().into(),
        ),
        path: root,
        bound_profile,
        git_name: git.get("user.name", false)?,
        git_email: git.get("user.email", false)?,
        remote,
        status,
        detail,
    })
}

pub fn scan_roots(
    runner: &dyn Runner,
    config: &Config,
    cancel: &AtomicBool,
    mut emit: impl FnMut(RepositoryScanEvent),
) -> Vec<RepositorySummary> {
    emit(RepositoryScanEvent::Started {
        roots: config.repository_roots.len(),
    });
    let mut repositories = Vec::new();
    let mut seen = BTreeSet::new();
    for root in &config.repository_roots {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        emit(RepositoryScanEvent::RootStarted { root: root.clone() });
        if !root.is_dir() {
            emit(RepositoryScanEvent::RootUnavailable {
                root: root.clone(),
                message: "approved root no longer exists or is not a directory".into(),
            });
            continue;
        }
        let mut stack = vec![(root.clone(), 0usize)];
        let mut visited = HashSet::new();
        let mut exhausted = false;
        while let Some((directory, depth)) = stack.pop() {
            if cancel.load(Ordering::Relaxed) {
                break;
            }
            if visited.len() >= MAX_SCANNED_DIRECTORIES {
                exhausted = true;
                break;
            }
            // `canonicalize` resolves symlinks, so a link into an already
            // visited tree collapses onto the same key and is skipped here.
            let canonical = match fs::canonicalize(&directory) {
                Ok(path) => path,
                Err(_) => continue,
            };
            if !visited.insert(path_key(&canonical)) {
                continue;
            }
            let marker = canonical.join(".git");
            if marker.is_dir() || marker.is_file() {
                let key = path_key(&canonical);
                if seen.insert(key)
                    && let Ok(summary) = summarize(runner, config, &canonical)
                {
                    emit(RepositoryScanEvent::RepositoryFound {
                        repository: Box::new(summary.clone()),
                    });
                    repositories.push(summary);
                }
                continue;
            }
            let entries = match fs::read_dir(&canonical) {
                Ok(entries) => entries,
                Err(_) => continue,
            };
            for entry in entries.flatten() {
                let path = entry.path();
                let file_type = match entry.file_type() {
                    Ok(file_type) => file_type,
                    Err(_) => continue,
                };
                if !file_type.is_dir() || file_type.is_symlink() {
                    continue;
                }
                let name = entry.file_name().to_string_lossy().into_owned();
                if SKIPPED_DIRECTORIES
                    .iter()
                    .any(|skip| name.eq_ignore_ascii_case(skip))
                {
                    continue;
                }
                if depth < MAX_SCAN_DEPTH {
                    stack.push((path, depth + 1));
                }
            }
        }
        if exhausted {
            emit(RepositoryScanEvent::RootUnavailable {
                root: root.clone(),
                message: format!(
                    "stopped after {MAX_SCANNED_DIRECTORIES} directories; narrow the approved root"
                ),
            });
        }
    }
    repositories.sort_by_key(|repository| path_key(&repository.path));
    emit(RepositoryScanEvent::Finished {
        repositories: repositories.len(),
        cancelled: cancel.load(Ordering::Relaxed),
    });
    repositories
}

fn path_key(path: &Path) -> String {
    let value = path.to_string_lossy().to_string();
    if cfg!(windows) {
        value.to_ascii_lowercase()
    } else {
        value
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::process::SystemRunner;
    use std::process::Command;

    #[test]
    fn skipped_directories_cover_generated_trees() {
        for name in [".git", "node_modules", "target", "vendor"] {
            assert!(SKIPPED_DIRECTORIES.contains(&name));
        }
    }

    #[test]
    fn scan_deduplicates_roots_and_skips_generated_trees() {
        let temp = tempfile::tempdir().unwrap();
        let repository = temp.path().join("project");
        let generated = temp.path().join("node_modules").join("private");
        fs::create_dir_all(&repository).unwrap();
        fs::create_dir_all(&generated).unwrap();
        for path in [&repository, &generated] {
            assert!(
                Command::new("git")
                    .args(["init", "--quiet"])
                    .current_dir(path)
                    .status()
                    .unwrap()
                    .success()
            );
        }
        let config = Config {
            repository_roots: vec![temp.path().into(), temp.path().into()],
            ..Config::default()
        };
        let repositories = scan_roots(&SystemRunner, &config, &AtomicBool::new(false), |_| {});
        assert_eq!(repositories.len(), 1);
        assert_eq!(repositories[0].name, "project");
    }

    #[test]
    fn cancelled_scan_finishes_without_descending() {
        let temp = tempfile::tempdir().unwrap();
        let config = Config {
            repository_roots: vec![temp.path().into()],
            ..Config::default()
        };
        let cancel = AtomicBool::new(true);
        let mut events = Vec::new();
        let repositories = scan_roots(&SystemRunner, &config, &cancel, |event| events.push(event));
        assert!(repositories.is_empty());
        assert!(matches!(
            events.last(),
            Some(RepositoryScanEvent::Finished {
                cancelled: true,
                ..
            })
        ));
    }
}
