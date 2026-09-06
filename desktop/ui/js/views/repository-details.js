// One repository: what its identity is now, what it would become, and the two
// actions that change it.
//
// This is the only view that writes to a repository, so every write is staged
// behind a second click. Binding over an existing identity and unbinding both
// require confirmation, and the confirmation says what will be overwritten
// rather than just asking "are you sure".
//
// The mockup labels the two selects "Identity" and "Commit Identity" as if they
// were independent. They are not: the commit identity *is* a property of the
// chosen identity, and offering them separately would let someone commit as one
// person while authenticating as another, which is the exact failure this
// product exists to prevent. So the commit identity is shown, sourced from the
// selection, and not separately editable here — it is edited on the identity.
import { badge, displayPath, pageHeader, statusGlyph } from "../components.js";
import { createView, h } from "../dom.js";
import { icon, spinner } from "../icons.js";
import { message } from "../ipc.js";
import { statusLabel } from "../status.js";

export function repositoryDetails(ctx) {
  const host = h("section", null);
  const view = createView(host, build);

  const selected = () => ctx.app.repositories.find((r) => r.path === ctx.app.selectedRepo);
  const profileNamed = (name) => ctx.app.profiles.find((item) => item.name === name);

  // --- actions ------------------------------------------------------------

  async function rescan() {
    try {
      const found = await ctx.api.scan(() => {});
      ctx.app.repositories = found;
      ctx.touch();
    } catch (reason) {
      ctx.fail(reason);
    }
  }

  async function apply() {
    const repo = selected();
    if (!repo) return;
    if (!view.state.confirmBind) {
      view.set({ confirmBind: true, confirmUnbind: false });
      return;
    }
    view.set({ working: true });
    try {
      // A drifted repository carries managed values that no longer match what
      // the identity says; force is what allows those to be overwritten.
      await ctx.api.bind(repo.path, view.state.profile, repo.status === "drifted");
      ctx.signal(`${repo.name} bound to ${view.state.profile}`);
      view.set({ confirmBind: false });
      await rescan();
    } catch (reason) {
      ctx.fail(reason);
    } finally {
      view.set({ working: false });
    }
  }

  async function unbind() {
    const repo = selected();
    if (!repo) return;
    if (!view.state.confirmUnbind) {
      view.set({ confirmUnbind: true, confirmBind: false });
      return;
    }
    view.set({ working: true });
    try {
      await ctx.api.unbind(repo.path);
      ctx.signal(`${repo.name} restored to its original settings`);
      view.set({ confirmUnbind: false });
      await rescan();
    } catch (reason) {
      ctx.fail(reason);
    } finally {
      view.set({ working: false });
    }
  }

  /** Full identity check for this repository, including the network probes. */
  async function inspect(network) {
    const repo = selected();
    if (!repo) return;
    view.set({ inspecting: true });
    try {
      const result = await ctx.api.inspect(repo.path, network);
      view.set({ report: result.report, networkChecked: result.network_checked });
    } catch (reason) {
      ctx.fail(reason);
    } finally {
      view.set({ inspecting: false });
    }
  }

  async function refreshCi() {
    const repo = selected();
    if (!repo) return;
    view.set({ ciBusy: true });
    try {
      view.set({ ci: await ctx.api.ciStatus(repo.path, 5) });
    } catch (reason) {
      ctx.fail(reason);
    } finally {
      view.set({ ciBusy: false });
    }
  }

  /**
   * Hook state is reported, never assumed. `paths()` refuses outright when
   * `core.hooksPath` is set, so the error is a state to show rather than a
   * failure to raise as a toast.
   */
  async function loadHooks() {
    const repo = selected();
    if (!repo) return;
    try {
      view.set({ hooks: await ctx.api.hookState(repo.path), hooksError: null });
    } catch (reason) {
      view.set({ hooks: null, hooksError: message(reason) });
    }
  }

  async function changeHooks(install) {
    const repo = selected();
    if (!repo) return;
    view.set({ hooksBusy: true });
    try {
      const state = install
        ? await ctx.api.installHooks(repo.path)
        : await ctx.api.uninstallHooks(repo.path);
      view.set({ hooks: state, hooksError: null });
      ctx.signal(install ? "Commit hooks installed" : "Commit hooks removed");
    } catch (reason) {
      ctx.fail(reason);
    } finally {
      view.set({ hooksBusy: false });
    }
  }

  /**
   * Who actually authored the commits already in the repository.
   *
   * The checks above describe how the repository is configured *now* and say
   * nothing about its history: a repository can pass every one of them and
   * still carry a commit made last week under the wrong address. This is the
   * only view of that. Local — it reads the commit log and verifies signatures,
   * and reaches no network.
   */
  async function runAudit() {
    const repo = selected();
    if (!repo) return;
    view.set({ auditing: true });
    try {
      view.set({ audit: await ctx.api.audit(repo.path, view.state.range.trim() || "HEAD") });
    } catch (reason) {
      ctx.fail(reason);
    } finally {
      view.set({ auditing: false });
    }
  }

  async function copyRemote() {
    const repo = selected();
    const url = repo?.remote?.url;
    if (!url) return;
    try {
      await ctx.api.copyText(url);
      ctx.signal("Remote URL copied");
    } catch (reason) {
      ctx.fail(reason);
    }
  }

  async function openFolder() {
    const repo = selected();
    if (!repo) return;
    try {
      await ctx.api.openPath(repo.path);
    } catch (reason) {
      ctx.fail(reason);
    }
  }

  // --- rendering ----------------------------------------------------------

  function build(state) {
    const repo = selected();
    if (!repo) {
      return h(
        "div",
        { class: "empty-inspector" },
        icon("git-branch", 24),
        h("h2", null, "No repository selected"),
        h(
          "button",
          { class: "secondary", "data-k": "details.back", onClick: () => ctx.go("repositories") },
          icon("arrow-left", 16),
          "Back to repositories",
        ),
      );
    }
    return [
      pageHeader(repo.name, displayPath(repo.path), [
        h(
          "button",
          { class: "secondary", "data-k": "details.open", onClick: () => void openFolder() },
          icon("folder", 16),
          "Open folder",
        ),
        h(
          "button",
          { class: "secondary", "data-k": "details.back", onClick: () => ctx.go("repositories") },
          icon("arrow-left", 16),
          "Back",
        ),
      ]),
      repo.detail
        ? h(
          "div",
          { class: "inline-alert" },
          icon("circle-alert", 17),
          h(
            "div",
            null,
            h("strong", null, "Identity drift detected"),
            h("p", null, repo.detail),
          ),
        )
        : null,
      h(
        "div",
        { class: "detail-columns" },
        configurationCard(state, repo),
        sideColumn(state, repo),
      ),
    ];
  }

  function configurationCard(state, repo) {
    const chosen = profileNamed(state.profile);
    return h(
      "div",
      { class: "card" },
      h(
        "div",
        { class: "card-head" },
        h("h2", null, "Configuration"),
        badge(repo.status, statusLabel(repo.status)),
      ),
      h(
        "div",
        { class: "field-row" },
        h("span", { class: "field-label" }, "Remote"),
        h(
          "span",
          { class: "input-action" },
          h("code", { class: "remote-url" }, repo.remote?.url ?? "No origin remote"),
          repo.remote?.url
            ? h(
              "button",
              {
                class: "icon-button",
                "data-k": "details.copyRemote",
                "aria-label": "Copy remote URL",
                onClick: () => void copyRemote(),
              },
              icon("copy", 15),
            )
            : null,
        ),
      ),
      h(
        "label",
        { class: "field-row" },
        h("span", { class: "field-label" }, "Identity"),
        h(
          "select",
          {
            "data-k": "details.profile",
            value: state.profile,
            onChange: (event) => view.set({ profile: event.target.value, confirmBind: false }),
          },
          ctx.app.profiles.map((item) => h("option", { value: item.name }, item.name)),
        ),
      ),
      h(
        "div",
        { class: "field-row" },
        h("span", { class: "field-label" }, "Commit identity"),
        h(
          "span",
          { class: "field-readout" },
          chosen ? `${chosen.profile.git_name} <${chosen.profile.git_email}>` : "—",
          h(
            "small",
            null,
            "Set on the identity, not per repository, so commits and authentication cannot disagree.",
          ),
        ),
      ),
      h(
        "div",
        { class: "field-row" },
        h("span", { class: "field-label" }, "Authentication"),
        h(
          "span",
          { class: "field-readout" },
          chosen?.profile.ssh_key ? `SSH · ${chosen.profile.ssh_key}` : "HTTPS · credential helper",
        ),
      ),
      h(
        "div",
        { class: "field-row" },
        h("span", { class: "field-label" }, "Commit signing"),
        h(
          "span",
          { class: "field-readout" },
          chosen?.profile.signing_key
            ? `${(chosen.profile.signing_format ?? "openpgp").toUpperCase()} · ${
              chosen.profile.require_signing ? "required" : "optional"
            }`
            : "Disabled",
          !chosen?.profile.signing_key
            ? h(
              "button",
              {
                class: "link-button",
                "data-k": "details.enableSigning",
                onClick: () => ctx.go("identities"),
              },
              "Enable on the identity",
            )
            : null,
        ),
      ),
      state.confirmBind ? bindPreview(state, repo) : null,
      state.confirmUnbind ? unbindPreview() : null,
      h(
        "div",
        { class: "button-row" },
        h(
          "button",
          {
            class: "primary",
            "data-k": "details.apply",
            disabled: state.working || ctx.app.profiles.length === 0,
            onClick: () => void apply(),
          },
          state.working ? spinner(16) : icon("git-branch", 16),
          state.confirmBind ? "Confirm and write" : "Apply configuration",
        ),
        state.confirmBind || state.confirmUnbind
          ? h(
            "button",
            {
              class: "secondary",
              "data-k": "details.cancel",
              onClick: () => view.set({ confirmBind: false, confirmUnbind: false }),
            },
            "Cancel",
          )
          : h(
            "button",
            {
              class: "secondary",
              "data-k": "details.reset",
              onClick: () =>
                view.set({ profile: repo.bound_profile ?? ctx.app.profiles[0]?.name ?? "" }),
            },
            "Reset",
          ),
        repo.bound_profile && !state.confirmBind
          ? h(
            "button",
            { class: "danger-text", "data-k": "details.unbind", onClick: () => void unbind() },
            state.confirmUnbind ? "Confirm unbind" : "Unbind and restore",
          )
          : null,
      ),
      h(
        "p",
        { class: "safe-note" },
        icon("shield-check", 15),
        "This writes repository-local Git settings only. Your active GitHub CLI account is not switched.",
      ),
    );
  }

  function bindPreview(state, repo) {
    return h(
      "div",
      { class: "rebind-preview" },
      h("strong", null, "Confirm repository-local changes"),
      h(
        "span",
        null,
        "Author and email will be replaced by the values from ",
        h("b", null, state.profile),
        ".",
        repo.status === "drifted" ? " Drifted managed values will be overwritten." : "",
        " The values from before the first bind stay available for unbind.",
      ),
    );
  }

  function unbindPreview() {
    return h(
      "div",
      { class: "rebind-preview" },
      h("strong", null, "Restore the original settings"),
      h(
        "span",
        null,
        "The exact repository-local values saved before the first bind are written back, and GitBound stops managing this repository.",
      ),
    );
  }

  function sideColumn(state, repo) {
    return h(
      "div",
      { class: "detail-side" },
      checksCard(state),
      auditCard(state),
      hooksCard(state),
      ciCard(state, repo),
    );
  }

  /**
   * Authorship of the commits already in the repository, which no other card
   * here reports. Never run on mount, unlike the hooks: a range can cover
   * thousands of commits, each needing a signature verification.
   */
  function auditCard(state) {
    return h(
      "div",
      { class: "card" },
      h(
        "div",
        { class: "card-head" },
        h("h2", null, "Commit authorship"),
        h(
          "span",
          { class: "input-action" },
          h("input", {
            type: "text",
            "data-k": "details.range",
            "aria-label": "Revision range to audit",
            placeholder: "HEAD",
            value: state.range,
            onInput: (event) => view.set({ range: event.target.value }),
          }),
          h(
            "button",
            {
              class: "secondary",
              "data-k": "details.audit",
              disabled: state.auditing,
              onClick: () => void runAudit(),
            },
            state.auditing ? spinner(15) : icon("search", 15),
            "Audit",
          ),
        ),
      ),
      state.audit
        ? h(
          "ul",
          { class: "check-list" },
          state.audit.checks.map((check) =>
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
          "Not audited yet. A range such as origin/main..HEAD reports who authored those commits, judged against the repository's committed policy.",
        ),
    );
  }

  /**
   * The pre-commit and pre-push hooks. Everything else in this view reports
   * what a repository looks like; this is the only thing that stops a bad
   * commit from being made at all, and until now it was reachable only from the
   * command line.
   */
  function hooksCard(state) {
    const installed = state.hooks?.pre_commit === "installed"
      && state.hooks?.pre_push === "installed";
    // Neither install nor uninstall will touch a hook GitBound did not write,
    // so offering a button here would promise something the backend refuses.
    const foreign = [state.hooks?.pre_commit, state.hooks?.pre_push].some((value) =>
      value === "occupied by another hook"
    );
    return h(
      "div",
      { class: "card" },
      h(
        "div",
        { class: "card-head" },
        h("h2", null, "Commit hooks"),
        state.hooks && !foreign
          ? h(
            "button",
            {
              class: "secondary",
              "data-k": "details.hooks",
              disabled: state.hooksBusy,
              onClick: () => void changeHooks(!installed),
            },
            state.hooksBusy ? spinner(15) : icon(installed ? "trash-2" : "shield-check", 15),
            installed ? "Remove" : "Install",
          )
          : null,
      ),
      state.hooksError
        ? h("p", { class: "empty-copy" }, state.hooksError)
        : state.hooks
        ? [
          h(
            "ul",
            { class: "check-list" },
            [
              ["pre-commit", state.hooks.pre_commit],
              ["pre-push", state.hooks.pre_push],
            ].map(([name, value]) =>
              h(
                "li",
                null,
                statusGlyph(value === "installed" ? "ok" : "unverified"),
                h("span", null, h("strong", null, name), h("small", null, value)),
              )
            ),
          ),
          foreign
            ? h(
              "p",
              { class: "network-note" },
              "A hook that GitBound did not write is already in place. It will not be replaced or removed here.",
            )
            : h(
              "p",
              { class: "network-note" },
              installed
                ? "Commits and pushes that do not match this identity are refused before they happen."
                : "Without these, a mismatched identity is only reported after the commit exists.",
            ),
        ]
        : h("p", { class: "empty-copy" }, "Reading hook state…"),
    );
  }

  function checksCard(state) {
    return h(
      "div",
      { class: "card" },
      h(
        "div",
        { class: "card-head" },
        h("h2", null, "Checks"),
        h(
          "button",
          {
            class: "secondary",
            "data-k": "details.inspect",
            disabled: state.inspecting,
            onClick: () => void inspect(true),
          },
          state.inspecting ? spinner(15) : icon("refresh-cw", 15),
          "Run checks",
        ),
      ),
      state.report
        ? [
          h(
            "ul",
            { class: "check-list" },
            state.report.checks.map((check) =>
              h(
                "li",
                null,
                statusGlyph(check.status),
                h(
                  "span",
                  null,
                  h("strong", null, check.id),
                  h("small", null, check.message),
                ),
                h("span", { class: `status-word ${check.status}` }, statusLabel(check.status)),
              )
            ),
          ),
          !state.networkChecked
            ? h(
              "p",
              { class: "network-note" },
              "GitHub CLI and SSH were not contacted. Those checks report as unverified.",
            )
            : null,
        ]
        : h(
          "p",
          { class: "empty-copy" },
          "No checks run yet. Running them contacts GitHub CLI and SSH.",
        ),
    );
  }

  function ciCard(state, repo) {
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
            "data-k": "details.ci",
            disabled: state.ciBusy,
            onClick: () => void refreshCi(),
          },
          state.ciBusy ? spinner(15) : icon("activity", 15),
          "Check CI",
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
        "Not checked. This asks GitHub CLI for recent workflow runs, so it is not done automatically.",
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
      return h("p", { class: "empty-copy" }, `No workflow runs found for ${repo.name}.`);
    }
    return h(
      "ul",
      { class: "check-list" },
      state.ci.runs.map((run) =>
        h(
          "li",
          null,
          statusGlyph(run.conclusion),
          h("span", null, h("strong", null, run.name), h("small", null, run.title)),
          h("span", { class: `status-word ${run.conclusion}` }, run.conclusion),
        )
      ),
    );
  }

  const repo = selected();
  view.start({
    profile: repo?.bound_profile ?? ctx.app.profiles[0]?.name ?? "",
    confirmBind: false,
    confirmUnbind: false,
    working: false,
    inspecting: false,
    report: null,
    networkChecked: false,
    ci: null,
    ciBusy: false,
    hooks: null,
    hooksBusy: false,
    hooksError: null,
    audit: null,
    auditing: false,
    range: "",
  });
  // Reading hook state is a few filesystem stats with no network and no writes,
  // so unlike the checks and the CI card it can load on its own. The card would
  // otherwise open on "unknown" and need a click to say something every user
  // wants to know at a glance.
  void loadHooks();
  return host;
}
