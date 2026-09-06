// The IPC surface. Replaces the old api.ts and, with it, the @tauri-apps/api
// npm package: `tauri.conf.json` sets `app.withGlobalTauri`, so Tauri injects
// its own JS API bundle as `window.__TAURI__` and there is nothing to install.
//
// Every command name below is checked against the generate_handler! list in
// desktop/src-tauri/src/main.rs by the `every_registered_command_is_reachable`
// test, so renaming a command in Rust without updating this file fails
// `cargo test`. That test does NOT check argument names — see the note on
// renameProfile.
const { invoke, Channel } = window.__TAURI__.core;

/**
 * Every command rejects with a serialized `gitbound::api::ApiError`
 * ({kind, message, exit_code, field?}). Normalizing here means no view has to
 * know that. Mirrors errorMessage() from the old App.tsx.
 */
export function message(error) {
  if (error && typeof error === "object" && typeof error.message === "string") return error.message;
  return error instanceof Error ? error.message : String(error);
}

export const api = {
  version: () => invoke("app_version"),

  // Profiles
  profiles: () => invoke("list_profiles"),
  createProfile: (name, profile) => invoke("create_profile", { name, profile }),
  updateProfile: (name, profile) => invoke("update_profile", { name, profile }),
  // Tauri maps camelCase JS keys onto snake_case Rust parameters, so `oldName`
  // reaches `old_name`. The contract test catches a wrong *command* name but
  // not a wrong *argument* name — this one has to be right by inspection.
  renameProfile: (oldName, newName) => invoke("rename_profile", { oldName, newName }),
  removeProfile: (name) => invoke("remove_profile", { name }),
  importPreview: (repository) => invoke("import_profile_preview", { repository }),

  // Native pickers. These are Rust commands calling app.dialog(), not the
  // dialog plugin's JS API, which is why the frontend never touches
  // window.__TAURI__.dialog.
  chooseFolder: () => invoke("choose_folder"),
  chooseKeyFile: () => invoke("choose_key_file"),

  // Repository roots and scanning
  roots: () => invoke("list_repository_roots"),
  addRoot: (path) => invoke("add_repository_root", { path }),
  removeRoot: (path) => invoke("remove_repository_root", { path }),
  cancelScan: () => invoke("cancel_repository_scan"),

  /**
   * Directory rules: a folder assigned to an identity, applied by Git's own
   * `includeIf` before GitBound is involved at all. Each rule resolves as
   * {profile, path, condition, include} — the last two being the Git config key
   * and the fragment file it points at, so the claim can be checked against
   * `git config --global --list` rather than taken on trust.
   *
   * addRule takes a folder the user picked with chooseFolder in this session;
   * any other path is refused by the backend. Both writes resolve with the
   * canonical path acted on, which is not always the one passed in.
   */
  directoryRules: () => invoke("list_directory_rules"),
  addDirectoryRule: (profile, path) => invoke("add_directory_rule", { profile, path }),
  removeDirectoryRule: (path) => invoke("remove_directory_rule", { path }),

  /**
   * Streams RepositoryScanEvent values over a Tauri Channel while the blocking
   * scan runs on a worker thread, then resolves with the final summary list.
   * Cancel with api.cancelScan().
   */
  scan(onEvent) {
    const events = new Channel();
    events.onmessage = onEvent;
    return invoke("scan_repositories", { events });
  },

  /**
   * Clone a repository into a folder the user picked with chooseFolder in this
   * session, and bind it. `parent` is the folder to clone *into*; the leaf
   * comes from the repository's own name. `protocol` is "ssh", "https", or
   * omitted to let the profile decide. Resolves with the path cloned to.
   *
   * Reaches the network and writes to disk, so only ever from a submit.
   */
  clone: (profile, repository, parent, protocol) =>
    invoke("clone_repository", { profile, repository, parent, protocol }),

  // Repository inspection and binding
  inspect: (repository, network = false) => invoke("inspect_repository", { repository, network }),

  /**
   * Who authored a range of commits, as a CheckReport — the same shape the
   * check list already renders. Local: reads the commit log and verifies
   * signatures, and reaches no network.
   */
  audit: (repository, range, maxCommits = 200) =>
    invoke("audit_repository", { repository, range, maxCommits }),

  /**
   * Recent GitHub Actions runs. This reaches the network, so it must only ever
   * be called from an explicit user action — a Refresh button, not a mount, not
   * a timer. Resolves with {runs, available, detail}; `available: false` is the
   * ordinary answer when gh is missing or the repository has no workflows, and
   * is not an error to report as one.
   */
  ciStatus: (repository, limit = 5) => invoke("repository_ci_status", { repository, limit }),
  bind: (repository, profile, force = false) =>
    invoke("bind_repository", { repository, profile, force }),
  unbind: (repository) => invoke("unbind_repository", { repository }),

  /**
   * The pre-commit and pre-push hooks, which are what stop a wrong-identity
   * commit from being made rather than reporting it afterwards. All three
   * resolve with {pre_commit, pre_push}, each a human string: "installed",
   * "not installed", "occupied by another hook", or "unreadable".
   *
   * Install refuses to overwrite a hook it did not write, and uninstall refuses
   * to remove one — so "occupied by another hook" is a state the UI has to
   * show rather than a problem it can offer to fix.
   */
  hookState: (repository) => invoke("hook_state", { repository }),
  installHooks: (repository) => invoke("install_hooks", { repository }),
  uninstallHooks: (repository) => invoke("uninstall_hooks", { repository }),

  /**
   * Write-only clipboard access, and only through a Rust command: the frontend
   * can copy a remote URL out, and has no route to read whatever the user last
   * copied in.
   */
  copyText: (text) => invoke("copy_text", { text }),

  /**
   * Reveal a repository in the OS file manager. Not a shell — see open_path in
   * desktop/src-tauri/src/main.rs for why "Open in Terminal" is not offered.
   */
  openPath: (repository) => invoke("open_path", { repository }),

  // GitHub CLI, SSH, health
  switchAccount: (profile) => invoke("switch_github_account", { profile }),
  accounts: (hostname) => invoke("github_accounts", { hostname }),
  testSsh: (profile) => invoke("test_ssh", { profile }),
  doctor: () => invoke("doctor"),

  /**
   * Fixture data for `?demo`. Answers `null` in a release build, where the
   * fixture document is compiled out - see desktop/src-tauri/src/demo.rs.
   */
  demoFixtures: () => invoke("demo_fixtures"),
};
