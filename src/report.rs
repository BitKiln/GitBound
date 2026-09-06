// Rendering an existing CheckReport for whatever is going to read it.
//
// This module adds no checks and makes no decisions. `src/check.rs` produces a
// `CheckReport`; everything here is presentation. That separation is what keeps
// the CI surface from drifting away from what the desktop app and the local CLI
// report — there is exactly one engine, and these are its output shapes.
//
// The exit-code contract is unaffected: `CheckReport::enforceable()` still
// decides success or failure, whatever format was asked for.

use crate::check::{CheckItem, CheckReport, CheckStatus, OverallStatus};
use serde_json::{Map, Value, json};
use std::{
    fmt::Write as _,
    fs::OpenOptions,
    io::Write as _,
    path::{Path, PathBuf},
};

const SARIF_SCHEMA: &str = "https://json.schemastore.org/sarif-2.1.0.json";
const TOOL_URI: &str = "https://github.com/BitKiln/GitBound";

/// How a report should be rendered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
#[clap(rename_all = "kebab-case")]
pub enum ReportFormat {
    /// Pick `github` inside a GitHub Actions runner, `human` everywhere else.
    Auto,
    Human,
    Json,
    Sarif,
    Junit,
    Github,
    Markdown,
}

impl ReportFormat {
    /// Resolve `Auto` against the environment. Every other format is itself.
    pub fn resolve(self) -> Self {
        match self {
            Self::Auto if in_github_actions() => Self::Github,
            Self::Auto => Self::Human,
            other => other,
        }
    }
}

impl std::str::FromStr for ReportFormat {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.to_ascii_lowercase().as_str() {
            "auto" => Ok(Self::Auto),
            "human" => Ok(Self::Human),
            "json" => Ok(Self::Json),
            "sarif" => Ok(Self::Sarif),
            "junit" => Ok(Self::Junit),
            "github" => Ok(Self::Github),
            "markdown" => Ok(Self::Markdown),
            other => Err(format!("unknown report format '{other}'")),
        }
    }
}

/// One `--output` entry: an optional format prefix and a path.
///
/// The prefix is recognised only when it is a known format name, which is what
/// keeps `--output C:\reports\out.sarif` on Windows from being read as a format
/// called `c`.
pub fn parse_output(value: &str) -> (Option<ReportFormat>, PathBuf) {
    if let Some((prefix, rest)) = value.split_once(':')
        && let Ok(format) = prefix.parse::<ReportFormat>()
        && !rest.is_empty()
    {
        return (Some(format), PathBuf::from(rest));
    }
    (None, PathBuf::from(value))
}

/// True when running inside a GitHub Actions job.
pub fn in_github_actions() -> bool {
    std::env::var("GITHUB_ACTIONS").is_ok_and(|value| value == "true")
}

/// Where SARIF results should be anchored. Identity checks are properties of a
/// repository rather than of a line of code, so there is no natural location;
/// the committed policy file is used when one exists and the repository's own
/// config otherwise.
fn anchor(repository: &Path) -> String {
    if repository.join(crate::policy::POLICY_FILE).is_file() {
        crate::policy::POLICY_FILE.to_string()
    } else {
        ".git/config".to_string()
    }
}

/// Render a report. `Auto` resolves against the environment first.
pub fn render(report: &CheckReport, format: ReportFormat) -> String {
    match format.resolve() {
        ReportFormat::Human | ReportFormat::Auto => report.render_human(),
        ReportFormat::Json => {
            serde_json::to_string_pretty(report).unwrap_or_else(|error| error.to_string())
        }
        ReportFormat::Sarif => render_sarif(report),
        ReportFormat::Junit => render_junit(report),
        ReportFormat::Github => render_github(report),
        ReportFormat::Markdown => render_markdown(report),
    }
}

// ---------------------------------------------------------------- SARIF 2.1.0

fn sarif_level(status: CheckStatus) -> &'static str {
    match status {
        CheckStatus::Failure => "error",
        CheckStatus::Warning => "warning",
        CheckStatus::Unverified => "note",
        CheckStatus::Ok => "none",
    }
}

fn rule(id: &str) -> Value {
    json!({
        "id": id,
        "name": id,
        "shortDescription": { "text": describe_rule(id) },
        "defaultConfiguration": { "level": "error" },
        "properties": { "tags": ["identity", "gitbound"] },
    })
}

