// The Ctrl/Cmd+K palette.
//
// Two deliberate departures from the mockup, both for the same reason: a
// keyboard shortcut that mutates state in one keystroke is exactly the kind of
// accident this product exists to prevent.
//
//   1. The mockup's second target is "Global git configuration". PRODUCT.md
//      rules out mutating global Git identity, and that is a safety property
//      rather than an omission — a global switch silently changes the author on
//      every repository on the machine, including ones the user is not looking
//      at. The second target here is the GitHub CLI account instead: still the
//      machine-wide thing a user wants to switch, but one the product already
//      switches explicitly, and one that touches no repository.
//
//   2. Choosing a target arms the action; it does not perform it. The palette
//      then shows exactly what will change, and a second, deliberate press
//      commits. Fast to reach, never fast to fire.
import { initials } from "../components.js";
import { createView, h } from "../dom.js";
import { icon, spinner } from "../icons.js";

const TARGETS = [
  {
    id: "repository",
    label: "Current repository",
    hint: "Bind the selected repository to this identity",
  },
  {
    id: "github-cli",
    label: "GitHub CLI account",
    hint: "Switch the account gh authenticates as, machine-wide",
  },
];

export function quickSwitch(ctx) {
  const host = h("div", { class: "modal-scrim" });
  const view = createView(host, build);

  const repo = () => ctx.app.repositories.find((item) => item.path === ctx.app.selectedRepo);

  function matches(state) {
    const needle = state.query.trim().toLowerCase();
    if (!needle) return ctx.app.profiles;
    return ctx.app.profiles.filter((item) =>
      `${item.name} ${item.profile.github_user} ${item.profile.git_email}`
        .toLowerCase()
        .includes(needle)
    );
  }

  /** First press selects and arms. Second press, on the armed one, commits. */
  function choose(name) {
    if (view.state.armed === name) {
      void commit(name);
      return;
    }
    view.set({ armed: name });
  }

  async function commit(name) {
    const state = view.state;
    view.set({ working: true });
    try {
      if (state.target === "repository") {
        const selected = repo();
        if (!selected) {
          ctx.fail({ message: "Select a repository first." });
          return;
        }
        await ctx.api.bind(selected.path, name, selected.status === "drifted");
        ctx.signal(`${selected.name} bound to ${name}`);
      } else {
        await ctx.api.switchAccount(name);
        ctx.signal(`GitHub CLI switched to ${name}`);
      }
      ctx.closeQuickSwitch();
      // Both branches changed state the frontend only knows about by asking:
      // a bind moves the repository's status, a switch moves the active
      // account. Re-read rather than leave the screen behind the machine.
      await ctx.reload();
    } catch (reason) {
      ctx.fail(reason);
    } finally {
      view.set({ working: false, armed: "" });
    }
  }

  function build(state) {
    return h(
      "div",
      {
        class: "modal",
        role: "dialog",
        "aria-modal": "true",
        "aria-label": "Quick switch identity",
      },
      h(
        "header",
        { class: "modal-head" },
        h("h2", null, "Quick switch"),
        h(
          "button",
          {
            class: "icon-button",
            "data-k": "quick.close",
            "aria-label": "Close",
            onClick: () => ctx.closeQuickSwitch(),
          },
          icon("x", 16),
        ),
      ),
      h(
        "div",
        { class: "search" },
        icon("search", 15),
        h("input", {
          "aria-label": "Search identities",
          "data-k": "quick.query",
          value: state.query,
          placeholder: "Search identities",
          onInput: (event) => view.set({ query: event.target.value, armed: "" }),
        }),
      ),
      h("div", { class: "quick-grid" }, matches(state).map((item) => chip(state, item))),
      matches(state).length === 0
        ? h("p", { class: "empty-copy pad" }, "No identity matches that search.")
        : null,
      targets(state),
      state.armed ? preview(state) : null,
      h(
        "footer",
        { class: "modal-foot" },
        h(
          "span",
          { class: "muted" },
          state.armed
            ? "Press the highlighted identity again, or Switch, to apply."
            : "Choose an identity to see what will change.",
        ),
        h(
          "div",
          { class: "button-row" },
          h(
            "button",
            { class: "secondary", "data-k": "quick.cancel", onClick: () => ctx.closeQuickSwitch() },
            "Cancel",
          ),
          h(
            "button",
            {
              class: "primary",
              "data-k": "quick.switch",
              disabled: !state.armed || state.working,
              onClick: () => void commit(state.armed),
            },
            state.working ? spinner(16) : icon("arrow-right-left", 16),
            "Switch",
          ),
        ),
      ),
    );
  }

  function chip(state, item) {
    return h(
      "button",
      {
        class: `quick-chip ${state.armed === item.name ? "armed" : ""}`,
        "data-k": `quick.chip.${item.name}`,
        "aria-pressed": state.armed === item.name ? "true" : "false",
        onClick: () => choose(item.name),
      },
      h("span", { class: "avatar", "aria-hidden": "true" }, initials(item.profile.git_name)),
      h(
        "span",
        null,
        h("strong", null, item.name),
        h("small", null, item.profile.git_email),
      ),
    );
  }

  function targets(state) {
    return h(
      "fieldset",
      { class: "quick-targets" },
      h("legend", null, "Apply to"),
      TARGETS.map((target) =>
        h(
          "label",
          { class: state.target === target.id ? "selected" : "" },
          h("input", {
            type: "radio",
            name: "quick-target",
            "data-k": `quick.target.${target.id}`,
            checked: state.target === target.id,
            onChange: () => view.set({ target: target.id, armed: "" }),
          }),
          h("span", null, h("strong", null, target.label), h("small", null, target.hint)),
        )
      ),
    );
  }

  /** What the armed action will actually do, in the words of the thing it does. */
  function preview(state) {
    const selected = repo();
    if (state.target === "repository") {
      return h(
        "div",
        { class: "rebind-preview" },
        h("strong", null, selected ? `Bind ${selected.name}` : "No repository selected"),
        h(
          "span",
          null,
          selected
            ? `Repository-local author, email, and SSH command become the values from ${state.armed}. Your GitHub CLI account is not touched.`
            : "Open a repository first, or switch the GitHub CLI account instead.",
        ),
      );
    }
    return h(
      "div",
      { class: "rebind-preview" },
      h("strong", null, "Switch the GitHub CLI account"),
      h(
        "span",
        null,
        `gh will authenticate as ${
          ctx.app.profiles.find((item) => item.name === state.armed)?.profile.github_user
        } on this machine. No repository is modified.`,
      ),
    );
  }

  view.start({
    query: "",
    // Repository-scoped by default: it is the narrower of the two, and the one
    // that cannot affect anything the user is not looking at.
    target: "repository",
    armed: "",
    working: false,
  });
  return host;
}
