// Release builds target the Windows GUI subsystem so launching GitBound does
// not open a console window. The cost is that panics and anything written to
// stderr are lost in a release build; debug builds keep the console for exactly
// that reason. Paired with CREATE_NO_WINDOW in `gitbound::process`, which
// stops each `git`/`gh`/`ssh` child from allocating a console of its own.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

// Fixture data for `?demo`; the payload itself is debug-only. See demo.rs.
mod demo;

// Holds the hand-written frontend to the Rust API it talks to. See contract.rs.
#[cfg(test)]
mod contract;

use gitbound::{
    api::{
        ApiError, DoctorReport, NamedProfile, ProfileDraft, RepositoryCiStatus,
        RepositoryScanEvent, RepositoryStatus, RepositorySummary, SshTestReport,
    },
    check::CheckReport,
    config::Profile,
    directory::RuleView,
    github::Account,
    hooks::HookState,
    process::SystemRunner,
    remote::RemoteProtocol,
    service::GitBoundService,
};
use std::{
    collections::HashSet,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};
use tauri::{AppHandle, State, ipc::Channel};
use tauri_plugin_clipboard_manager::ClipboardExt;
use tauri_plugin_dialog::DialogExt;

#[derive(Default)]
struct ScanState(Mutex<Option<Arc<AtomicBool>>>);

#[derive(Default)]
struct ApprovedPaths(Mutex<HashSet<PathBuf>>);

fn service() -> Result<GitBoundService<'static>, ApiError> {
    static RUNNER: SystemRunner = SystemRunner;
    GitBoundService::discover(&RUNNER).map_err(Into::into)
}

#[tauri::command]
async fn list_profiles() -> Result<Vec<NamedProfile>, ApiError> {
    service()?.list_profiles().map_err(Into::into)
}

#[tauri::command]
async fn create_profile(name: String, profile: Profile) -> Result<NamedProfile, ApiError> {
    service()?
        .create_profile(&name, profile)
        .map_err(Into::into)
}

#[tauri::command]
async fn update_profile(name: String, profile: Profile) -> Result<NamedProfile, ApiError> {
    service()?
        .update_profile(&name, profile)
        .map_err(Into::into)
}

#[tauri::command]
async fn remove_profile(name: String) -> Result<(), ApiError> {
    service()?.remove_profile(&name).map_err(Into::into)
}

#[tauri::command]
async fn rename_profile(old_name: String, new_name: String) -> Result<NamedProfile, ApiError> {
    service()?
        .rename_profile(&old_name, &new_name)
        .map_err(Into::into)
}

#[tauri::command]
async fn import_profile_preview(
    repository: PathBuf,
    approved: State<'_, ApprovedPaths>,
) -> Result<ProfileDraft, ApiError> {
    let resolved = ensure_approved(&repository, &approved)?;
    service()?
        .import_preview(&resolved, "origin")
        .map_err(Into::into)
}

#[tauri::command]
fn choose_folder(
    app: AppHandle,
    approved: State<'_, ApprovedPaths>,
) -> Result<Option<PathBuf>, ApiError> {
    app.dialog()
        .file()
        .blocking_pick_folder()
        .map(|path| {
            path.into_path()
                .map_err(|error| ApiError {
                    kind: "usage".into(),
                    message: error.to_string(),
                    exit_code: 2,
                    field: Some("path".into()),
                })
                .and_then(|path| {
                    let canonical = std::fs::canonicalize(&path).map_err(ApiError::from_io)?;
                    let normalized = strip_unc_prefix(&canonical);
                    approved
                        .0
                        .lock()
                        .map_err(|_| ApiError::internal("approved-folder state is unavailable"))?
                        .insert(normalized.clone());
                    Ok(normalized)
                })
        })
        .transpose()
}

#[tauri::command]
fn choose_key_file(app: AppHandle) -> Result<Option<PathBuf>, ApiError> {
    app.dialog()
        .file()
        .blocking_pick_file()
        .map(|path| {
            path.into_path().map_err(|error| ApiError {
                kind: "usage".into(),
                message: error.to_string(),
                exit_code: 2,
                field: Some("ssh_key".into()),
            })
        })
        .transpose()
}