/// A one-line explanation per check id, so a SARIF consumer showing only the
/// rule has something meaningful to display.
fn describe_rule(id: &str) -> &'static str {
    match id {
        "binding" => "The repository is bound to an explicit GitBound profile",
        "profile" => "The bound profile exists in the local configuration",
        "git_name" => "The configured Git author name matches the bound profile",
        "git_email" => "The configured Git author email matches the bound profile",
        "signing_key" => "The configured signing key matches the bound profile",
        "signing_format" => "The configured signing format matches the bound profile",
        "commit_signing" => "Commit signing is enabled where the profile requires it",
        "remote" => "The repository has the expected remote",
        "hostname" => "The remote host matches the bound profile",
        "transport" => "The remote uses an encrypted transport",
        "owner" => "The remote owner is allowed by the bound profile",
        "ssh_command" => "The repository-local SSH command uses the profile key",
        "github_cli" => "The active GitHub CLI account matches the bound profile",
        "ssh_identity" => "SSH authenticates as the profile's GitHub user",
        "credential_helper" => "A compatible Git credential helper is configured",
        "commit_author" => "Every commit in the range is authored within policy",
        "commit_signature" => "Every commit in the range carries a required signature",
        _ => "GitBound identity check",
    }
}

fn render_sarif(report: &CheckReport) -> String {
    let repository = PathBuf::from(&report.repository);
    let uri = anchor(&repository);

    let mut rule_ids: Vec<&str> = Vec::new();
    let mut results = Vec::new();
    for check in &report.checks {
        if check.status == CheckStatus::Ok {
            continue;
        }
        if !rule_ids.contains(&check.id.as_str()) {
            rule_ids.push(&check.id);
        }
        let mut properties = Map::new();
        if let Some(expected) = &check.expected {
            properties.insert("expected".into(), Value::String(expected.clone()));
        }
        if let Some(actual) = &check.actual {
            properties.insert("actual".into(), Value::String(actual.clone()));
        }
        results.push(json!({
            "ruleId": check.id,
            "level": sarif_level(check.status),
            "message": { "text": detail(check) },
            "properties": Value::Object(properties),
            "locations": [{
                "physicalLocation": {
                    "artifactLocation": { "uri": uri },
                    "region": { "startLine": 1 },
                }
            }],
        }));
    }

    let rules: Vec<Value> = rule_ids.iter().map(|id| rule(id)).collect();
    let document = json!({
        "$schema": SARIF_SCHEMA,
        "version": "2.1.0",
        "runs": [{
            "tool": { "driver": {
                "name": "GitBound",
                "version": env!("CARGO_PKG_VERSION"),
                "informationUri": TOOL_URI,
                "rules": rules,
            }},
            "results": results,
        }],
    });
    serde_json::to_string_pretty(&document).unwrap_or_else(|error| error.to_string())
}

// ----------------------------------------------------------------- JUnit XML

/// JUnit has no "warning", so a warning is a passing case carrying its detail on
/// stdout, and an unverified check is skipped rather than silently green.
fn render_junit(report: &CheckReport) -> String {
    let failures = report
        .checks
        .iter()
        .filter(|check| check.status == CheckStatus::Failure)
        .count();
    let skipped = report
        .checks
        .iter()
        .filter(|check| check.status == CheckStatus::Unverified)
        .count();

    let mut output = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    let _ = writeln!(
        output,
        "<testsuites name=\"gitbound\" tests=\"{}\" failures=\"{failures}\" skipped=\"{skipped}\">",
        report.checks.len()
    );
    let _ = writeln!(
        output,
        "  <testsuite name=\"identity\" tests=\"{}\" failures=\"{failures}\" skipped=\"{skipped}\" hostname=\"{}\">",
        report.checks.len(),
        xml(&report.repository)
    );
    for check in &report.checks {
        let _ = writeln!(
            output,
            "    <testcase classname=\"gitbound.identity\" name=\"{}\">",
            xml(&check.id)
        );
        match check.status {
            CheckStatus::Failure => {
                let _ = writeln!(
                    output,
                    "      <failure message=\"{}\">{}</failure>",
                    xml(&check.message),
                    xml(&detail(check))
                );
            }
            CheckStatus::Unverified => {
                let _ = writeln!(
                    output,
                    "      <skipped message=\"{}\"/>",
                    xml(&check.message)
                );
            }
            CheckStatus::Warning => {
                let _ = writeln!(
                    output,
                    "      <system-out>{}</system-out>",
                    xml(&detail(check))
                );
            }
            CheckStatus::Ok => {}
        }
        let _ = writeln!(output, "    </testcase>");
    }
    let _ = writeln!(output, "  </testsuite>");
    let _ = writeln!(output, "</testsuites>");
    output
}

