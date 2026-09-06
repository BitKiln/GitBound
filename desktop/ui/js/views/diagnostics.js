// Diagnostics: the structured health report from `gitbound doctor`.
//
// Read-only — it never writes configuration, which is why it stays reachable
// even when the configuration could not be read at all.
//
// The mockup puts a "Fix" button on every failing row. Here that button reveals
// the exact command that fixes the problem and copies it, rather than running
// anything. Every remediation this report can produce is an action on somebody
// else's tool — install Git, authenticate GitHub CLI, add a key to an agent —
// and a program that silently runs privileged commands against a developer's
// machine to fix its own complaints is the opposite of what this product is.
// Showing the command is also the only version that teaches the user what
// happened.
import { badge, pageHeader, statusGlyph } from "../components.js";
import { createView, h } from "../dom.js";
import { icon, spinner } from "../icons.js";

export function diagnostics(ctx) {
  const host = h("section", null);
  const view = createView(host, build);

  async function load() {
    view.set({ loading: true });
    try {
      view.set({ report: await ctx.api.doctor() });
    } catch (reason) {
      ctx.fail(reason);
    } finally {
      view.set({ loading: false });
    }
  }

  async function copy(command) {
    try {
      await ctx.api.copyText(command);
      ctx.signal("Command copied");
    } catch (reason) {
      ctx.fail(reason);
    }
  }

  function build(state) {
    const report = state.report;
    return [
      pageHeader(
        "Diagnostics",
        "Structured checks for GitBound, Git, GitHub CLI, and OpenSSH.",
        [
          h(
            "button",
            {
              class: "secondary",
              "data-k": "diagnostics.run",
              disabled: state.loading,
              onClick: () => void load(),
            },
            state.loading ? spinner(16) : icon("refresh-cw", 16),
            "Run again",
          ),
        ],
      ),
      report ? healthBanner(report) : null,
      report && report.profile_issues.length ? profileIssues(report.profile_issues) : null,
      report
        ? h(
          "div",
          { class: "diagnostic-list" },
          report.dependencies.map((item) => dependencyRow(state, item)),
        )
        : null,
      report ? recommendation(state, report) : null,
    ];
  }

  function healthBanner(report) {
    const attention = report.dependencies.filter((item) => item.state !== "ok").length
      + report.profile_issues.length;
    return h(
      "div",
      { class: `health-banner ${report.healthy ? "healthy" : "attention"}` },
      statusGlyph(report.healthy ? "ok" : "warning"),
      h(
        "div",
        null,
        h(
          "strong",
          null,
          report.healthy
            ? "Everything required is available"
            : `${attention} checks need attention`,
        ),
        h(
          "p",
          null,
          `Configuration schema ${report.schema_version} · ${report.profile_count} identities · ${report.config_path}`,
        ),
      ),
    );
  }

  function profileIssues(issues) {
    return h(
      "div",
      { class: "profile-issues" },
      h("strong", null, "Identity configuration"),
      issues.map((issue) => h("p", null, icon("circle-alert", 15), issue)),
    );
  }

  function dependencyRow(state, item) {
    const open = state.open === item.name;
    return h(
      "div",
      { class: "diagnostic-row" },
      h("span", { class: `dependency-icon ${item.state}` }, statusGlyph(item.state)),
      h(
        "div",
        null,
        h("h2", null, item.name),
        h("p", null, item.detail),
        open && item.remediation ? commandBlock(item.remediation) : null,
      ),
      h(
        "div",
        { class: "row-actions" },
        badge(item.state),
        item.remediation
          ? h(
            "button",
            {
              class: "secondary",
              "data-k": `diagnostics.fix.${item.name}`,
              "aria-expanded": open ? "true" : "false",
              onClick: () => view.set({ open: open ? "" : item.name }),
            },
            open ? "Hide" : "Fix",
          )
          : null,
      ),
    );
  }

  function commandBlock(command) {
    return h(
      "div",
      { class: "remediation" },
      h("code", null, command),
      h(
        "button",
        {
          class: "icon-button",
          "data-k": `diagnostics.copy.${command.slice(0, 24)}`,
          title: "Copy command",
          "aria-label": "Copy command",
          onClick: () => void copy(command),
        },
        icon("copy", 15),
      ),
    );
  }

  /**
   * The single most useful next step, promoted out of the list. A report with
   * five warnings and no ordering leaves the user to guess which one matters,
   * and an unavailable dependency always matters more than a warning.
   */
  function recommendation(state, report) {
    const worst = report.dependencies.find((item) => item.state === "unavailable")
      ?? report.dependencies.find((item) => item.state === "warning");
    if (!worst || !worst.remediation) return null;
    const open = state.open === `recommended:${worst.name}`;
    return h(
      "div",
      { class: "recommendation" },
      icon("circle-alert", 17),
      h(
        "div",
        null,
        h("strong", null, "Recommended"),
        h("p", null, worst.detail),
        open ? commandBlock(worst.remediation) : null,
      ),
      h(
        "button",
        {
          class: "secondary",
          "data-k": "diagnostics.showCommand",
          "aria-expanded": open ? "true" : "false",
          onClick: () => view.set({ open: open ? "" : `recommended:${worst.name}` }),
        },
        icon("terminal", 15),
        open ? "Hide command" : "Show command",
      ),
    );
  }

  view.start({ report: null, loading: false, open: "" });
  void load();
  return host;
}