#[tauri::command]
async fn list_repository_roots() -> Result<Vec<PathBuf>, ApiError> {
    service()?.repository_roots().map_err(Into::into)
}

#[tauri::command]
async fn add_repository_root(
    path: PathBuf,
    approved: State<'_, ApprovedPaths>,
) -> Result<PathBuf, ApiError> {
    let resolved = ensure_session_approved(&path, &approved)?;
    service()?
        .add_repository_root(&resolved)
        .map_err(Into::into)
}

#[tauri::command]
async fn remove_repository_root(path: PathBuf) -> Result<(), ApiError> {
    service()?.remove_repository_root(&path).map_err(Into::into)
}

#[tauri::command]
async fn scan_repositories(
    state: State<'_, ScanState>,
    events: Channel<RepositoryScanEvent>,
) -> Result<Vec<RepositorySummary>, ApiError> {
    let cancel = Arc::new(AtomicBool::new(false));
    {
        let mut guard = state
            .0
            .lock()
            .map_err(|_| ApiError::internal("scan-state lock poisoned"))?;
        if let Some(prev) = guard.replace(cancel.clone()) {
            prev.store(true, Ordering::Relaxed);
        }
    }
    let cancel_for_worker = cancel.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        service()?
            .scan_repositories(&cancel_for_worker, |event| {
                let _ = events.send(event);
            })
            .map_err(ApiError::from)
    })
    .await
    .map_err(|error| ApiError {
        kind: "dependency".into(),
        message: error.to_string(),
        exit_code: 3,
        field: None,
    })?;

    let Ok(mut guard) = state.0.lock() else {
        return result;
    };
    if guard
        .as_ref()
        .is_some_and(|current| Arc::ptr_eq(current, &cancel))
    {
        *guard = None;
    }

    result
}

#[tauri::command]
fn cancel_repository_scan(state: State<'_, ScanState>) {
    let Ok(guard) = state.0.lock() else { return };
    if let Some(cancel) = guard.as_ref() {
        cancel.store(true, Ordering::Relaxed);
    }
}

#[tauri::command]
async fn inspect_repository(
    repository: PathBuf,
    network: bool,
    approved: State<'_, ApprovedPaths>,
) -> Result<RepositoryStatus, ApiError> {
    let resolved = ensure_approved(&repository, &approved)?;
    service()?
        .inspect_repository(&resolved, "origin", network)
        .map_err(Into::into)
}

/// Recent GitHub Actions runs for a repository.
///
/// Reaches the network, so the frontend must only call it from an explicit user
/// action — never on mount, never on a timer. PRODUCT.md rules out automatic
/// network checks, and a CI panel that refreshed itself would be exactly that.
#[tauri::command]
async fn repository_ci_status(
    repository: PathBuf,
    limit: usize,
    approved: State<'_, ApprovedPaths>,
) -> Result<RepositoryCiStatus, ApiError> {
    let resolved = ensure_approved(&repository, &approved)?;
    service()?
        .repository_ci_status(&resolved, limit)
        .map_err(Into::into)
}

#[tauri::command]
async fn bind_repository(
    repository: PathBuf,
    profile: String,
    force: bool,
    approved: State<'_, ApprovedPaths>,
) -> Result<(), ApiError> {
    let resolved = ensure_approved(&repository, &approved)?;
    service()?
        .bind_repository(&resolved, &profile, "origin", force)
        .map_err(Into::into)
}

#[tauri::command]
async fn unbind_repository(
    repository: PathBuf,
    approved: State<'_, ApprovedPaths>,
) -> Result<(), ApiError> {
    let resolved = ensure_approved(&repository, &approved)?;
    service()?.unbind_repository(&resolved).map_err(Into::into)
}

