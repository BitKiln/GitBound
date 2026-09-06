// Status: expected versus actual identity for one repository. Read-only.
//
// Local checks run automatically when the selected repository changes; the
// network checks (which reach GitHub through `gh`) only run when asked for, so
// opening this view never makes a network call on its own.
import { badge, loading, pageHeader } from "../components.js";
import { createView, h } from "../dom.js";
import { icon, spinner } from "../icons.js";

export function status(ctx) {
  const host = h("section", null);
  const view = createView(host, build);

  async function inspect(network = false) {
    const path = ctx.app.selectedRepo;
    if (!path) return;
    view.set({ refreshing: true });
    try {
      view.set({ report: await ctx.api.inspect(path, network) });
    } catch (reason) {
      ctx.fail(reason);
    } finally {
      view.set({ refreshing: false });
    }
  }

  function selectedRepo() {
    return ctx.app.repositories.find((repo) => repo.path === ctx.app.selectedRepo);
  }

  function build(state) {
    const selected = selectedRepo();
    return [
      pageHeader("Status", "Expected versus actual identity for one repository.", [
        h(
          "button",
          {
            class: "secondary",
            onClick: () => void inspect(true),
            disabled: !selected || state.refreshing,
          },
          state.refreshing ? spinner(16) : icon("refresh-cw", 16),
          "Refresh network checks",
        ),
      ]),
      toolbar(state, selected),
      body(state, selected),
    ];
  }

  function toolbar(state, selected) {
    return h(
      "div",
      { class: "status-toolbar" },
      h(
        "label",
        null,
        "Repository",
        h(
          "select",
          {
            "data-k": "status.repository",
            value: selected?.path ?? "",
            // Inspection is kicked off from here rather than from the render
            // function: the shell rebuilds this view's parent on unrelated
            // changes, and re-inspecting on every one of those would re-run the
            // check needlessly.
            onChange: (event) => {
              ctx.select(event.target.value);
              view.set({ report: null });
              void inspect(false);
            },
          },
          ctx.app.repositories.map((repo) =>
            h("option", { value: repo.path }, `${repo.name} — ${repo.path}`)
          ),
        ),
      ),
      state.report
        ? h(
          "span",
          { class: "network-note" },
          state.report.network_checked ? "Network checks current" : "Local checks only",
        )
        : null,
    );
  }

  function body(state, selected) {
    if (!selected) {
      return h(
        "div",
        { class: "empty-inspector status-empty" },
        icon("git-branch", 28),
        h("h2", null, "No repository selected"),
        h(
          "p",
          null,
          "Add an approved root and scan it under Repositories, then return here to inspect its identity.",
        ),
      );
    }
    if (state.refreshing && !state.report) return loading();
    if (!state.report) return null;
    return checkTable(state.report.report.checks);
  }

  function checkTable(checks) {
    return h(
      "table",
      { class: "check-table" },
      h(
        "thead",
        null,
        h(
          "tr",
          null,
          h("th", null, "Check"),
          h("th", null, "Expected"),
          h("th", null, "Actual"),
          h("th", null, "Result"),
        ),
      ),
      h(
        "tbody",
        null,
        checks.map((item) =>
          h(
            "tr",
            null,
            h("th", { scope: "row" }, item.id.replace(/_/g, " "), h("small", null, item.message)),
            h("td", null, item.expected ?? "—"),
            h("td", null, item.actual ?? "Not checked"),
            h("td", null, badge(item.status)),
          )
        ),
      ),
    );
  }

  view.start({ report: null, refreshing: false });
  // Mounting with a repository already chosen elsewhere should show its state
  // straight away rather than an empty table.
  if (ctx.app.selectedRepo) void inspect(false);
  return host;
}
