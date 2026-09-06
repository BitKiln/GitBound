// Application shell: title bar, navigation, the workspace the views mount into,
// the Quick Switch palette, and the toast strip. Boots by reading local
// configuration and fails closed if it cannot.
//
// State is split the way the React original split it. Shell state (which view,
// the profile and repository lists, toasts) lives in `app` below and is owned
// here. State that belonged to a single component — a form draft, a filter
// string, a confirmation that is pending — stays inside that view's own
// createView, so typing in a form re-renders only that form.
import { initials, loadFailure, loading } from "./components.js";
import { h } from "./dom.js";
import { icon } from "./icons.js";
import { api, message } from "./ipc.js";
import { applyTheme, readTheme } from "./theme.js";
import { dashboard } from "./views/dashboard.js";
import { diagnostics } from "./views/diagnostics.js";
import { identities } from "./views/identities.js";
import { onboarding } from "./views/onboarding.js";
import { quickSwitch } from "./views/quick-switch.js";
import { repositories } from "./views/repositories.js";
import { repositoryDetails } from "./views/repository-details.js";
import { settings } from "./views/settings.js";
import { ssh } from "./views/ssh.js";
import { status } from "./views/status.js";

const VIEWS = [
  { id: "dashboard", label: "Dashboard", icon: "layout-dashboard" },
  { id: "identities", label: "Identities", icon: "users-round" },
  { id: "repositories", label: "Repositories", icon: "git-branch" },
  { id: "ssh", label: "SSH Keys", icon: "key-round" },
  { id: "diagnostics", label: "Diagnostics", icon: "monitor-cog" },
  { id: "settings", label: "Settings", icon: "settings" },
];

// `status` and `repository-details` are reachable from the Dashboard and the
// Repositories table but are not peers in the sidebar: they are always *about*
// something the user selected first.
const NESTED_VIEWS = new Set(["status", "repository-details"]);

const app = {
  version: "",
  view: "dashboard",
  profiles: [],
  // The config schema the backend actually reports, and the identity the
  // GitHub CLI is actually active as. Both are read from the backend rather
  // than assumed, because the sidebar states them as fact.
  schemaVersion: 0,
  activeGithubUser: "",
  repositories: [],
  roots: [],
  selectedRepo: "",
  // Set by the Settings view; persisted per machine, not in config.toml, which
  // is the safety-relevant file and has no business holding a colour scheme.
  theme: readTheme(),
  quickSwitch: false,
  busy: true,
  loadFailed: false,
  error: "",
  notice: "",
};

const root = document.getElementById("root");
let noticeTimer = 0;
// The view currently mounted in the workspace, so shell redraws do not tear
// down and rebuild a view that has not changed — which would discard whatever
// the user had typed into it.
let mounted = { id: null, host: null };

/** Raise a transient success message. A second notice restarts the countdown. */
export function signal(text) {
  app.notice = text;
  window.clearTimeout(noticeTimer);
  noticeTimer = window.setTimeout(() => {
    app.notice = "";
    render();
  }, 3500);
  render();
}

/**
 * Raise an error. Errors are not timed out the way notices are: an error says
 * something the user asked for did not happen, and one that vanishes on its own
 * leaves them believing it did. It is cleared by dismissing it, or by leaving
 * the view that raised it - see `go`.
 */
export function fail(reason) {
  app.error = message(reason);
  render();
}

/** Re-read profiles and roots from disk. Used after any write. */
export async function reload() {
  app.busy = true;
  app.error = "";
  app.loadFailed = false;
  render();
  try {
    const [profiles, roots] = await Promise.all([activeApi.profiles(), activeApi.roots()]);
    app.profiles = profiles;
    app.roots = roots;
    await readBackendFacts();
  } catch (reason) {
    app.error = message(reason);
    app.loadFailed = true;
  } finally {
    app.busy = false;
    // A reload changes the data the mounted view was built from, so it has to
    // be rebuilt from scratch rather than left showing stale rows.
    mounted = { id: null, host: null };
    render();
  }
}

/**
 * Read the two facts the sidebar reports: the config schema version, and which
 * GitHub account the CLI is currently active as.
 *
 * Both are best-effort. A missing or unauthenticated `gh`, or a doctor call
 * that fails, leaves the sidebar saying less rather than saying something
 * untrue, and must never fail a reload — the rest of the app works offline.
 */