/// Clone a repository into an approved folder and bind it.
///
/// `parent` must be a folder the user picked in this session, for the same
/// reason `add_repository_root` requires one: this writes a new tree to disk,
/// and the folder it writes into should be a place the user chose rather than
/// one a page supplied.
#[tauri::command]
async fn clone_repository(
    profile: String,
    repository: String,
    parent: PathBuf,
    protocol: Option<String>,
    approved: State<'_, ApprovedPaths>,
) -> Result<PathBuf, ApiError> {
    let resolved = ensure_session_approved(&parent, &approved)?;
    let protocol = match protocol.as_deref() {
        None | Some("") | Some("auto") => None,
        Some("ssh") => Some(RemoteProtocol::Ssh),
        Some("https") => Some(RemoteProtocol::Https),
        Some(other) => {
            return Err(ApiError {
                kind: "usage".into(),
                message: format!("unknown transport '{other}'"),
                exit_code: 2,
                field: Some("protocol".into()),
            });
        }
    };
    service()?
        .clone_repository(&profile, &repository, &resolved, protocol)
        .map_err(Into::into)
}

/// Who authored a range of commits. Reads the commit log and verifies
/// signatures locally; nothing here reaches the network.
#[tauri::command]
async fn audit_repository(
    repository: PathBuf,
    range: String,
    max_commits: usize,
    approved: State<'_, ApprovedPaths>,
) -> Result<CheckReport, ApiError> {
    let resolved = ensure_approved(&repository, &approved)?;
    service()?
        .audit_repository(&resolved, &range, max_commits)
        .map_err(Into::into)
}

#[tauri::command]
async fn list_directory_rules() -> Result<Vec<RuleView>, ApiError> {
    service()?.directory_rules().map_err(Into::into)
}

/// A directory rule hands a folder — and every repository ever created under
/// it — to a profile, so like `add_repository_root` it takes only a folder the
/// user picked in this session. A path arriving any other way is refused.
#[tauri::command]
async fn add_directory_rule(
    profile: String,
    path: PathBuf,
    approved: State<'_, ApprovedPaths>,
) -> Result<PathBuf, ApiError> {
    let resolved = ensure_session_approved(&path, &approved)?;
    service()?
        .add_directory_rule(&profile, &resolved)
        .map_err(Into::into)
}

/// No approval needed: the path has to already be a configured rule, and
/// removing one only ever takes authority away.
#[tauri::command]
async fn remove_directory_rule(path: PathBuf) -> Result<PathBuf, ApiError> {
    service()?.remove_directory_rule(&path).map_err(Into::into)
}

#[tauri::command]
async fn hook_state(
    repository: PathBuf,
    approved: State<'_, ApprovedPaths>,
) -> Result<HookState, ApiError> {
    let resolved = ensure_approved(&repository, &approved)?;
    service()?.hook_state(&resolved).map_err(Into::into)
}

#[tauri::command]
async fn install_hooks(
    repository: PathBuf,
    approved: State<'_, ApprovedPaths>,
) -> Result<HookState, ApiError> {
    let resolved = ensure_approved(&repository, &approved)?;
    service()?.install_hooks(&resolved).map_err(Into::into)
}

#[tauri::command]
async fn uninstall_hooks(
    repository: PathBuf,
    approved: State<'_, ApprovedPaths>,
) -> Result<HookState, ApiError> {
    let resolved = ensure_approved(&repository, &approved)?;
    service()?.uninstall_hooks(&resolved).map_err(Into::into)
}

/// Put text on the clipboard.
///
/// Deliberately a Rust command rather than the clipboard plugin's JS API, and
/// deliberately write-only. The frontend gets a way to copy a remote URL; it
/// does not get a way to *read* whatever the user last copied, which could be a
/// password from their password manager. Same reasoning as the dialog plugin,
/// which is also only ever reached from Rust.
#[tauri::command]
fn copy_text(app: AppHandle, text: String) -> Result<(), ApiError> {
    app.clipboard().write_text(text).map_err(|error| ApiError {
        kind: "dependency".into(),
        message: format!("could not write to the clipboard: {error}"),
        exit_code: 3,
        field: None,
    })
}

