// Identities: create, edit, duplicate, and remove the records that repositories
// get bound to. Also the only place the active GitHub CLI account can be
// switched, and that stays explicit and separate from binding.
//
// "Identity" is the word on screen; `profile` is the word in config.toml, in
// the CLI, and in every Rust type. Renaming the concept would break every
// existing user's configuration and every documented command in exchange for a
// label, so the two words mean the same thing and the user guide says so.
//
// The card grid replaced a list/detail split. The list showed one identity's
// details at a time, which is the wrong shape for the question this view
// actually answers — "which of these am I using, and where?" — and that
// question is a comparison.
import { badge, initials, pageHeader } from "../components.js";
import { createView, h } from "../dom.js";
import { icon } from "../icons.js";
import { EMPTY_DRAFT, identityWizard } from "./identity-wizard.js";

export function identities(ctx) {
  const host = h("section", null);
  const view = createView(host, build);

  const list = () => ctx.app.profiles;
  const byName = (name) => list().find((item) => item.name === name);
  const repositoriesFor = (name) =>
    ctx.app.repositories.filter((repo) => repo.bound_profile === name);

  // --- actions ------------------------------------------------------------

  async function save() {
    const state = view.state;
    const trimmed = state.name.trim();
    if (!trimmed) {
      ctx.fail({ message: "Identity name cannot be empty" });
      return;
    }
    view.set({ working: true });
    try {
      if (state.mode === "new") {
        await ctx.api.createProfile(trimmed, state.draft);
        ctx.signal(`Identity '${trimmed}' saved`);
      } else if (trimmed !== state.editing) {
        // Rename first: updateProfile addresses the profile by its new name,
        // so the order here is load-bearing.
        await ctx.api.renameProfile(state.editing, trimmed);
        await ctx.api.updateProfile(trimmed, state.draft);
        ctx.signal(`Identity renamed to '${trimmed}' and updated`);
      } else {
        await ctx.api.updateProfile(trimmed, state.draft);
        ctx.signal(`Identity '${trimmed}' saved`);
      }
      view.set({ mode: "grid" });
      await ctx.reload();
    } catch (reason) {
      ctx.fail(reason);
    } finally {
      view.set({ working: false });
    }
  }

  /** Seed a new identity from an existing one under a free name. */
  function duplicate(name) {
    const existing = byName(name);
    let candidate = `${existing.name}-copy`;
    let counter = 2;
    while (list().some((item) => item.name === candidate)) {
      candidate = `${existing.name}-copy-${counter}`;
      counter += 1;
    }
    view.set({
      mode: "new",
      step: 1,
      name: candidate,
      draft: { ...existing.profile },
      menu: "",
      pickerError: "",
    });
  }

  async function remove(name) {
    try {
      await ctx.api.removeProfile(name);
      ctx.signal(`Identity '${name}' removed`);
      view.set({ confirmRemove: "", menu: "" });
      await ctx.reload();
    } catch (reason) {
      ctx.fail(reason);
    }
  }

  async function previewSwitch(name) {
    const existing = byName(name);
    try {
      const accounts = await ctx.api.accounts(existing.profile.hostname);
      view.set({
        menu: "",
        switchPreview: {
          name,
          current: accounts.find((account) => account.active)?.login ?? "No active account",
          target: existing.profile.github_user,
        },
      });
    } catch (reason) {
      ctx.fail(reason);
    }
  }

  async function switchAccount(name) {
    try {
      await ctx.api.switchAccount(name);
      ctx.signal(`GitHub CLI switched to ${byName(name).profile.github_user}`);
      view.set({ switchPreview: null });
      // Which account is active is backend state, not something the frontend
      // can derive from the call it just made, so it has to be re-read. Without
      // this the "Active" badge keeps pointing at the previous account until
      // the app is restarted, which is the exact wrong answer from the one
      // screen whose job is to report the current identity.
      await ctx.reload();
    } catch (reason) {
      ctx.fail(reason);
    }
  }

  async function pickKey() {
    view.set({ pickerError: "" });
    try {
      const selected = await ctx.api.chooseKeyFile();
      if (selected) view.set({ draft: { ...view.state.draft, ssh_key: selected } });
    } catch (reason) {
      view.set({ pickerError: reason?.message ?? String(reason) });
    }
  }

  // --- rendering ----------------------------------------------------------

  function build(state) {
    if (state.mode !== "grid") return editorPage(state);
    return [
      pageHeader("Identities", "Records applied to repositories you explicitly bind.", [
        h(
          "button",
          {
            class: "primary",
            "data-k": "identities.add",
            onClick: () =>
              view.set({
                mode: "new",
                step: 1,
                name: "",
                draft: EMPTY_DRAFT,
                pickerError: "",
              }),
          },
          icon("plus", 16),
          "Add identity",
        ),
      ]),
      searchRow(state),
      grid(state),
      state.switchPreview ? switchPanel(state) : null,
    ];
  }

  function searchRow(state) {
    return h(
      "div",
      { class: "search" },
      icon("search", 15),
      h("input", {
        "aria-label": "Search identities",
        "data-k": "identities.query",
        value: state.query,
        placeholder: "Search identities",
        onInput: (event) => view.set({ query: event.target.value }),
      }),
    );
  }

  function grid(state) {
    const needle = state.query.trim().toLowerCase();
    const filtered = list().filter((item) =>
      `${item.name} ${item.profile.github_user} ${item.profile.git_email}`
        .toLowerCase()
        .includes(needle)
    );
    if (filtered.length === 0) {
      return h("p", { class: "empty-copy pad" }, "No identity matches that search.");
    }
    return h("div", { class: "identity-cards" }, filtered.map((item) => card(state, item)));
  }

  function card(state, item) {
    const profile = item.profile;
    const bound = repositoriesFor(item.name);
    // The card used to badge whichever identity sorted first as "Default".
    // Profiles arrive from a BTreeMap, so that was alphabetical order wearing
    // the costume of a choice, and there is no default-profile concept in the
    // configuration at all. What a reader wants to know here is the same thing
    // the dashboard answers: which account the GitHub CLI is actually using.
    const isActive = Boolean(ctx.app.activeGithubUser)
      && profile.github_user.toLowerCase() === ctx.app.activeGithubUser.toLowerCase();
    return h(
      "article",
      { class: `identity-card ${isActive ? "is-active" : ""}` },
      h(
        "header",
        null,
        h("span", { class: "avatar large", "aria-hidden": "true" }, initials(profile.git_name)),
        h(
          "div",
          { class: "identity-card-title" },
          h("h2", null, item.name),
          isActive ? h("span", { class: "badge ok" }, "Active") : null,
        ),
        h(
          "span",
          {
            class: "identity-card-star",
            title: isActive ? "GitHub CLI is active as this identity" : "",
          },
          isActive ? icon("check", 15, "Active identity") : null,
        ),
      ),
      h(
        "dl",
        { class: "identity-card-facts" },
        fact("users-round", profile.git_name),
        fact("git-branch", profile.git_email),
        fact("shield-check", `@${profile.github_user}`),
        fact("key-round", profile.ssh_key ?? "No SSH key"),
      ),
      h(
        "p",
        { class: "identity-card-count" },
        `${bound.length} ${bound.length === 1 ? "repository" : "repositories"}`,
      ),
      h(
        "div",
        { class: "identity-card-actions" },
        h(
          "button",
          {
            class: "secondary",
            "data-k": `identities.use.${item.name}`,
            onClick: () => void previewSwitch(item.name),
          },
          icon("arrow-right-left", 15),
          "Use",
        ),
        h(
          "button",
          {
            class: "secondary",
            "data-k": `identities.edit.${item.name}`,
            onClick: () =>
              view.set({
                mode: "edit",
                editing: item.name,
                step: 1,
                name: item.name,
                draft: { ...profile },
                pickerError: "",
              }),
          },
          "Edit",
        ),
        h(
          "button",
          {
            class: "icon-button",
            "data-k": `identities.menu.${item.name}`,
            "aria-label": `More actions for ${item.name}`,
            "aria-expanded": state.menu === item.name ? "true" : "false",
            onClick: () => view.set({ menu: state.menu === item.name ? "" : item.name }),
          },
          icon("ellipsis", 16),
        ),
        state.menu === item.name ? menu(state, item) : null,
      ),
      state.confirmRemove === item.name ? removalConfirm(item) : null,
    );
  }

  function fact(iconName, value) {
    return h("div", null, icon(iconName, 14), h("span", null, value));
  }

  function menu(state, item) {
    return h(
      "div",
      { class: "row-menu", role: "menu" },
      h(
        "button",
        {
          role: "menuitem",
          "data-k": `identities.duplicate.${item.name}`,
          onClick: () => duplicate(item.name),
        },
        icon("copy", 15),
        "Duplicate",
      ),
      h(
        "button",
        {
          role: "menuitem",
          "data-k": `identities.remove.${item.name}`,
          class: "danger-text",
          onClick: () => view.set({ confirmRemove: item.name, menu: "" }),
        },
        icon("trash-2", 15),
        "Remove",
      ),
    );
  }

  function removalConfirm(item) {
    return h(
      "div",
      { class: "destructive-zone" },
      h("span", null, "Remove this identity? Bound repositories will report it missing."),
      h(
        "button",
        {
          class: "danger-text",
          "data-k": `identities.confirmRemove.${item.name}`,
          onClick: () => void remove(item.name),
        },
        "Confirm remove",
      ),
      h(
        "button",
        {
          class: "secondary",
          "data-k": `identities.cancelRemove.${item.name}`,
          onClick: () => view.set({ confirmRemove: "" }),
        },
        "Cancel",
      ),
    );
  }

  /**
   * Switching the GitHub CLI account is a machine-wide change, so it is armed
   * and then confirmed, and the panel shows what it is switching *from* as well
   * as to. This is the one place in the app that is not repository-scoped.
   */
  function switchPanel(state) {
    const preview = state.switchPreview;
    // Arming a switch to the account that is already active would run a `gh`
    // command that changes nothing, so the panel says so and offers only a way
    // out. The comparison is case-insensitive because GitHub logins are.
    const alreadyActive = preview.current.toLowerCase() === preview.target.toLowerCase();
    return h(
      "div",
      { class: "explicit-action" },
      h(
        "div",
        null,
        h("strong", null, "GitHub CLI account"),
        h("p", null, "Switching is explicit and separate from repository binding."),
      ),
      h(
        "div",
        { class: "switch-preview" },
        h("span", null, h("small", null, "Current"), preview.current),
        icon("arrow-right-left", 16),
        h("span", null, h("small", null, "Target"), preview.target),
        alreadyActive
          ? h("span", { class: "switch-noop" }, `Already active as ${preview.target}.`)
          : h(
            "button",
            {
              class: "primary",
              "data-k": "identities.confirmSwitch",
              onClick: () => void switchAccount(preview.name),
            },
            "Confirm switch",
          ),
        h(
          "button",
          {
            class: "secondary",
            "data-k": "identities.cancelSwitch",
            onClick: () => view.set({ switchPreview: null }),
          },
          alreadyActive ? "Close" : "Cancel",
        ),
      ),
    );
  }

  function editorPage(state) {
    const bound = state.mode === "edit" ? repositoriesFor(state.editing) : [];
    return [
      pageHeader(
        state.mode === "new" ? "Add identity" : `Edit ${state.editing}`,
        state.mode === "new"
          ? "Nothing is written until the final step."
          : "Changes apply to repositories the next time you bind them.",
        [
          h(
            "button",
            {
              class: "secondary",
              "data-k": "identities.back",
              onClick: () => view.set({ mode: "grid" }),
            },
            icon("arrow-left", 16),
            "Back",
          ),
        ],
      ),
      identityWizard({
        state,
        set: (patch) => view.set(patch),
        onSubmit: () => void save(),
        onCancel: () => view.set({ mode: "grid" }),
        onPickKey: () => void pickKey(),
        submitLabel: state.mode === "new" ? "Create identity" : "Save changes",
      }),
      bound.length
        ? [
          h("h3", null, "Repositories using this identity"),
          h(
            "div",
            { class: "compact-list" },
            bound.map((repo) =>
              h(
                "div",
                null,
                icon("git-branch", 15),
                h("span", null, h("strong", null, repo.name), h("small", null, repo.path)),
                badge(repo.status),
              )
            ),
          ),
          h(
            "p",
            { class: "empty-copy" },
            "Editing an identity does not rewrite these repositories. Rebind each one to apply the change.",
          ),
        ]
        : null,
    ];
  }

  view.start({
    mode: "grid",
    query: "",
    menu: "",
    editing: "",
    step: 1,
    name: "",
    draft: EMPTY_DRAFT,
    working: false,
    confirmRemove: "",
    switchPreview: null,
    nameLocked: false,
    pickerError: "",
  });
  return host;
}