async function readBackendFacts() {
  // The schema version is settled when the backend loads the config and cannot
  // change while the app is running, so it is read once rather than on every
  // write. The active account can change, so it is not cached.
  if (!app.schemaVersion) {
    try {
      const report = await activeApi.doctor();
      app.schemaVersion = report?.schema_version ?? 0;
    } catch {
      app.schemaVersion = 0;
    }
  }
  // One lookup per distinct host: profiles usually share github.com, and the
  // active account is per host.
  const hosts = [...new Set(app.profiles.map((entry) => entry.profile.hostname))];
  let active = "";
  for (const host of hosts) {
    try {
      const accounts = await activeApi.accounts(host);
      const found = accounts.find((account) => account.active && account.valid);
      if (found) {
        active = found.login;
        break;
      }
    } catch {
      // Leave the account unknown for this host and try the next one.
    }
  }
  app.activeGithubUser = active;
}

// Swapped for a fixture-backed stand-in when `?demo` is active, so no view has
// to know whether it is looking at real configuration or not.
let activeApi = api;

/** The context every view receives. */
const context = {
  app,
  get api() {
    return activeApi;
  },
  signal,
  fail,
  reload,
  select(path) {
    app.selectedRepo = path;
    render();
  },
  /**
   * Redraw the shell after a view changed shared state - the sidebar's drift
   * dot reads app.repositories, for instance. The mounted view is reused
   * rather than rebuilt, so this never discards what a view is holding.
   */
  touch() {
    render();
  },
  /**
   * Show a view. Any error still on screen belonged to the view being left: it
   * named something that did not happen there, and following the user to an
   * unrelated page it does not describe only makes it look like the new page is
   * broken. Leaving is as good a dismissal as pressing the button.
   */
  go(view) {
    if (view !== app.view) app.error = "";
    app.view = view;
    render();
  },
  /** Open a repository's detail page, selecting it on the way. */
  openRepository(path) {
    // Same reasoning as `go`, plus: a different repository is a different page,
    // so an error about the last one does not belong on it either.
    if (app.view !== "repository-details" || path !== app.selectedRepo) app.error = "";
    app.selectedRepo = path;
    app.view = "repository-details";
    render();
  },
  setTheme(theme, accent) {
    app.theme = applyTheme(theme, accent);
    render();
  },
  openQuickSwitch() {
    // Nothing to switch between before a profile exists, and the palette over
    // the onboarding wizard would be a dead end.
    if (app.profiles.length === 0 || app.loadFailed) return;
    app.quickSwitch = true;
    render();
  },
  closeQuickSwitch() {
    app.quickSwitch = false;
    render();
  },
};

function titlebar() {
  return h(
    "header",
    { class: "titlebar" },
    h(
      "div",
      { class: "wordmark" },
      h("span", { class: "mark" }, icon("git-branch", 17)),
      h("span", null, "GitBound"),
      app.version ? h("span", { class: "version" }, `v${app.version}`) : null,
    ),
    h(
      "div",
      { class: "titlebar-actions" },
      h(
        "button",
        {
          class: "quick-switch-trigger",
          "data-k": "shell-quick-switch",
          "aria-label": "Quick switch identity",
          disabled: app.profiles.length === 0 || app.loadFailed,
          onClick: () => context.openQuickSwitch(),
        },
        icon("arrow-right-left", 15),
        h("span", null, "Quick switch"),
        h("kbd", null, shortcutLabel()),
      ),
      h("div", { class: "local-only" }, icon("shield-check", 15), " Local configuration only"),
    ),
  );
}

/** macOS says Cmd, everywhere else says Ctrl, and the handler accepts both. */
function shortcutLabel() {
  return navigator.platform.toLowerCase().includes("mac") ? "⌘K" : "Ctrl K";
}