/// Reveal a repository in the operating system's file manager.
///
/// `reveal_item_in_dir`, not a shell. The mockup's "Open in Terminal" would
/// need arbitrary command spawn, which is a large new attack surface for a
/// convenience button on a tool whose entire point is refusing to do surprising
/// things to repositories. The path still goes through the same session
/// authorization boundary as every other repository operation, so this cannot
/// be pointed at a folder the user never selected.
#[tauri::command]
fn open_path(repository: PathBuf, approved: State<'_, ApprovedPaths>) -> Result<(), ApiError> {
    let resolved = ensure_approved(&repository, &approved)?;
    tauri_plugin_opener::reveal_item_in_dir(&resolved).map_err(|error| ApiError {
        kind: "dependency".into(),
        message: format!("could not open {}: {error}", resolved.display()),
        exit_code: 3,
        field: None,
    })
}

#[tauri::command]
async fn switch_github_account(profile: String) -> Result<(), ApiError> {
    service()?
        .switch_github_account(&profile)
        .map_err(Into::into)
}

#[tauri::command]
async fn github_accounts(hostname: String) -> Result<Vec<Account>, ApiError> {
    service()?.github_accounts(&hostname).map_err(Into::into)
}

#[tauri::command]
async fn test_ssh(profile: String) -> Result<SshTestReport, ApiError> {
    service()?.ssh_test(&profile).map_err(Into::into)
}

#[tauri::command]
async fn doctor() -> Result<DoctorReport, ApiError> {
    service()?.doctor().map_err(Into::into)
}

/// The version shown in the title bar. Reads this crate's version, which is the
/// single version literal in the repository — `tauri.conf.json` deliberately
/// omits `version` so it cannot drift.
#[tauri::command]
fn app_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

fn main() {
    tauri::Builder::default()
        // Both of these are called from Rust only — see copy_text and open_path
        // — so neither appears in capabilities/default.json. The frontend has no
        // route to the plugin APIs themselves.
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(ScanState::default())
        .manage(ApprovedPaths::default())
        .invoke_handler(tauri::generate_handler![
            list_profiles,
            create_profile,
            update_profile,
            rename_profile,
            remove_profile,
            import_profile_preview,
            choose_folder,
            choose_key_file,
            list_repository_roots,
            add_repository_root,
            remove_repository_root,
            scan_repositories,
            cancel_repository_scan,
            inspect_repository,
            repository_ci_status,
            copy_text,
            open_path,
            bind_repository,
            unbind_repository,
            clone_repository,
            audit_repository,
            list_directory_rules,
            add_directory_rule,
            remove_directory_rule,
            hook_state,
            install_hooks,
            uninstall_hooks,
            switch_github_account,
            github_accounts,
            test_ssh,
            doctor,
            app_version,
            demo::demo_fixtures
        ])
        .run(tauri::generate_context!())
        .expect("error while running GitBound desktop");
}

fn ensure_session_approved(
    path: &PathBuf,
    approved: &State<'_, ApprovedPaths>,
) -> Result<PathBuf, ApiError> {
    let canonical = std::fs::canonicalize(path).map_err(ApiError::from_io)?;
    let normalized = strip_unc_prefix(&canonical);
    let session = approved
        .0
        .lock()
        .map_err(|_| ApiError::internal("approved-folder state is unavailable"))?;
    if authorize(&normalized, &session, &[]) {
        Ok(normalized)
    } else {
        Err(ApiError {
            kind: "usage".into(),
            message: "Choose this folder in GitBound before using it.".into(),
            exit_code: 2,
            field: Some("path".into()),
        })
    }
}