fn xml(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&apos;"),
            // XML 1.0 forbids most control characters outright; dropping them is
            // preferable to emitting a document no parser will accept.
            c if (c as u32) < 0x20 && c != '\t' && c != '\n' && c != '\r' => {}
            c => escaped.push(c),
        }
    }
    escaped
}

// -------------------------------------------------- GitHub workflow commands

fn render_github(report: &CheckReport) -> String {
    let mut output = String::new();
    for check in &report.checks {
        let command = match check.status {
            CheckStatus::Failure => "error",
            CheckStatus::Warning => "warning",
            CheckStatus::Unverified => "notice",
            CheckStatus::Ok => continue,
        };
        let _ = writeln!(
            output,
            "::{command} title={}::{}",
            property(&format!("GitBound: {}", check.id)),
            data(&detail(check))
        );
    }
    output.push_str(&report.render_human());
    output
}

/// GitHub workflow-command escaping for the message body.
fn data(value: &str) -> String {
    value
        .replace('%', "%25")
        .replace('\r', "%0D")
        .replace('\n', "%0A")
}

/// Property values additionally escape the separators of the command itself.
fn property(value: &str) -> String {
    data(value).replace(':', "%3A").replace(',', "%2C")
}

/// Append the markdown report to `$GITHUB_STEP_SUMMARY` when a runner provided
/// one. A missing or unwritable summary file is not an error: the report has
/// already been printed, and failing the step over a cosmetic write would be
/// worse than losing the summary.
pub fn write_step_summary(report: &CheckReport) -> bool {
    let Ok(path) = std::env::var("GITHUB_STEP_SUMMARY") else {
        return false;
    };
    let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) else {
        return false;
    };
    file.write_all(render_markdown(report).as_bytes()).is_ok()
}

// ------------------------------------------------------------------ Markdown

fn render_markdown(report: &CheckReport) -> String {
    let mut output = format!("### GitBound — {}\n\n", verdict(report.overall));
    let _ = writeln!(output, "- **Repository:** `{}`", report.repository);
    let _ = writeln!(
        output,
        "- **Profile:** {}",
        report
            .profile
            .as_deref()
            .map(|name| format!("`{name}`"))
            .unwrap_or_else(|| "_none_".into())
    );
    if let Some(remote) = &report.remote {
        let _ = writeln!(
            output,
            "- **Remote:** `{}` ({}/{})",
            remote.url, remote.owner, remote.repository
        );
    }
    output.push_str("\n| Check | Status | Expected | Actual | Detail |\n");
    output.push_str("| --- | --- | --- | --- | --- |\n");
    for check in &report.checks {
        let _ = writeln!(
            output,
            "| `{}` | {} | {} | {} | {} |",
            check.id,
            marker(check.status),
            cell(check.expected.as_deref()),
            cell(check.actual.as_deref()),
            md(&check.message)
        );
    }
    output.push('\n');
    output
}

fn verdict(overall: OverallStatus) -> &'static str {
    match overall {
        OverallStatus::Ok => "all checks passed",
        OverallStatus::Warning => "attention needed",
        OverallStatus::Failure => "identity check failed",
    }
}

fn marker(status: CheckStatus) -> &'static str {
    match status {
        CheckStatus::Ok => "ok",
        CheckStatus::Warning => "warning",
        CheckStatus::Failure => "failure",
        CheckStatus::Unverified => "unverified",
    }
}

fn cell(value: Option<&str>) -> String {
    match value {
        Some(value) => format!("`{}`", md(value)),
        None => "—".into(),
    }
}

/// Neutralize the characters that would break out of a markdown table cell.
fn md(value: &str) -> String {
    value.replace('|', "\\|").replace(['\r', '\n'], " ")
}

// ------------------------------------------------------------------- shared

