// First run.
//
// Two ways in, and the order matters: importing from a repository you already
// have is offered first, because the answers are already on disk and reading
// them is more reliable than asking someone to retype their own email.
// Starting from scratch is the fallback, not the default.
//
// Deliberately narrow either way. Only the chosen folder is inspected — no disk
// scan happens here — and everything is shown for confirmation before anything
// is written.
import { createView, h } from "../dom.js";
import { icon } from "../icons.js";
import { EMPTY_DRAFT, identityWizard } from "./identity-wizard.js";

export function onboarding(ctx) {
  const host = h("section", { class: "onboarding" });
  const view = createView(host, build);

  /** Read an existing repository's identity and pre-fill the wizard with it. */
  async function importFromRepository() {
    try {
      const selected = await ctx.api.chooseFolder();
      if (!selected) return;
      const preview = await ctx.api.importPreview(selected);
      view.set({
        path: selected,
        mode: "wizard",
        step: 1,
        name: "personal",
        draft: {
          github_user: preview.github_user ?? "",
          git_name: preview.git_name ?? "",
          git_email: preview.git_email ?? "",
          hostname: preview.hostname,
          ssh_host: preview.ssh_host ?? undefined,
          allowed_owners: preview.allowed_owners,
          signing_key: preview.signing_key,
          signing_format: preview.signing_format,
          require_signing: preview.require_signing,
        },
      });
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

  async function finish() {
    const state = view.state;
    view.set({ working: true });
    try {
      await ctx.api.createProfile(state.name, state.draft);
      // Only bind when the identity came from a repository. Someone who started
      // from scratch has not chosen a repository yet, and binding one they did
      // not pick would be exactly the kind of surprise this product exists to
      // prevent.
      if (state.path) await ctx.api.bind(state.path, state.name);
      // Reloading swaps this wizard out for the main views, because the shell
      // only shows onboarding while no profile exists.
      await ctx.reload();
      ctx.signal(state.path ? `${state.name} created and bound` : `${state.name} created`);
    } catch (reason) {
      ctx.fail(reason);
    } finally {
      view.set({ working: false });
    }
  }

  function build(state) {
    return [
      copyPane(state),
      h(
        "div",
        { class: "setup-panel" },
        state.mode === "choose" ? chooser() : wizard(state),
      ),
    ];
  }

  function copyPane(state) {
    const item = (index, label) =>
      h("li", { class: state.mode === "wizard" && state.step >= index ? "current" : "" }, label);
    return h(
      "div",
      { class: "onboarding-copy" },
      h("div", { class: "onboarding-icon" }, icon("shield-check", 24)),
      h("h1", null, "One identity at a time"),
      h(
        "p",
        null,
        "GitBound keeps commit identity, GitHub account, SSH key, and signing as four separate things it can check independently. Set up the first one and nothing on disk changes until you say so.",
      ),
      h(
        "ol",
        null,
        item(1, "Git identity"),
        item(2, "GitHub account"),
        item(3, "SSH key"),
        item(4, "Review"),
      ),
    );
  }

  function chooser() {
    return [
      h("h2", null, "Set up your first identity"),
      h("p", null, "Only the folder you choose is inspected. Nothing else on disk is scanned."),
      h(
        "button",
        {
          class: "drop-target",
          "data-k": "onboarding.import",
          onClick: () => void importFromRepository(),
        },
        icon("folder-plus", 24),
        h("strong", null, "Import from a repository"),
        h("span", null, "Reads the author, remote, and signing settings already configured there"),
      ),
      h(
        "button",
        {
          class: "secondary wide",
          "data-k": "onboarding.scratch",
          onClick: () => view.set({ mode: "wizard", step: 1, path: "" }),
        },
        icon("plus", 16),
        "Start from scratch instead",
      ),
    ];
  }

  function wizard(state) {
    return [
      state.path ? h("div", { class: "selected-path" }, icon("folder", 16), state.path) : null,
      identityWizard({
        state,
        set: (patch) => view.set(patch),
        onSubmit: () => void finish(),
        onCancel: () => view.set({ mode: "choose", path: "" }),
        onPickKey: () => void pickKey(),
        submitLabel: state.path ? "Create identity and bind" : "Create identity",
      }),
    ];
  }

  view.start({
    mode: "choose",
    path: "",
    draft: EMPTY_DRAFT,
    name: "personal",
    step: 1,
    working: false,
    nameLocked: false,
    pickerError: "",
  });
  return host;
}
