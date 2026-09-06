// The explicit theme choice.
//
// Colour scheme followed the operating system exclusively until v3. That is
// still the default, and still the right default — but "follow the system" and
// "I want this window dark" are different requests, and only one of them was
// answerable.
//
// The choice lives in localStorage rather than in config.toml. config.toml is
// the safety-relevant file: it holds identities, allowed owners, and signing
// policy, it is locked and atomically rewritten on every change, and it is
// shared with the CLI. A colour preference has no business in it, and a
// preference that failed to save because another process held the lock would be
// a genuinely absurd failure mode.

const KEY = "gitbound.theme";
const MODES = new Set(["system", "light", "dark"]);
const ACCENTS = new Set(["moss", "violet"]);

const FALLBACK = { mode: "system", accent: "moss" };

/**
 * Read the stored preference. Every access is guarded: a private window, cleared
 * site data, or a webview configured to block storage all throw here, and none
 * of them is a reason to fail to start.
 */
export function readTheme() {
  try {
    const raw = window.localStorage.getItem(KEY);
    if (!raw) return { ...FALLBACK };
    const parsed = JSON.parse(raw);
    return {
      mode: MODES.has(parsed.mode) ? parsed.mode : FALLBACK.mode,
      accent: ACCENTS.has(parsed.accent) ? parsed.accent : FALLBACK.accent,
    };
  } catch {
    return { ...FALLBACK };
  }
}

/**
 * Apply a preference to the document and persist it. Returns the preference
 * actually applied, so a caller storing it in shell state cannot drift from
 * what is on screen.
 *
 * `system` removes the attribute entirely rather than setting it to "system",
 * because the stylesheet's dark block is written as
 * `:root:not([data-theme="light"])` under `prefers-color-scheme` — absence is
 * what lets the OS decide.
 */
export function applyTheme(mode, accent) {
  const applied = {
    mode: MODES.has(mode) ? mode : FALLBACK.mode,
    accent: ACCENTS.has(accent) ? accent : FALLBACK.accent,
  };
  const root = document.documentElement;
  if (applied.mode === "system") root.removeAttribute("data-theme");
  else root.setAttribute("data-theme", applied.mode);
  if (applied.accent === "moss") root.removeAttribute("data-accent");
  else root.setAttribute("data-accent", applied.accent);
  try {
    window.localStorage.setItem(KEY, JSON.stringify(applied));
  } catch {
    // The theme is applied either way; it just will not survive a restart.
  }
  return applied;
}
