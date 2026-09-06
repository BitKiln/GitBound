// The four-step Add Identity form.
//
// One profile form was already shared between onboarding and the editor
// (profile-fields.js). This splits the same fields across four labelled steps,
// because presenting nine fields at once asks the user to understand the whole
// model before they can fill in their own name, and the four groups happen to be
// exactly the four identities the product treats as separate concerns:
//
//   1. Git identity          — who commits say you are
//   2. GitHub account        — who the CLI authenticates as
//   3. SSH key and signing   — how you prove it
//   4. Review                — what will be written
//
// Like profile-fields.js this owns no state: the caller passes state in and
// receives patches, so a keystroke re-renders the caller's view and the caret
// survives via each control's data-k.
import { h } from "../dom.js";
import { icon, spinner } from "../icons.js";

export const STEPS = [
  { id: 1, label: "Git Identity" },
  { id: 2, label: "GitHub Account" },
  { id: 3, label: "SSH Key" },
  { id: 4, label: "Finish" },
];

export const EMPTY_DRAFT = {
  github_user: "",
  git_name: "",
  git_email: "",
  hostname: "github.com",
  allowed_owners: [],
  signing_format: "openpgp",
  require_signing: false,
};

/**
 * Which fields a step needs before it can be left. Returned as a message rather
 * than a boolean so the Next button can say why it is disabled instead of just
 * being grey.
 */
export function stepBlocker(step, state) {
  const { name, draft } = state;
  if (step === 1) {
    if (!name.trim()) return "Give the identity a name.";
    if (!draft.git_name.trim()) return "A Git author name is required.";
    if (!draft.git_email.trim()) return "A Git author email is required.";
    if (!draft.git_email.includes("@")) return "That does not look like an email address.";
    return "";
  }
  if (step === 2) {
    if (!draft.github_user.trim()) return "A GitHub username is required.";
    if (!draft.hostname.trim()) return "A hostname is required.";
    return "";
  }
  // Step 3 is entirely optional: an identity with no SSH key is valid and uses
  // HTTPS with a credential helper.
  return "";
}

/**
 * @param opts.state     {step, name, draft, pickerError, working}
 * @param opts.set       (patch) => void
 * @param opts.onSubmit  () => void
 * @param opts.onCancel  () => void
 * @param opts.onPickKey () => void
 * @param opts.submitLabel text for the final button
 */
