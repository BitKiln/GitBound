// Repositories: scan approved folders and see, at a glance, which identity each
// repository is using.
//
// This used to be a list/detail split. The list column was too narrow to show
// the identity and the remote at the same time, which meant the one question
// this view exists to answer — "is anything bound to the wrong account?" —
// needed a click per repository. A table answers it without any.
//
// The detail pane moved to repository-details.js, which is also where every
// write now lives. Nothing on this page changes a repository.
import { displayPath, pageHeader, relativeTime, statusGlyph } from "../components.js";
import { createView, h } from "../dom.js";
import { icon } from "../icons.js";
import { statusLabel } from "../status.js";

export function repositories(ctx) {
  const host = h("section", null);
  const view = createView(host, build);

  // --- actions ------------------------------------------------------------

  async function addRoot() {
    try {
      const path = await ctx.api.chooseFolder();
      if (!path) return;
      ctx.app.roots = [...ctx.app.roots, await ctx.api.addRoot(path)];
      view.set({});
      await scan();
    } catch (reason) {
      ctx.fail(reason);
    }
  }

  /**
   * Clone and bind in one step.
   *
   * The destination is picked with chooseFolder rather than typed, because that
   * is what grants the backend's session approval — a path this page supplied
   * from memory is refused, by design.
   */
  async function chooseDestination() {
    try {
      const parent = await ctx.api.chooseFolder();
      if (parent) view.set({ cloneParent: parent });
    } catch (reason) {
      ctx.fail(reason);
    }
  }

  async function clone() {
    const { cloneProfile, cloneRepo, cloneParent, cloneProtocol } = view.state;
    const profile = cloneProfile || ctx.app.profiles[0]?.name;
    if (!profile || !cloneRepo.trim() || !cloneParent) return;
    view.set({ cloning: true });
    try {
      const path = await ctx.api.clone(profile, cloneRepo.trim(), cloneParent, cloneProtocol);
      ctx.signal(`Cloned to ${path} and bound to ${profile}`);
      view.set({ cloneOpen: false, cloneRepo: "", cloneParent: "" });
      // A clone that is not in the list is a clone the user cannot act on, and
      // it only appears once it has been discovered under an approved root.
      await scan();
    } catch (reason) {
      ctx.fail(reason);
    } finally {
      view.set({ cloning: false });
    }
  }

  async function scan() {
    view.set({ scanning: true, scanProgress: "Starting approved-root scan…" });
    try {
      const found = await ctx.api.scan((event) => {
        // Progress arrives on a Tauri Channel while the blocking walk runs.
        if (event.type === "root_started") view.set({ scanProgress: `Scanning ${event.root}` });
        if (event.type === "repository_found") {
          view.set({ scanProgress: "Repository found; continuing scan…" });
        }
        if (event.type === "finished") {
          view.set({ scanProgress: `${event.repositories ?? 0} repositories found` });
        }
      });
      ctx.app.repositories = found;
      if (!ctx.app.selectedRepo && found[0]) ctx.app.selectedRepo = found[0].path;
      // The sidebar's drift dot reads the list this just replaced.
      ctx.touch();
      ctx.signal(`Scan complete · ${found.length} repositories found`);
    } catch (reason) {
      ctx.fail(reason);
    } finally {
      view.set({ scanning: false });
    }
  }

  async function cancelScan() {
    view.set({ scanProgress: "Cancelling scan…" });
    await ctx.api.cancelScan();
  }

  /**
   * CI status for every listed repository, fetched once, on demand.
   *
   * Explicitly user-initiated: PRODUCT.md rules out automatic network checks,
   * and a table that fetched CI on mount would be exactly that. It also fetches
   * serially rather than in parallel, because a scan of thirty repositories
   * would otherwise open thirty `gh` processes at once.
   */
  async function refreshCi() {
    view.set({ ciBusy: true });
    const ci = { ...view.state.ci };
    try {
      for (const repo of ctx.app.repositories) {
        ci[repo.path] = await ctx.api.ciStatus(repo.path, 1);
        view.set({ ci: { ...ci } });
      }
      ctx.signal("CI status refreshed");
    } catch (reason) {
      ctx.fail(reason);
    } finally {
      view.set({ ciBusy: false });
    }
  }

  // --- rendering ----------------------------------------------------------

  function build(state) {
    return [
      pageHeader("Repositories", "Repositories discovered under the folders you approved.", [
        h(
          "button",
          {
            class: "secondary",
            "data-k": "repositories.ci",
            disabled: state.ciBusy || ctx.app.repositories.length === 0,
            onClick: () => void refreshCi(),
          },
          icon("activity", 16),
          state.ciBusy ? "Checking CI…" : "Check CI",
        ),
        h(
          "button",
          { class: "secondary", "data-k": "repositories.addRoot", onClick: () => void addRoot() },
          icon("folder-plus", 16),
          "Add path",
        ),
        h(
          "button",
          {
            class: "secondary",
            "data-k": "repositories.clone",
            disabled: ctx.app.profiles.length === 0,
            onClick: () => view.set({ cloneOpen: !state.cloneOpen }),
          },
          icon("git-branch", 16),
          "Clone",
        ),
        h(
          "button",
          {
            class: "primary",
            "data-k": "repositories.scan",
            onClick: () => void (state.scanning ? cancelScan() : scan()),
          },
          state.scanning ? icon("square", 14) : icon("refresh-cw", 16),
          state.scanning ? "Cancel scan" : "Scan",
        ),
      ]),
      state.cloneOpen ? cloneForm(state) : null,
      searchRow(state),
      state.scanProgress
        ? h("p", { class: "scan-progress", role: "status" }, state.scanProgress)
        : null,
      ctx.app.roots.length === 0 ? noRoots() : table(state),
      h(
        "footer",
        { class: "table-foot" },
        `${ctx.app.repositories.length} ${
          ctx.app.repositories.length === 1 ? "repository" : "repositories"
        }`,
        h(
          "button",
          {
            class: "link-button",
            "data-k": "repositories.manageRoots",
            onClick: () => ctx.go("settings"),
          },
          "Manage approved paths",
        ),
      ),
    ];
  }

  /**
   * The clone form. Cloning and binding are one action here, as they are on the
   * command line: a clone that succeeded and a bind that failed leaves a
   * repository configured as whoever the machine is by default, which is the
   * outcome the product exists to prevent.
   */
  function cloneForm(state) {
    const ready = state.cloneRepo.trim() !== "" && state.cloneParent !== "";
    return h(
      "div",
      { class: "card clone-form" },
      h(
        "label",
        { class: "field-row" },
        h("span", { class: "field-label" }, "Identity"),
        h(
          "select",
          {
            "data-k": "repositories.cloneProfile",
            value: state.cloneProfile || ctx.app.profiles[0]?.name || "",
            onChange: (event) => view.set({ cloneProfile: event.target.value }),
          },
          ctx.app.profiles.map((item) => h("option", { value: item.name }, item.name)),
        ),
      ),
      h(
        "label",
        { class: "field-row" },
        h("span", { class: "field-label" }, "Repository"),
        h("input", {
          type: "text",
          "data-k": "repositories.cloneRepo",
          placeholder: "owner/repo, or a full URL",
          value: state.cloneRepo,
          onInput: (event) => view.set({ cloneRepo: event.target.value }),
        }),
      ),
      h(
        "label",
        { class: "field-row" },
        h("span", { class: "field-label" }, "Transport"),
        h(
          "select",
          {
            "data-k": "repositories.cloneProtocol",
            value: state.cloneProtocol,
            onChange: (event) => view.set({ cloneProtocol: event.target.value }),
          },
          h("option", { value: "auto" }, "Chosen by the identity"),
          h("option", { value: "ssh" }, "SSH"),
          h("option", { value: "https" }, "HTTPS"),
        ),
      ),
      h(
        "div",
        { class: "field-row" },
        h("span", { class: "field-label" }, "Clone into"),
        h(
          "span",
          { class: "input-action" },
          h("code", null, state.cloneParent ? displayPath(state.cloneParent) : "No folder chosen"),
          h(
            "button",
            {
              class: "secondary",
              "data-k": "repositories.cloneBrowse",
              onClick: () => void chooseDestination(),
            },
            icon("folder", 15),
            "Choose",
          ),
        ),
      ),
      h(
        "div",
        { class: "form-actions" },
        h(
          "p",
          { class: "empty-copy" },
          "The repository is bound to the identity as part of the clone. If binding fails, the clone is removed.",
        ),
        h(
          "button",
          {
            class: "secondary",
            "data-k": "repositories.cloneCancel",
            onClick: () => view.set({ cloneOpen: false }),
          },
          "Cancel",
        ),
        h(
          "button",
          {
            class: "primary",
            "data-k": "repositories.cloneSubmit",
            disabled: !ready || state.cloning,
            onClick: () => void clone(),
          },
          state.cloning ? "Cloning…" : "Clone and bind",
        ),
      ),
    );
  }

  function noRoots() {
    return h(
      "div",
      { class: "empty-inspector" },
      icon("folder-plus", 24),
      h("h2", null, "No folders approved yet"),
      h(
        "p",
        null,
        "GitBound only looks inside folders you choose. Nothing else on disk is scanned.",
      ),
      h(
        "button",
        { class: "primary", "data-k": "repositories.addRootEmpty", onClick: () => void addRoot() },
        icon("folder-plus", 16),
        "Add a path",
      ),
    );
  }

  function searchRow(state) {
    return h(
      "div",
      { class: "search" },
      icon("search", 15),
      h("input", {
        "aria-label": "Search repositories",
        // Without data-k the caret would be lost on every keystroke, because
        // each one rebuilds this whole view.
        "data-k": "repositories.query",
        value: state.query,
        placeholder: "Search repositories",
        onInput: (event) => view.set({ query: event.target.value }),
      }),
    );
  }

  function table(state) {
    const needle = state.query.toLowerCase();
    const filtered = ctx.app.repositories.filter((repo) =>
      `${repo.name} ${repo.path} ${repo.bound_profile ?? ""}`.toLowerCase().includes(needle)
    );
    if (ctx.app.repositories.length === 0) {
      return h(
        "p",
        { class: "empty-copy pad" },
        "No repositories discovered yet. Run a scan to find them.",
      );
    }
    if (filtered.length === 0) {
      return h("p", { class: "empty-copy pad" }, "No matching repositories.");
    }
    return h(
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
            h("th", { scope: "col" }, "CI"),
            h("th", { scope: "col" }, h("span", { class: "sr-only" }, "Actions")),
          ),
        ),
        h("tbody", null, filtered.map((repo) => row(state, repo))),
      ),
    );
  }

  function row(state, repo) {
    const ci = state.ci[repo.path];
    return h(
      "tr",
      { class: ctx.app.selectedRepo === repo.path ? "selected" : "" },
      h(
        "td",
        null,
        h(
          "button",
          {
            class: "cell-link",
            "data-k": `repositories.open.${repo.path}`,
            onClick: () => ctx.openRepository(repo.path),
          },
          icon("folder", 15),
          h("span", null, h("strong", null, repo.name), h("small", null, displayPath(repo.path))),
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
      h("td", null, ciCell(ci)),
      h(
        "td",
        { class: "row-actions" },
        h(
          "button",
          {
            class: "icon-button",
            "data-k": `repositories.details.${repo.path}`,
            "aria-label": `Open ${repo.name}`,
            onClick: () => ctx.openRepository(repo.path),
          },
          icon("chevron-right", 16),
        ),
      ),
    );
  }

  /**
   * Three states, all of them normal: never asked, asked and unavailable, asked
   * and answered. "Unavailable" is not an error — a repository with no
   * workflows, or a machine without GitHub CLI, reaches here routinely.
   */
  function ciCell(ci) {
    if (!ci) return h("span", { class: "muted" }, "—");
    if (!ci.available) {
      return h(
        "span",
        { class: "muted", title: ci.detail ?? "" },
        "unavailable",
      );
    }
    const run = ci.runs[0];
    if (!run) return h("span", { class: "muted" }, "no runs");
    return h(
      "span",
      { class: `status-word ${run.conclusion}`, title: `${run.name} · ${run.branch}` },
      statusGlyph(run.conclusion),
      run.conclusion,
      h("small", null, relativeTime(run.created_at)),
    );
  }

  view.start({
    query: "",
    scanning: false,
    scanProgress: "",
    ci: {},
    ciBusy: false,
    cloneOpen: false,
    cloning: false,
    cloneProfile: "",
    cloneRepo: "",
    cloneParent: "",
    cloneProtocol: "auto",
  });
  return host;
}
