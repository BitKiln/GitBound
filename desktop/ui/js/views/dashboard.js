// Dashboard: the one screen that answers "am I about to commit as the wrong
// person" without any clicking.
//
// It is deliberately read-mostly. Everything that writes lives one click away,
// on the view that owns it, so glancing at this page can never change anything.
// The one exception is the identity check, which contacts GitHub CLI and SSH —
// and that is a button, never a page load, because PRODUCT.md rules out
// automatic network checks.
import { badge, initials, pageHeader, relativeTime, statusGlyph } from "../components.js";
import { createView, h } from "../dom.js";
import { icon, spinner } from "../icons.js";
import { statusLabel } from "../status.js";

export function dashboard(ctx) {
  const host = h("section", null);
  const view = createView(host, build);

  // The identity the GitHub CLI is actually active as. Reporting the first
  // configured profile instead would put an "Active" badge on an identity that
  // is not in use, which is the precise confusion this product exists to
  // prevent.
  const active = () =>
    ctx.app.activeGithubUser
      ? ctx.app.profiles.find((entry) =>
        entry.profile.github_user.toLowerCase() === ctx.app.activeGithubUser.toLowerCase()
      )
      : null;
  const current = () =>
    ctx.app.repositories.find((repo) => repo.path === ctx.app.selectedRepo)
      ?? ctx.app.repositories[0];

  // --- actions ------------------------------------------------------------

  async function runChecks() {
    const repo = current();
    if (!repo) return;
    view.set({ checking: true });
    try {
      const result = await ctx.api.inspect(repo.path, true);
      view.set({ report: result.report, checkedAt: new Date().toISOString() });
    } catch (reason) {
      ctx.fail(reason);
    } finally {
      view.set({ checking: false });
    }
  }

  async function refreshCi() {
    const repo = current();
    if (!repo) return;
    view.set({ ciBusy: true });
    try {
      view.set({ ci: await ctx.api.ciStatus(repo.path, 3) });
    } catch (reason) {
      ctx.fail(reason);
    } finally {
      view.set({ ciBusy: false });
    }
  }

  // --- rendering ----------------------------------------------------------

  function build(state) {
    return [
      pageHeader("Dashboard", "Your current identity and the repository you are working in.", [
        h(
          "button",
          {
            class: "primary",
            "data-k": "dashboard.quickSwitch",
            onClick: () => ctx.openQuickSwitch(),
          },
          icon("arrow-right-left", 16),
          "Quick switch",
        ),
      ]),
      h("div", { class: "dashboard-grid" }, identityCard(), repositoryCard(state)),
      shortcuts(),
      ciCard(state),
      recentTable(),
    ];
  }

  function identityCard() {
    const item = active();
    if (!item) {
      // Either the GitHub CLI has no active account, or it has one that matches
      // no configured identity. Both are worth saying plainly instead of
      // showing a card that claims something is active.
      const copy = ctx.app.activeGithubUser
        ? `GitHub CLI is active as @${ctx.app.activeGithubUser}, which matches no identity here.`
        : "No GitHub CLI account is active.";
      return h(
        "div",
        { class: "card" },
        h("div", { class: "card-head" }, h("h2", null, "Active identity")),
        h("p", { class: "empty-copy" }, copy),
      );
    }
    const profile = item.profile;
    return h(
      "div",
      { class: "card" },
      h(
        "div",
        { class: "card-head" },
        h("h2", null, "Active identity"),
        h("span", { class: "badge ok" }, "Active"),
      ),
      h(
        "div",
        { class: "active-identity" },
        h("span", { class: "avatar large", "aria-hidden": "true" }, initials(profile.git_name)),
        h(
          "div",
          null,
          h("h3", null, item.name),
          h("p", null, profile.git_name),
          h("p", { class: "muted" }, profile.git_email),
          h("p", { class: "muted" }, `@${profile.github_user} · ${profile.hostname}`),
          h(
            "p",
            { class: "muted" },
            profile.ssh_key ? `SSH · ${profile.ssh_key}` : "HTTPS · credential helper",
          ),
        ),
      ),
      h(
        "button",
        {
          class: "secondary wide",
          "data-k": "dashboard.switchIdentity",
          onClick: () => ctx.openQuickSwitch(),
        },
        icon("arrow-right-left", 16),
        "Switch identity",
      ),
    );
  }

  function repositoryCard(state) {
    const repo = current();
    if (!repo) {
      return h(
        "div",
        { class: "card" },
        h("div", { class: "card-head" }, h("h2", null, "Current repository")),
        h(
          "p",
          { class: "empty-copy" },
          "No repository discovered yet. Approve a folder and run a scan.",
        ),
        h(
          "button",
          {
            class: "secondary wide",
            "data-k": "dashboard.goRepositories",
            onClick: () => ctx.go("repositories"),
          },
          icon("git-branch", 16),
          "Go to repositories",
        ),
      );
    }
    return h(
      "div",
      { class: "card" },
      h(
        "div",
        { class: "card-head" },
        h("h2", null, "Current repository"),
        badge(repo.status, statusLabel(repo.status)),
      ),
      h(
        "div",
        { class: "repo-title" },
        icon("folder", 18),
        h("div", null, h("h3", null, repo.name), h("p", { class: "path" }, repo.path)),
      ),
      state.report
        ? h(
          "ul",
          { class: "check-list" },
          state.report.checks.map((check) =>
            h(
              "li",
              null,
              statusGlyph(check.status),
              h("span", null, h("strong", null, check.id), h("small", null, check.message)),
              h("span", { class: `status-word ${check.status}` }, statusLabel(check.status)),
            )
          ),
        )
        : h(
          "p",
          { class: "empty-copy" },
          "Checks contact GitHub CLI and SSH, so they run when you ask rather than on their own.",
        ),
      h(
        "div",
        { class: "button-row" },
        h(
          "button",
          {
            class: "secondary",
            "data-k": "dashboard.runChecks",
            disabled: state.checking,
            onClick: () => void runChecks(),
          },
          state.checking ? spinner(16) : icon("play", 16),
          state.report ? "Re-run checks" : "Run checks",
        ),
        h(
          "button",
          {
            class: "secondary",
            "data-k": "dashboard.openRepository",
            onClick: () => ctx.openRepository(repo.path),
          },
          icon("folder", 16),
          "Open repository",
        ),
      ),
      state.checkedAt
        ? h("p", { class: "muted" }, `Last checked ${relativeTime(state.checkedAt)}`)
        : null,
    );
  }

  function shortcuts() {
    const card = (iconName, title, description, onClick, key) =>
      h(
        "button",
        { class: "shortcut", "data-k": key, onClick },
        icon(iconName, 18),
        h("span", null, h("strong", null, title), h("small", null, description)),
      );
    return h(
      "div",
      { class: "shortcut-row" },
      card(
        "arrow-right-left",
        "Switch identity",
        "Change the active identity",
        () => ctx.openQuickSwitch(),
        "dashboard.shortcut.switch",
      ),
      card(
        "git-branch",
        "View repositories",
        "Manage repository identities",
        () => ctx.go("repositories"),
        "dashboard.shortcut.repositories",
      ),
      card(
        "monitor-cog",
        "Run diagnostics",
        "Check for issues",
        () => ctx.go("diagnostics"),
        "dashboard.shortcut.diagnostics",
      ),
    );
  }

  function ciCard(state) {
    const repo = current();
    if (!repo) return null;
    return h(
      "div",
      { class: "card" },
      h(
        "div",
        { class: "card-head" },
        h("h2", null, "Continuous integration"),
        h(
          "button",
          {
            class: "secondary",
            "data-k": "dashboard.ci",
            disabled: state.ciBusy,
            onClick: () => void refreshCi(),
          },
          state.ciBusy ? spinner(15) : icon("activity", 15),
          state.ci ? "Refresh" : "Check CI status",
        ),
      ),
      ciBody(state, repo),
    );
  }

  function ciBody(state, repo) {
    if (!state.ci) {
      return h(
        "p",
        { class: "empty-copy" },
        `Not checked. Asking GitHub CLI for ${repo.name}'s workflow runs is a network call, so it happens on request.`,
      );
    }
    if (!state.ci.available) {
      return h(
        "p",
        { class: "empty-copy" },
        state.ci.detail ?? "GitHub CLI could not report workflow runs for this repository.",
      );
    }
    if (state.ci.runs.length === 0) {
      return h("p", { class: "empty-copy" }, "No workflow runs found.");
    }
    return h(
      "ul",
      { class: "check-list" },
      state.ci.runs.map((run) =>
        h(
          "li",
          null,
          statusGlyph(run.conclusion),
          h(
            "span",
            null,
            h("strong", null, run.name),
            h("small", null, `${run.title} · ${run.branch}`),
          ),
          h(
            "span",
            { class: `status-word ${run.conclusion}` },
            run.conclusion,
            h("small", null, relativeTime(run.created_at)),
          ),
        )
      ),
    );
  }

  function recentTable() {
    const recent = ctx.app.repositories.slice(0, 5);
    if (recent.length === 0) return null;
    return h(
      "div",
      { class: "card" },
      h("div", { class: "card-head" }, h("h2", null, "Recent repositories")),
      h(
        "div",
        { class: "table-wrap" },
        h(
          "table",
          { class: "data-table" },
          h(
            "thead",
            null,
            h(
              "tr",
              null,
              h("th", { scope: "col" }, "Repository"),
              h("th", { scope: "col" }, "Identity"),
              h("th", { scope: "col" }, "Remote"),
              h("th", { scope: "col" }, "Status"),
            ),
          ),
          h(
            "tbody",
            null,
            recent.map((repo) =>
              h(
                "tr",
                null,
                h(
                  "td",
                  null,
                  h(
                    "button",
                    {
                      class: "cell-link",
                      "data-k": `dashboard.open.${repo.path}`,
                      onClick: () => ctx.openRepository(repo.path),
                    },
                    icon("folder", 15),
                    h("span", null, h("strong", null, repo.name), h("small", null, repo.path)),
                  ),
                ),
                h(
                  "td",
                  null,
                  repo.bound_profile
                    ? h("span", { class: "identity-pill" }, repo.bound_profile)
                    : h("span", { class: "identity-pill none" }, "Not assigned"),
                ),
                h("td", null, h("code", null, repo.remote?.hostname ?? "—")),
                h(
                  "td",
                  null,
                  h(
                    "span",
                    { class: `status-word ${repo.status}` },
                    statusGlyph(repo.status),
                    statusLabel(repo.status),
                  ),
                ),
              )
            ),
          ),
        ),
      ),
    );
  }

  view.start({
    report: null,
    checkedAt: "",
    checking: false,
    ci: null,
    ciBusy: false,
  });
  return host;
}
