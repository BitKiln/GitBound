// Small pieces shared by more than one view. Each is a plain function
// returning an element — the direct translation of the components of the same
// name in the React original.
import { h } from "./dom.js";
import { icon, spinner } from "./icons.js";
import { statusIcon, statusLabel } from "./status.js";

/** Title, supporting sentence, and an optional row of actions. */
export function pageHeader(title, description, actions) {
  return h(
    "header",
    { class: "page-header" },
    h("div", null, h("h1", null, title), h("p", null, description)),
    actions && actions.length ? h("div", { class: "page-actions" }, actions) : null,
  );
}

/** Shown while the first configuration read is in flight. */
export function loading() {
  return h(
    "div",
    { class: "loading" },
    spinner(24),
    h("span", null, "Reading local configuration…"),
  );
}

/**
 * The fail-closed state. GitBound stops before offering any write action
 * when it cannot read its own configuration, so this offers only retry and
 * diagnostics — diagnostics stays reachable precisely because it is what
 * explains the failure.
 */
export function loadFailure(onRetry, onDiagnostics) {
  return h(
    "section",
    { class: "load-failure" },
    icon("circle-x", 24),
    h("h1", null, "Configuration could not be read"),
    h(
      "p",
      null,
      "GitBound has stopped before offering any write actions. Fix the reported error, then retry.",
    ),
    h(
      "div",
      { class: "button-row" },
      h(
        "button",
        { class: "primary", onClick: () => void onRetry() },
        icon("refresh-cw", 16),
        "Retry",
      ),
      h(
        "button",
        { class: "secondary", onClick: onDiagnostics },
        icon("monitor-cog", 16),
        "Open diagnostics",
      ),
    ),
  );
}

/** The check/alert/cross glyph for a backend status string. */
export function statusGlyph(status) {
  return icon(statusIcon(status), 15);
}

/** A status pill. Defaults to the status itself as its text. */
export function badge(status, text) {
  return h("span", { class: `badge ${status}` }, statusGlyph(status), text ?? statusLabel(status));
}

/**
 * Two letters for an avatar tile. Falls back to a dash rather than an empty
 * square, so a profile with a blank author name still reads as a profile.
 */
export function initials(name) {
  const parts = String(name ?? "").trim().split(/\s+/).filter(Boolean);
  if (parts.length === 0) return "–";
  if (parts.length === 1) return parts[0].slice(0, 2).toUpperCase();
  return (parts[0][0] + parts[parts.length - 1][0]).toUpperCase();
}

/** A relative time like "2 min ago" from an ISO timestamp. */
export function relativeTime(iso) {
  const then = Date.parse(iso ?? "");
  if (Number.isNaN(then)) return "—";
  const seconds = Math.round((Date.now() - then) / 1000);
  if (seconds < 60) return "just now";
  const minutes = Math.round(seconds / 60);
  if (minutes < 60) return `${minutes} min ago`;
  const hours = Math.round(minutes / 60);
  if (hours < 24) return `${hours} h ago`;
  const days = Math.round(hours / 24);
  if (days < 30) return `${days} d ago`;
  return new Date(then).toLocaleDateString();
}

/**
 * A Windows path as a person would write it.
 *
 * Approved folders are stored canonicalised, and on Windows `canonicalize`
 * returns the extended-length form — `\?\C:\src` rather than `C:\src`. That
 * prefix is meaningful to the API that consumes the path and meaningless to the
 * person reading it, so it is stripped for display only: the stored value is
 * what still goes back to the backend when a folder is revoked.
 */
export function displayPath(path) {
  if (typeof path !== "string") return "";
  if (path.startsWith("\\\\?\\UNC\\")) return "\\\\" + path.slice(8);
  if (path.startsWith("\\\\?\\")) return path.slice(4);
  return path;
}