function sidebar() {
  const drifted = app.repositories.some((repo) => repo.status === "drifted");
  // The identity the GitHub CLI is actually active as, not merely the first one
  // configured. When the active account matches no profile — or `gh` could not
  // be asked — the chip is omitted rather than naming an identity that is not
  // in use.
  const active = app.activeGithubUser
    ? app.profiles.find((entry) =>
      entry.profile.github_user.toLowerCase() === app.activeGithubUser.toLowerCase()
    )
    : null;
  return h(
    "aside",
    { class: "sidebar", "aria-label": "Primary navigation" },
    h(
      "nav",
      null,
      VIEWS.map((item) =>
        h(
          "button",
          {
            "aria-label": item.label,
            "aria-current": app.view === item.id ? "page" : null,
            class: app.view === item.id ? "nav-item active" : "nav-item",
            onClick: () => context.go(item.id),
          },
          icon(item.icon, 17),
          h("span", null, item.label),
          item.id === "repositories" && drifted
            ? h("i", { class: "nav-dot", "aria-hidden": "true" })
            : null,
        )
      ),
    ),
    h(
      "div",
      { class: "sidebar-foot" },
      active
        ? h(
          "button",
          {
            class: "identity-chip",
            "data-k": "sidebar-identity",
            onClick: () => context.go("identities"),
          },
          h("span", { class: "avatar", "aria-hidden": "true" }, initials(active.profile.git_name)),
          h(
            "span",
            { class: "identity-chip-text" },
            h("strong", null, active.name),
            h("span", null, active.profile.git_email),
          ),
        )
        : null,
      h(
        "div",
        { class: "sidebar-note" },
        h("span", { class: "status-dot ok" }),
        "Configuration protected",
        h(
          "span",
          { class: "muted" },
          app.schemaVersion
            ? `Schema ${app.schemaVersion} · no secrets stored`
            : "No secrets stored",
        ),
      ),
    ),
  );
}

function toasts() {
  return [
    app.error
      ? h(
        "div",
        { class: "toast error", role: "alert" },
        icon("circle-x", 17),
        h("span", null, app.error),
        h(
          "button",
          {
            "aria-label": "Dismiss error",
            onClick: () => {
              app.error = "";
              render();
            },
          },
          icon("x", 16),
        ),
      )
      : null,
    app.notice
      ? h(
        "div",
        { class: "toast success", role: "status" },
        icon("check", 17),
        h("span", null, app.notice),
      )
      : null,
  ];
}

const BUILDERS = {
  dashboard,
  diagnostics,
  identities,
  repositories,
  "repository-details": repositoryDetails,
  settings,
  ssh,
  status,
};

/** Placeholder for a view id that has no builder. */
function stub(id) {
  const label = VIEWS.find((view) => view.id === id)?.label ?? id;
  return h(
    "section",
    null,
    h(
      "header",
      { class: "page-header" },
      h("div", null, h("h1", null, label), h("p", null, "Not yet ported.")),
    ),
  );
}

/** Build a view by id, falling back to the placeholder. */
function buildView(id) {
  const builder = BUILDERS[id];
  return builder ? builder(context) : stub(id);
}

function workspace() {
  const host = h("main", { class: "workspace" });
  if (app.busy) {
    host.append(loading());
    mounted = { id: null, host: null };
    return host;
  }
  if (app.loadFailed) {
    // Diagnostics stays reachable while everything else is withheld: it is the
    // view that explains why the read failed.
    host.append(
      app.view === "diagnostics"
        ? buildView("diagnostics")
        : loadFailure(reload, () => context.go("diagnostics")),
    );
    mounted = { id: null, host: null };
    return host;
  }
  if (app.profiles.length === 0) {
    // No profile exists yet, so there is nothing the other views could act on.
    if (mounted.id === "onboarding" && mounted.host) {
      host.append(mounted.host);
    } else {
      const wizard = onboarding(context);
      mounted = { id: "onboarding", host: wizard };
      host.append(wizard);
    }
    return host;
  }
  // A nested view with nothing selected has no subject, so fall back rather
  // than render a detail page about nothing.
  if (NESTED_VIEWS.has(app.view) && !app.selectedRepo) {
    app.view = "repositories";
  }
  // Reuse the existing view element when the view has not changed, so a shell
  // redraw (a toast appearing, say) does not discard view-local state.
  if (mounted.id === app.view && mounted.host) {
    host.append(mounted.host);
  } else {
    const view = buildView(app.view);
    mounted = { id: app.view, host: view };
    host.append(view);
  }
  return host;
}

function render() {
  root.replaceChildren(
    h(
      "div",
      { class: "app-shell" },
      titlebar(),
      sidebar(),
      workspace(),
      app.quickSwitch ? quickSwitch(context) : null,
      toasts(),
    ),
  );
}