fn ensure_approved(
    path: &PathBuf,
    approved: &State<'_, ApprovedPaths>,
) -> Result<PathBuf, ApiError> {
    let canonical = std::fs::canonicalize(path).map_err(ApiError::from_io)?;
    let normalized = strip_unc_prefix(&canonical);
    {
        let session = approved
            .0
            .lock()
            .map_err(|_| ApiError::internal("approved-folder state is unavailable"))?;
        if authorize(&normalized, &session, &[]) {
            return Ok(normalized);
        }
    }
    let roots = service()?.repository_roots()?;
    if authorize(&normalized, &HashSet::new(), &roots) {
        return Ok(normalized);
    }
    Err(ApiError {
        kind: "usage".into(),
        message: "This repository is outside the folders approved in GitBound.".into(),
        exit_code: 2,
        field: Some("repository".into()),
    })
}

/// The desktop app's only path-authorization boundary: a canonical, UNC-stripped
/// `path` is allowed when the user picked it in this session, or when it lies
/// under a persisted approved root.
///
/// `Path::starts_with` compares whole components, so a sibling directory whose
/// name merely shares a prefix with an approved root (`/work-other` against
/// `/work`) is not authorized by it.
fn authorize(path: &std::path::Path, session: &HashSet<PathBuf>, roots: &[PathBuf]) -> bool {
    session.contains(path)
        || roots
            .iter()
            .any(|root| path.starts_with(strip_unc_prefix(root)))
}

/// Strip the Windows extended-length path prefix (`\\?\`) so that
/// `starts_with` comparisons work uniformly whether both sides were
/// canonicalized or not.
fn strip_unc_prefix(path: &std::path::Path) -> PathBuf {
    #[cfg(windows)]
    {
        let s = path.to_string_lossy();
        if let Some(stripped) = s.strip_prefix(r"\\?\") {
            return PathBuf::from(stripped);
        }
    }
    path.to_path_buf()
}

trait DesktopApiError {
    fn from_io(error: std::io::Error) -> Self;
    fn internal(message: &str) -> Self;
}

impl DesktopApiError for ApiError {
    fn from_io(error: std::io::Error) -> Self {
        Self {
            kind: "usage".into(),
            message: error.to_string(),
            exit_code: 2,
            field: Some("path".into()),
        }
    }
    fn internal(message: &str) -> Self {
        Self {
            kind: "dependency".into(),
            message: message.into(),
            exit_code: 3,
            field: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(paths: &[&str]) -> HashSet<PathBuf> {
        paths.iter().map(PathBuf::from).collect()
    }

    #[test]
    fn a_session_pick_authorizes_only_that_exact_folder() {
        let session = set(&["/home/a/work"]);
        assert!(authorize(
            std::path::Path::new("/home/a/work"),
            &session,
            &[]
        ));
        assert!(!authorize(
            std::path::Path::new("/home/a/other"),
            &session,
            &[]
        ));
    }

    #[test]
    fn an_approved_root_authorizes_repositories_beneath_it() {
        let roots = vec![PathBuf::from("/home/a/work")];
        assert!(authorize(
            std::path::Path::new("/home/a/work/project"),
            &HashSet::new(),
            &roots
        ));
        assert!(authorize(
            std::path::Path::new("/home/a/work"),
            &HashSet::new(),
            &roots
        ));
    }

    #[test]
    fn a_sibling_sharing_a_name_prefix_is_rejected() {
        // The failure mode a string `starts_with` would have: /work-secret must
        // not be authorized by an approval of /work.
        let roots = vec![PathBuf::from("/home/a/work")];
        assert!(!authorize(
            std::path::Path::new("/home/a/work-secret"),
            &HashSet::new(),
            &roots
        ));
        assert!(!authorize(
            std::path::Path::new("/home/a/work-secret/project"),
            &HashSet::new(),
            &roots
        ));
    }

    #[test]
    fn nothing_is_authorized_without_a_pick_or_a_root() {
        assert!(!authorize(
            std::path::Path::new("/home/a/work"),
            &HashSet::new(),
            &[]
        ));
    }

    #[test]
    fn a_parent_of_an_approved_root_is_rejected() {
        let roots = vec![PathBuf::from("/home/a/work")];
        assert!(!authorize(
            std::path::Path::new("/home/a"),
            &HashSet::new(),
            &roots
        ));
    }
}