/// One line combining a check's message with its expected/actual pair, used by
/// every machine-readable format so they all say the same thing.
fn detail(check: &CheckItem) -> String {
    match (&check.expected, &check.actual) {
        (Some(expected), Some(actual)) => {
            format!("{} (expected {expected}, found {actual})", check.message)
        }
        (Some(expected), None) => format!("{} (expected {expected})", check.message),
        (None, Some(actual)) => format!("{} (found {actual})", check.message),
        (None, None) => check.message.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::check::{CheckItem, CheckReport, CheckStatus, OverallStatus};

    fn report() -> CheckReport {
        CheckReport {
            repository: "/tmp/project".into(),
            profile: Some("work".into()),
            remote: None,
            overall: OverallStatus::Failure,
            checks: vec![
                CheckItem {
                    id: "git_email".into(),
                    status: CheckStatus::Failure,
                    expected: Some("alice@company.example".into()),
                    actual: Some("alice@personal.example".into()),
                    message: "Git email does not match the bound profile".into(),
                },
                CheckItem {
                    id: "github_cli".into(),
                    status: CheckStatus::Unverified,
                    expected: None,
                    actual: None,
                    message: "GitHub CLI is unavailable".into(),
                },
                CheckItem {
                    id: "git_name".into(),
                    status: CheckStatus::Ok,
                    expected: Some("Alice".into()),
                    actual: Some("Alice".into()),
                    message: "Git author matches".into(),
                },
            ],
        }
    }

    #[test]
    fn sarif_is_valid_json_with_a_result_per_non_ok_check() {
        let value: Value = serde_json::from_str(&render_sarif(&report())).expect("valid JSON");
        assert_eq!(value["version"], "2.1.0");
        assert_eq!(value["$schema"], SARIF_SCHEMA);
        let results = value["runs"][0]["results"].as_array().expect("results");
        assert_eq!(
            results.len(),
            2,
            "the passing check must not produce a result"
        );
        assert_eq!(results[0]["level"], "error");
        assert_eq!(results[1]["level"], "note");
        let rules = value["runs"][0]["tool"]["driver"]["rules"]
            .as_array()
            .expect("rules");
        assert_eq!(rules.len(), 2);
    }

    #[test]
    fn junit_counts_failures_and_skips_separately() {
        let xml = render_junit(&report());
        assert!(xml.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>"));
        assert!(xml.contains("tests=\"3\" failures=\"1\" skipped=\"1\""));
        assert!(xml.contains("<failure message=\"Git email does not match the bound profile\">"));
        assert!(xml.contains("<skipped message=\"GitHub CLI is unavailable\"/>"));
    }

    #[test]
    fn github_emits_one_annotation_per_non_ok_check() {
        let output = render_github(&report());
        assert!(output.contains("::error title=GitBound%3A git_email::"));
        assert!(output.contains("::notice title=GitBound%3A github_cli::"));
        assert!(!output.contains("git_name::"));
    }

    #[test]
    fn markdown_neutralises_pipes_in_values() {
        let mut report = report();
        report.checks[0].actual = Some("a|b".into());
        let table = render_markdown(&report);
        assert!(table.contains("`a\\|b`"));
    }

    #[test]
    fn auto_resolves_away_before_rendering() {
        // Whatever the environment says, `Auto` must never reach the renderer.
        assert!(matches!(
            ReportFormat::Auto.resolve(),
            ReportFormat::Github | ReportFormat::Human
        ));
    }

    #[test]
    fn output_prefixes_are_recognised_without_eating_windows_drive_letters() {
        assert_eq!(
            parse_output("sarif:out.sarif"),
            (Some(ReportFormat::Sarif), PathBuf::from("out.sarif"))
        );
        assert_eq!(
            parse_output("json:C:\\reports\\out.json"),
            (
                Some(ReportFormat::Json),
                PathBuf::from("C:\\reports\\out.json")
            )
        );
        // A drive letter is not a format name, so the whole value is the path.
        assert_eq!(
            parse_output("C:\\reports\\out.sarif"),
            (None, PathBuf::from("C:\\reports\\out.sarif"))
        );
        assert_eq!(
            parse_output("plain.txt"),
            (None, PathBuf::from("plain.txt"))
        );
    }

    #[test]
    fn xml_escaping_drops_forbidden_control_characters() {
        assert_eq!(xml("a\u{1}b"), "ab");
        assert_eq!(xml("<a & 'b'>"), "&lt;a &amp; &apos;b&apos;&gt;");
    }
}
