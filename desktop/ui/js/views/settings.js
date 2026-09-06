// Settings: appearance, the folders GitBound is allowed to look in, the folders
// it assigns an identity to, and what it knows about itself.
//
// Directory rules sit here rather than on Repositories for the same reason
// approved folders do. A rule is standing authority — every repository under
// the folder gets that identity through Git's own includeIf, including ones
// cloned later by any tool — not a per-repository action.
//
// Approved paths moved here from the Repositories page. Approving a folder is a
// permission grant, not a routine operation — it is the thing that decides what
// GitBound is allowed to see at all — and it does not belong on the page a
// user visits every day, sitting next to Scan.
//
// Revoking one is a two-click action for the same reason it always was.
import { displayPath, pageHeader } from "../components.js";
import { createView, h } from "../dom.js";
import { icon } from "../icons.js";

const THEMES = [
  { id: "system", label: "System", hint: "Follow the operating system" },
  { id: "light", label: "Light", hint: "Always light" },
  { id: "dark", label: "Dark", hint: "Always dark" },
];

const ACCENTS = [
  { id: "moss", label: "Moss" },
  { id: "violet", label: "Violet" },
];

export function settings(ctx) {
  const host = h("section", null);
  const view = createView(host, build);

  async function addRoot() {
    try {
      const path = await ctx.api.chooseFolder();
      if (!path) return;
      ctx.app.roots = [...ctx.app.roots, await ctx.api.addRoot(path)];
      ctx.signal("Folder approved");
      view.set({});
    } catch (reason) {
      ctx.fail(reason);
    }
  }

  /** First click arms the confirmation; the second actually revokes. */
  async function revokeRoot(path) {
    if (view.state.removeRoot !== path) {
      view.set({ removeRoot: path });
      return;
    }
    try {
      await ctx.api.removeRoot(path);
      ctx.app.roots = ctx.app.roots.filter((root) => root !== path);
      // Repositories discovered under a revoked root are no longer ours to show.
      ctx.app.repositories = ctx.app.repositories.filter(
        (repo) => !repo.path.startsWith(path),
      );
      view.set({ removeRoot: "" });
      ctx.signal("Approved folder removed");
      ctx.touch();
    } catch (reason) {
      ctx.fail(reason);
    }
  }

  /**
   * A directory rule assigns a folder to an identity through Git's own
   * `includeIf`, so every repository under it gets that identity without being
   * bound — including ones cloned later, by any tool. That is the reason this
   * belongs next to approved folders rather than on the Repositories page: it
   * is standing authority, not a per-repository action.
   */
  async function loadDirectories() {
    try {
      view.set({ directories: await ctx.api.directoryRules() });
    } catch (reason) {
      ctx.fail(reason);
    }
  }

  async function addDirectory() {
    const profile = view.state.ruleProfile || ctx.app.profiles[0]?.name;
    if (!profile) return;
    try {
      // chooseFolder is what grants the backend's session approval, so the
      // picker has to come first — a path typed or remembered is refused.
      const path = await ctx.api.chooseFolder();
      if (!path) return;
      await ctx.api.addDirectoryRule(profile, path);
      ctx.signal(`Folder assigned to ${profile}`);
      await loadDirectories();
    } catch (reason) {
      ctx.fail(reason);
    }
  }

  /** Two clicks, like revoking a root: this changes what Git does globally. */
  async function removeDirectory(path) {
    if (view.state.removeRule !== path) {
      view.set({ removeRule: path });
      return;
    }
    try {
      await ctx.api.removeDirectoryRule(path);
      view.set({ removeRule: "" });
      ctx.signal("Directory rule removed");
      await loadDirectories();
    } catch (reason) {
      ctx.fail(reason);
    }
  }

  async function copyConfigPath() {
    try {
      await ctx.api.copyText(view.state.doctor?.config_path ?? "");
      ctx.signal("Configuration path copied");
    } catch (reason) {
      ctx.fail(reason);
    }
  }

  async function loadDoctor() {
    try {
      view.set({ doctor: await ctx.api.doctor() });
    } catch {
      // The About panel is informational. Failing to fill it in must not stop
      // the user changing their theme.
    }
  }

  function build(state) {
    return [
      pageHeader(
        "Settings",
        "Appearance, approved folders, directory rules, and what GitBound knows about itself.",
      ),
      appearance(),
      roots(state),
      directories(state),
      about(state),
    ];
  }

  /**
   * Change one half of the theme without disturbing the other.
   *
   * `ctx.setTheme` redraws the shell, but the shell reuses an already-mounted
   * view rather than rebuilding it — that is what stops a toast from wiping out
   * a half-typed form. The consequence here is that this view's handlers would
   * otherwise keep the `ctx.app.theme` they closed over at build time, so
   * picking an accent after picking dark mode would write the *old* mode back
   * and silently undo it. Reading the current value at click time and asking
   * for a redraw afterwards is what keeps the two radio groups independent.
   */
  function setTheme(patch) {
    const current = ctx.app.theme;
    ctx.setTheme(patch.mode ?? current.mode, patch.accent ?? current.accent);
    view.set({});
  }

  function appearance() {
    const theme = ctx.app.theme;
    return h(
      "div",
      { class: "settings-section" },
      h(
        "div",
        { class: "section-head" },
        h("h2", null, "Appearance"),
        h("p", null, "Stored on this machine only. Never written to your configuration file."),
      ),
      h(
        "fieldset",
        { class: "choice-row" },
        h("legend", null, "Colour scheme"),
        THEMES.map((option) =>
          h(
            "label",
            { class: theme.mode === option.id ? "selected" : "" },
            h("input", {
              type: "radio",
              name: "theme-mode",
              "data-k": `settings.theme.${option.id}`,
              checked: theme.mode === option.id,
              onChange: () => setTheme({ mode: option.id }),
            }),
            h("span", null, h("strong", null, option.label), h("small", null, option.hint)),
          )
        ),
      ),
      h(
        "fieldset",
        { class: "choice-row" },
        h("legend", null, "Accent"),
        ACCENTS.map((option) =>
          h(
            "label",
            { class: theme.accent === option.id ? "selected" : "" },
            h("input", {
              type: "radio",
              name: "theme-accent",
              "data-k": `settings.accent.${option.id}`,
              checked: theme.accent === option.id,
              onChange: () => setTheme({ accent: option.id }),
            }),
            h(
              "span",
              null,
              h("strong", null, option.label),
              h("small", null, "Status colours are unaffected"),
            ),
          )
        ),
      ),
    );
  }

  function roots(state) {
    return h(
      "div",
      { class: "settings-section" },
      h(
        "div",
        { class: "section-head" },
        h("h2", null, "Approved folders"),
        h(
          "p",
          null,
          "GitBound only looks inside these. Nothing else on disk is ever scanned.",
        ),
        h(
          "button",
          { class: "secondary", "data-k": "settings.addRoot", onClick: () => void addRoot() },
          icon("folder-plus", 16),
          "Add folder",
        ),
      ),
      ctx.app.roots.length === 0
        ? h("p", { class: "empty-copy" }, "No folder is approved, so no repository can be found.")
        : h(
          "ul",
          { class: "root-list" },
          ctx.app.roots.map((root) =>
            h(
              "li",
              null,
              icon("folder", 15),
              h("code", null, displayPath(root)),
              state.removeRoot === root
                ? [
                  h("span", { class: "confirm-copy" }, "Revoke access to this folder?"),
                  h(
                    "button",
                    {
                      class: "danger-text",
                      "data-k": `settings.confirmRoot.${root}`,
                      onClick: () => void revokeRoot(root),
                    },
                    "Confirm",
                  ),
                  h(
                    "button",
                    {
                      class: "secondary",
                      "data-k": `settings.cancelRoot.${root}`,
                      onClick: () => view.set({ removeRoot: "" }),
                    },
                    "Cancel",
                  ),
                ]
                : h(
                  "button",
                  {
                    class: "icon-button",
                    "data-k": `settings.removeRoot.${root}`,
                    "aria-label": `Remove approved folder ${root}`,
                    onClick: () => void revokeRoot(root),
                  },
                  icon("x", 15),
                ),
            )
          ),
        ),
    );
  }

  function directories(state) {
    const rules = state.directories;
    return h(
      "div",
      { class: "settings-section" },
      h(
        "div",
        { class: "section-head" },
        h("h2", null, "Directory rules"),
        h(
          "p",
          null,
          "Every repository under one of these folders uses that identity automatically, through Git's own includeIf — including ones cloned later, by any tool.",
        ),
        ctx.app.profiles.length === 0 ? null : h(
          "span",
          { class: "head-action" },
          h(
            "select",
            {
              "data-k": "settings.ruleProfile",
              "aria-label": "Identity to assign",
              value: state.ruleProfile || ctx.app.profiles[0]?.name || "",
              onChange: (event) => view.set({ ruleProfile: event.target.value }),
            },
            ctx.app.profiles.map((item) => h("option", { value: item.name }, item.name)),
          ),
          h(
            "button",
            {
              class: "secondary",
              "data-k": "settings.addRule",
              onClick: () => void addDirectory(),
            },
            icon("folder-plus", 16),
            "Assign folder",
          ),
        ),
      ),
      ctx.app.profiles.length === 0
        ? h("p", { class: "empty-copy" }, "Create an identity before assigning a folder to one.")
        : rules === null
        ? h("p", { class: "empty-copy" }, "Reading directory rules…")
        : rules.length === 0
        ? h(
          "p",
          { class: "empty-copy" },
          "No folder is assigned. Repositories get their identity from binding alone.",
        )
        : h(
          "ul",
          { class: "root-list" },
          rules.map((rule) =>
            h(
              "li",
              null,
              icon("folder", 15),
              h(
                "span",
                { class: "rule-line" },
                h("code", null, displayPath(rule.path)),
                // What the rule does, in the reader's terms. The Git config key
                // behind it is still one hover away, so the claim can be checked
                // against `git config --global --list` rather than taken on
                // trust — but it is not what the line has to say first.
                h(
                  "small",
                  { title: `${rule.condition} = ${rule.include}` },
                  `${rule.profile} · every repository in this folder`,
                ),
              ),
              state.removeRule === rule.path
                ? [
                  h("span", { class: "confirm-copy" }, "Stop applying this identity here?"),
                  h(
                    "button",
                    {
                      class: "danger-text",
                      "data-k": `settings.confirmRule.${rule.path}`,
                      onClick: () => void removeDirectory(rule.path),
                    },
                    "Confirm",
                  ),
                  h(
                    "button",
                    {
                      class: "secondary",
                      "data-k": `settings.cancelRule.${rule.path}`,
                      onClick: () => view.set({ removeRule: "" }),
                    },
                    "Cancel",
                  ),
                ]
                : h(
                  "button",
                  {
                    class: "icon-button",
                    "data-k": `settings.removeRule.${rule.path}`,
                    "aria-label": `Remove directory rule for ${rule.path}`,
                    onClick: () => void removeDirectory(rule.path),
                  },
                  icon("x", 15),
                ),
            )
          ),
        ),
    );
  }

  function about(state) {
    const doctor = state.doctor;
    const row = (term, value, action) =>
      h("div", null, h("dt", null, term), h("dd", null, value, action ?? null));
    return h(
      "div",
      { class: "settings-section" },
      h(
        "div",
        { class: "section-head" },
        h("h2", null, "About"),
        h("p", null, "GitBound stores paths and policy. It never stores credentials."),
      ),
      h(
        "dl",
        { class: "identity-grid wide" },
        row("Version", ctx.app.version ? `v${ctx.app.version}` : "unknown"),
        row(
          "Configuration file",
          doctor?.config_path ?? "…",
          doctor?.config_path
            ? h(
              "button",
              {
                class: "icon-button",
                "data-k": "settings.copyConfig",
                "aria-label": "Copy configuration path",
                onClick: () => void copyConfigPath(),
              },
              icon("copy", 15),
            )
            : null,
        ),
        row("Schema version", doctor ? String(doctor.schema_version) : "…"),
        row("Identities configured", doctor ? String(doctor.profile_count) : "…"),
      ),
      h(
        "div",
        { class: "safe-note" },
        icon("shield-check", 16),
        "Credentials stay with GitHub CLI, your Git credential helper, and OpenSSH. GitBound never reads, stores, or transmits them.",
      ),
    );
  }

  view.start({
    removeRoot: "",
    doctor: null,
    directories: null,
    removeRule: "",
    ruleProfile: "",
  });
  void loadDoctor().then(() => view.set({}));
  // Reading rules is a config-file read plus a path join per rule: no network,
  // no writes, nothing to confirm. Requiring a click to see standing authority
  // over a folder would be the wrong default.
  void loadDirectories();
  return host;
}
