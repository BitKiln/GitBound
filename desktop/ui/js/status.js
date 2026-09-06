// Maps every status string that can cross the IPC boundary onto one of three
// severities. Five separate Rust enums land here — RepositoryLocalStatus,
// CheckStatus, DependencyState, SshTestStatus, and CiConclusion — because the
// UI only ever needs to know "good, needs attention, or broken".
//
// Keeping this in JS avoids a DTO change that would ripple into tests/cli.rs
// JSON assertions. The risk that creates — a new Rust variant the UI silently
// mishandles — is covered by the
// `every_backend_status_appears_in_the_ui_severity_table` test, which checks
// this file for each variant name. Add a variant in Rust, add it here.
const SEVERITY = new Map([
  // RepositoryLocalStatus::Bound, CheckStatus::Ok, SshTestStatus::Verified
  ["bound", "good"],
  ["pass", "good"],
  ["ok", "good"],
  ["verified", "good"],
  // Not wrong, but not confirmed either.
  ["unbound", "warn"],
  ["warning", "warn"],
  ["unverified", "warn"],
  // Actively wrong, unavailable, or refused.
  ["drifted", "bad"],
  ["missing_profile", "bad"],
  ["unavailable", "bad"],
  ["failure", "bad"],
  ["rejected", "bad"],
  // CiConclusion. A red pipeline is somebody else's problem to fix, not an
  // identity fault, so nothing here is "bad" except an outright failed run.
  ["success", "good"],
  ["cancelled", "warn"],
  ["skipped", "warn"],
  ["running", "warn"],
  ["unknown", "warn"],
]);

/** "good" | "warn" | "bad" — the severity of a backend status string. */
export function severity(status) {
  // An unrecognised status reads as needing attention rather than as fine, so
  // a variant added in Rust and forgotten here fails safe.
  return SEVERITY.get(status) ?? "bad";
}

/** The sprite id of the glyph for a status. */
export function statusIcon(status) {
  const level = severity(status);
  return level === "good" ? "check" : level === "warn" ? "circle-alert" : "circle-x";
}

/** Default badge text: the raw status with underscores read as spaces. */
export function statusLabel(status) {
  return String(status).replace(/_/g, " ");
}
