// SSH & Signing: what each profile has configured, and an authentication test
// that only ever runs when the button is pressed. Opening this view makes no
// network connection.
import { badge, pageHeader, statusGlyph } from "../components.js";
import { createView, h } from "../dom.js";
import { icon } from "../icons.js";

export function ssh(ctx) {
  const host = h("section", null);
  const view = createView(host, build);

  const current = () =>
    ctx.app.profiles.find((item) => item.name === view.state.selected) ?? ctx.app.profiles[0];

  async function test() {
    try {
      view.set({ report: await ctx.api.testSsh(view.state.selected) });
      ctx.signal("SSH authentication test complete");
    } catch (reason) {
      ctx.fail(reason);
    }
  }

  function build(state) {
    const entry = current();
    const profile = entry.profile;
    return [
      pageHeader(
        "SSH Keys",
        "Inspect configured keys and run authentication tests only when requested.",
      ),
      h(
        "div",
        { class: "settings-layout" },
        selector(state),
        authSection(state, profile),
        signingSection(profile),
      ),
    ];
  }

  function selector(state) {
    return h(
      "div",
      { class: "profile-selector" },
      h(
        "label",
        null,
        "Profile",
        h(
          "select",
          {
            "data-k": "ssh.profile",
            value: state.selected,
            // A report belongs to the profile it was run for, so switching
            // profiles clears it rather than showing a stale verdict.
            onChange: (event) => view.set({ selected: event.target.value, report: null }),
          },
          ctx.app.profiles.map((item) => h("option", { value: item.name }, item.name)),
        ),
      ),
    );
  }

  function authSection(state, profile) {
    const cell = (term, value, wide) =>
      h("div", wide ? { class: "wide" } : null, h("dt", null, term), h("dd", null, value));
    return h(
      "div",
      { class: "settings-section" },
      h(
        "div",
        { class: "section-head" },
        h(
          "div",
          null,
          h("h2", null, "SSH authentication"),
          h("p", null, "GitBound never edits ", h("code", null, "~/.ssh/config"), "."),
        ),
        h(
          "button",
          {
            class: "primary",
            onClick: () => void test(),
            disabled: !profile.ssh_key,
            title: profile.ssh_key ? null : "Configure a private SSH key path in Profiles first",
          },
          icon("terminal", 16),
          "Test authentication",
        ),
      ),
      h(
        "dl",
        { class: "identity-grid" },
        cell("Host", profile.ssh_host ?? profile.hostname),
        cell("Expected account", profile.github_user),
        cell("Identity file", profile.ssh_key ?? "No key configured", true),
      ),
      !profile.ssh_key
        ? h(
          "div",
          { class: "test-result unavailable", role: "status" },
          icon("circle-alert", 17),
          h(
            "div",
            null,
            h("strong", null, "No SSH identity file is configured"),
            h(
              "p",
              null,
              "Edit this profile under Profiles and add the private key path used for authentication.",
            ),
          ),
        )
        : null,
      state.report
        ? h(
          "div",
          { class: `test-result ${state.report.status}` },
          statusGlyph(state.report.status),
          h(
            "div",
            null,
            h("strong", null, state.report.message),
            h("p", null, "The test was initiated manually and made one SSH connection."),
          ),
        )
        : null,
    );
  }

  function signingSection(profile) {
    const cell = (term, value) => h("div", null, h("dt", null, term), h("dd", null, value));
    return h(
      "div",
      { class: "settings-section" },
      h(
        "div",
        { class: "section-head" },
        h(
          "div",
          null,
          h("h2", null, "Commit signing"),
          h("p", null, "Expected settings applied when this profile is bound."),
        ),
        badge(
          profile.require_signing ? "bound" : "warning",
          profile.require_signing ? "Required" : "Optional",
        ),
      ),
      h(
        "dl",
        { class: "identity-grid" },
        cell("Format", (profile.signing_format ?? "openpgp").toUpperCase()),
        cell("Signing key", profile.signing_key ?? "Not configured"),
      ),
    );
  }

  view.start({ selected: ctx.app.profiles[0]?.name ?? "", report: null });
  return host;
}