// The only global key handling in the application. A document-level listener is
// enough and stays inside `script-src 'self'` — no globalShortcut plugin, and no
// accelerator that would capture the chord while GitBound is in the
// background.
document.addEventListener("keydown", (event) => {
  if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "k") {
    event.preventDefault();
    if (app.quickSwitch) context.closeQuickSwitch();
    else context.openQuickSwitch();
    return;
  }
  if (event.key === "Escape" && app.quickSwitch) {
    event.preventDefault();
    context.closeQuickSwitch();
  }
});

/**
 * Ask the backend for demo fixtures. Returns null unless this is a debug build
 * launched with `?demo` — a release binary answers `null`, because the fixture
 * document is compiled out of it entirely.
 */
async function demoFixtures() {
  if (!new URLSearchParams(location.search).has("demo")) return null;
  try {
    return await api.demoFixtures();
  } catch {
    return null;
  }
}

/** A read-only stand-in for `api` that answers from fixtures. */
function demoApi(fixtures) {
  const noop = async () => {};
  return {
    ...api,
    version: async () => "demo",
    profiles: async () => fixtures.profiles,
    roots: async () => fixtures.roots,
    doctor: async () => fixtures.doctor,
    scan: async () => fixtures.repositories,
    cancelScan: noop,
    accounts: async () => fixtures.accounts,
    testSsh: async () => fixtures.ssh_test,
    ciStatus: async () => fixtures.ci_status,
    // Demo mode runs in a plain browser with no Tauri to invoke, so every
    // command a view calls on its own has to be answered here. The hooks card
    // reads on mount, which makes this one load-bearing rather than cosmetic.
    clone: async (_profile, repository, parent) => `${parent}/${repository.split("/").pop()}`,
    audit: async (repository) => ({
      repository,
      profile: null,
      overall: "ok",
      checks: [
        {
          id: "commits",
          expected: null,
          actual: "3",
          status: "ok",
          message: "3 commits inspected",
        },
        {
          id: "author_identity",
          expected: "oss@mira.dev",
          actual: "oss@mira.dev",
          status: "ok",
          message: "every commit was authored by an allowed address",
        },
      ],
    }),
    directoryRules: async () => [
      {
        profile: fixtures.profiles[0]?.name ?? "work",
        path: fixtures.roots[0] ?? "/home/mira/work",
        condition: `includeIf.gitdir:${fixtures.roots[0] ?? "/home/mira/work"}/.path`,
        include: "/home/mira/.config/GitBound/work.gitconfig",
      },
    ],
    hookState: async () => ({ pre_commit: "installed", pre_push: "installed" }),
    installHooks: async () => ({ pre_commit: "installed", pre_push: "installed" }),
    uninstallHooks: async () => ({ pre_commit: "not installed", pre_push: "not installed" }),
    inspect: async (repository, network = false) => ({
      network_checked: network,
      report: {
        repository,
        profile: fixtures.repositories.find((r) => r.path === repository)?.bound_profile,
        overall: "warning",
        checks: [
          {
            id: "git_author",
            expected: "Mira Chen",
            actual: "Mira Chen",
            status: "ok",
            message: "Git author matches the bound profile",
          },
          {
            id: "git_email",
            expected: "oss@mira.dev",
            actual: "oss@mira.dev",
            status: "ok",
            message: "Git email matches the bound profile",
          },
          {
            id: "remote_owner",
            expected: "tauri-apps",
            actual: "tauri-apps",
            status: "ok",
            message: "Remote owner is allowed by the profile",
          },
          {
            id: "github_cli",
            expected: "personal",
            actual: network ? "mira-acme" : undefined,
            status: network ? "warning" : "unverified",
            message: network
              ? "Active account differs; no switch was performed."
              : "Run network refresh to check.",
          },
        ],
      },
    }),
  };
}

applyTheme(app.theme.mode, app.theme.accent);
render();

const fixtures = await demoFixtures();
if (fixtures) {
  activeApi = demoApi(fixtures);
  app.repositories = fixtures.repositories;
  app.selectedRepo = fixtures.repositories[1]?.path ?? "";
}

try {
  app.version = await activeApi.version();
} catch {
  // A missing version is cosmetic; it must not block the app from starting.
}
await reload();