export function identityWizard(opts) {
  const { state, set, onSubmit, onCancel, onPickKey, submitLabel = "Create identity" } = opts;
  const field = (key, value) => set({ draft: { ...state.draft, [key]: value } });
  const optional = (key, value) =>
    set({ draft: { ...state.draft, [key]: value.trim() || undefined } });
  const blocker = stepBlocker(state.step, state);

  return h(
    "div",
    { class: "wizard" },
    stepper(state.step),
    h("div", { class: "wizard-body" }, body()),
    h(
      "div",
      { class: "wizard-foot" },
      blocker ? h("span", { class: "field-error", role: "status" }, blocker) : h("span", null),
      h(
        "div",
        { class: "button-row" },
        h(
          "button",
          {
            class: "secondary",
            "data-k": "wizard.back",
            onClick: () => (state.step === 1 ? onCancel() : set({ step: state.step - 1 })),
          },
          state.step === 1 ? "Cancel" : "Back",
        ),
        state.step < STEPS.length
          ? h(
            "button",
            {
              class: "primary",
              "data-k": "wizard.next",
              disabled: Boolean(blocker),
              onClick: () => set({ step: state.step + 1 }),
            },
            "Next",
          )
          : h(
            "button",
            {
              class: "primary",
              "data-k": "wizard.submit",
              disabled: state.working || Boolean(anyBlocker(state)),
              onClick: () => onSubmit(),
            },
            state.working ? spinner(16) : icon("check", 16),
            submitLabel,
          ),
      ),
    ),
  );

  function body() {
    if (state.step === 1) return stepGitIdentity();
    if (state.step === 2) return stepGitHubAccount();
    if (state.step === 3) return stepSshKey();
    return stepReview();
  }

  function stepGitIdentity() {
    return [
      h("h2", null, "Git Identity"),
      h("p", null, "The author recorded on every commit made under this identity."),
      h(
        "div",
        { class: "form-grid" },
        h(
          "label",
          null,
          "Identity name",
          h("input", {
            "data-k": "wizard.name",
            value: state.name,
            placeholder: "personal",
            disabled: state.nameLocked,
            onInput: (event) => set({ name: event.target.value }),
          }),
          h("small", { class: "field-hint" }, "How you will refer to it. Not sent anywhere."),
        ),
        h(
          "label",
          null,
          "Git author name",
          h("input", {
            "data-k": "wizard.git_name",
            value: state.draft.git_name,
            placeholder: "Ada Lovelace",
            onInput: (event) => field("git_name", event.target.value),
          }),
        ),
        h(
          "label",
          { class: "span-2" },
          "Git author email",
          h("input", {
            "data-k": "wizard.git_email",
            type: "email",
            value: state.draft.git_email,
            placeholder: "ada@example.com",
            onInput: (event) => field("git_email", event.target.value),
          }),
        ),
      ),
    ];
  }

  function stepGitHubAccount() {
    return [
      h("h2", null, "GitHub Account"),
      h(
        "p",
        null,
        "Which account this identity expects. GitBound checks it and never signs in for you — authentication stays with GitHub CLI.",
      ),
      h(
        "div",
        { class: "form-grid" },
        h(
          "label",
          null,
          "GitHub username",
          h("input", {
            "data-k": "wizard.github_user",
            value: state.draft.github_user,
            placeholder: "octocat",
            onInput: (event) => field("github_user", event.target.value),
          }),
        ),
        h(
          "label",
          null,
          "Hostname",
          h("input", {
            "data-k": "wizard.hostname",
            value: state.draft.hostname,
            onInput: (event) => field("hostname", event.target.value),
          }),
          h("small", { class: "field-hint" }, "github.com, or your Enterprise host."),
        ),
        h(
          "label",
          { class: "span-2" },
          "Allowed repository owners",
          h("input", {
            "data-k": "wizard.allowed_owners",
            value: state.draft.allowed_owners.join(", "),
            placeholder: "organization, username",
            onInput: (event) =>
              set({
                draft: {
                  ...state.draft,
                  allowed_owners: event.target.value
                    .split(",")
                    .map((value) => value.trim())
                    .filter(Boolean),
                },
              }),
          }),
          h(
            "small",
            { class: "field-hint" },
            "Leave empty to allow any owner. Listing owners is what turns a wrong-account push into a failed check.",
          ),
        ),
      ),
    ];
  }

  function stepSshKey() {
    const format = state.draft.signing_format ?? "openpgp";
    return [
      h("h2", null, "SSH Key and Signing"),
      h("p", null, "Optional. Without a key this identity uses HTTPS and a credential helper."),
      h(
        "div",
        { class: "form-grid" },
        h(
          "div",
          { class: "form-field span-2" },
          h("label", { for: "wizard-ssh-key" }, "SSH private key"),
          h(
            "span",
            { class: "input-action" },
            h("input", {
              id: "wizard-ssh-key",
              "data-k": "wizard.ssh_key",
              value: state.draft.ssh_key ?? "",
              placeholder: "~/.ssh/id_ed25519",
              onInput: (event) => optional("ssh_key", event.target.value),
            }),
            h(
              "button",
              { type: "button", class: "secondary", onClick: onPickKey },
              icon("folder", 15),
              "Browse",
            ),
          ),
          h(
            "small",
            { class: "field-hint" },
            "Choose the private key, not the matching ",
            h("code", null, ".pub"),
            " file.",
          ),
          state.pickerError
            ? h("small", { class: "field-error", role: "alert" }, state.pickerError)
            : null,
        ),
        h(
          "label",
          { class: "span-2" },
          "SSH host alias",
          h("input", {
            "data-k": "wizard.ssh_host",
            value: state.draft.ssh_host ?? "",
            placeholder: state.draft.hostname,
            onInput: (event) => optional("ssh_host", event.target.value),
          }),
          h(
            "small",
            { class: "field-hint" },
            "Only if your SSH config gives this account its own ",
            h("code", null, "Host"),
            " entry, such as ",
            h("code", null, "github.com-work"),
            ". The hostname above stays the real one.",
          ),
        ),
        h(
          "label",
          null,
          "Signing format",
          h(
            "select",
            {
              "data-k": "wizard.signing_format",
              value: format,
              onChange: (event) => field("signing_format", event.target.value),
            },
            h("option", { value: "openpgp" }, "OpenPGP"),
            h("option", { value: "ssh" }, "SSH"),
          ),
        ),
        h(
          "label",
          null,
          "Signing key",
          h("input", {
            "data-k": "wizard.signing_key",
            value: state.draft.signing_key ?? "",
            placeholder: format === "ssh" ? "Public key path or key:: value" : "OpenPGP key ID",
            onInput: (event) => optional("signing_key", event.target.value),
          }),
        ),
        h(
          "label",
          { class: "check-row span-2" },
          h("input", {
            type: "checkbox",
            "data-k": "wizard.require_signing",
            checked: state.draft.require_signing,
            onChange: (event) => field("require_signing", event.target.checked),
          }),
          h("span", null, "Require signed commits for repositories bound to this identity"),
        ),
      ),
    ];
  }

  function stepReview() {
    const row = (term, value) =>
      h("div", null, h("dt", null, term), h("dd", null, value || h("em", null, "not set")));
    return [
      h("h2", null, "Review"),
      h("p", null, "Nothing has been written yet. This is what will be saved."),
      h(
        "dl",
        { class: "identity-grid wide" },
        row("Identity name", state.name),
        row("Git author", state.draft.git_name),
        row("Git email", state.draft.git_email),
        row("GitHub user", state.draft.github_user),
        row("Hostname", state.draft.hostname),
        row("SSH key", state.draft.ssh_key),
        row("SSH host alias", state.draft.ssh_host),
        row(
          "Allowed owners",
          state.draft.allowed_owners.length ? state.draft.allowed_owners.join(", ") : "any owner",
        ),
        row(
          "Signing",
          state.draft.signing_key
            ? `${state.draft.signing_format} · ${
              state.draft.require_signing ? "required" : "optional"
            }`
            : "",
        ),
      ),
      h(
        "div",
        { class: "safe-note" },
        icon("shield-check", 16),
        "Saving records this identity locally. It changes no repository until you bind one, and never switches your GitHub CLI account on its own.",
      ),
    ];
  }
}

/** The first step that is still incomplete, so Save cannot skip a gap. */
function anyBlocker(state) {
  for (const step of STEPS) {
    const blocker = stepBlocker(step.id, state);
    if (blocker) return blocker;
  }
  return "";
}

function stepper(current) {
  return h(
    "ol",
    { class: "stepper", "aria-label": "Progress" },
    STEPS.map((step) =>
      h(
        "li",
        {
          class: step.id === current ? "current" : step.id < current ? "done" : "",
          "aria-current": step.id === current ? "step" : null,
        },
        h(
          "span",
          { class: "stepper-dot" },
          step.id < current ? icon("check", 13) : String(step.id),
        ),
        h("span", null, step.label),
      )
    ),
  );
}
