// End-to-end coverage for the CI surface: `verify`, `audit`, the report
// formatters, and the committed repository policy.
//
// These drive the real binary against real Git repositories, like tests/cli.rs,
// because the thing under test is the contract a pipeline depends on — the exit
// code, the file that lands on disk, the annotation on stdout — and none of that
// is observable from a unit test.

use assert_cmd::cargo::cargo_bin_cmd;
use predicates::prelude::*;
use std::{fs, path::Path, process::Command};

fn initialized_repo(parent: &Path) -> std::path::PathBuf {
    let repo = parent.join("repo");
    fs::create_dir(&repo).unwrap();
    assert!(
        Command::new("git")
            .args(["init", "--quiet"])
            .current_dir(&repo)
            .status()
            .unwrap()
            .success()
    );
    repo
}

fn add_profile(repo: &Path, config: &Path) {
    cargo_bin_cmd!()
        .current_dir(repo)
        .env("GITBOUND_CONFIG", config)
        .args([
            "profile",
            "add",
            "work",
            "--github-user",
            "alice-work",
            "--git-name",
            "Alice Work",
            "--git-email",
            "work@example.com",
        ])
        .assert()
        .success();
}

/// Commit with an explicit repository-local identity, so the test never depends
/// on — or disturbs — the developer's global Git configuration.
fn commit_as(repo: &Path, name: &str, email: &str, subject: &str) {
    for (key, value) in [("user.name", name), ("user.email", email)] {
        assert!(
            Command::new("git")
                .args(["config", "--local", key, value])
                .current_dir(repo)
                .status()
                .unwrap()
                .success()
        );
    }
    assert!(
        Command::new("git")
            .args([
                "commit",
                "--allow-empty",
                "--no-gpg-sign",
                "-q",
                "-m",
                subject
            ])
            .current_dir(repo)
            .status()
            .unwrap()
            .success()
    );
}

fn write_policy(repo: &Path, body: &str) {
    fs::write(repo.join(".gitbound.toml"), body).unwrap();
}

const COMPANY_ONLY: &str =
    "schema_version = 1\n[identity]\nallowed_email_domains = [\"company.example\"]\n";

#[test]
fn help_lists_the_ci_commands() {
    cargo_bin_cmd!()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("verify"))
        .stdout(predicate::str::contains("audit"));
}

#[test]
fn verify_renders_sarif_to_a_file_and_to_stdout() {
    let temp = tempfile::tempdir().unwrap();
    let config = temp.path().join("config.toml");
    let repo = initialized_repo(temp.path());
    add_profile(&repo, &config);
    let sarif = temp.path().join("out.sarif");

    // The repository is unbound, which `verify` reports as `unverified` rather
    // than treating as a fault: a pipeline checkout never has a binding. It is
    // still a SARIF result, so there is something to upload, but on its own it
    // does not fail the run.
    cargo_bin_cmd!()
        .current_dir(&repo)
        .env("GITBOUND_CONFIG", &config)
        .args(["verify", "--format", "sarif", "--output"])
        .arg(&sarif)
        .assert()
        .code(0);

    let text = fs::read_to_string(&sarif).unwrap();
    let value: serde_json::Value = serde_json::from_str(&text).expect("SARIF must be valid JSON");
    assert_eq!(value["version"], "2.1.0");
    assert!(
        value["runs"][0]["results"]
            .as_array()
            .expect("results")
            .iter()
            .any(|result| result["ruleId"] == "binding")
    );
}

#[test]
fn verify_emits_workflow_commands_and_a_step_summary_in_a_runner() {
    let temp = tempfile::tempdir().unwrap();
    let config = temp.path().join("config.toml");
    let repo = initialized_repo(temp.path());
    add_profile(&repo, &config);
    let summary = temp.path().join("summary.md");

    cargo_bin_cmd!()
        .current_dir(&repo)
        .env("GITBOUND_CONFIG", &config)
        .env("GITHUB_ACTIONS", "true")
        .env("GITHUB_STEP_SUMMARY", &summary)
        .arg("verify")
        .assert()
        .code(0)
        .stdout(predicate::str::contains(
            "::notice title=GitBound%3A binding::",
        ));

    let written = fs::read_to_string(&summary).unwrap();
    assert!(written.contains("| Check | Status |"));
    assert!(written.contains("`binding`"));
}

/// A pipeline checkout, with a remote and a committed policy but no binding.
fn pipeline_checkout(temp: &Path, email: &str) -> std::path::PathBuf {
    let repo = initialized_repo(temp);
    assert!(
        Command::new("git")
            .args([
                "remote",
                "add",
                "origin",
                "https://github.com/company-name/project.git",
            ])
            .current_dir(&repo)
            .status()
            .unwrap()
            .success()
    );
    for (key, value) in [("user.name", "Alice"), ("user.email", email)] {
        assert!(
            Command::new("git")
                .args(["config", "--local", key, value])
                .current_dir(&repo)
                .status()
                .unwrap()
                .success()
        );
    }
    repo
}

#[test]
fn verify_applies_repository_policy_to_an_unbound_checkout() {
    // The whole point of CI mode. `inspect` used to return as soon as it found
    // no binding, so no `git_email` item ever reached `Policy::evaluate`, which
    // then reported "no Git author email is configured" for a repository that
    // had one and was breaking the rule. The verdict was still `failure` — but
    // only because the missing binding failed it, so the policy was decorative.
    let temp = tempfile::tempdir().unwrap();
    let config = temp.path().join("config.toml");
    let repo = pipeline_checkout(temp.path(), "alice@personal.example");
    write_policy(&repo, COMPANY_ONLY);

    cargo_bin_cmd!()
        .current_dir(&repo)
        .env("GITBOUND_CONFIG", &config)
        .args(["verify", "--format", "json"])
        .assert()
        .code(1)
        .stdout(predicate::str::contains(
            "alice@personal.example is not permitted by .gitbound.toml",
        ))
        .stdout(predicate::str::contains("no Git author email is configured").not());
}

#[test]
fn verify_passes_a_compliant_pipeline_checkout() {
    // And the other direction, which is what makes the gate usable: a checkout
    // that satisfies the committed policy exits 0 even though it has no
    // binding, because a pipeline never has one.
    let temp = tempfile::tempdir().unwrap();
    let config = temp.path().join("config.toml");
    let repo = pipeline_checkout(temp.path(), "alice@company.example");
    write_policy(&repo, COMPANY_ONLY);

    cargo_bin_cmd!()
        .current_dir(&repo)
        .env("GITBOUND_CONFIG", &config)
        .args(["verify", "--format", "human"])
        .assert()
        .code(0)
        .stdout(predicate::str::contains(
            "author email is permitted by repository policy",
        ));
}

#[test]
fn check_still_fails_an_unbound_repository() {
    // `verify` excusing an absent binding must not have relaxed the gate that
    // runs on the machine which owns the binding.
    let temp = tempfile::tempdir().unwrap();
    let config = temp.path().join("config.toml");
    let repo = pipeline_checkout(temp.path(), "alice@company.example");
    write_policy(&repo, COMPANY_ONLY);

    cargo_bin_cmd!()
        .current_dir(&repo)
        .env("GITBOUND_CONFIG", &config)
        .args(["check", "--format", "human"])
        .assert()
        .code(1)
        .stdout(predicate::str::contains(
            "repository is not bound to a GitBound profile",
        ));
}

#[test]
fn require_policy_refuses_a_repository_that_declares_none() {
    // Without a policy there is no rule to break, so `verify` would otherwise
    // pass having checked nothing — and deleting the file would be enough to
    // switch the gate off.
    let temp = tempfile::tempdir().unwrap();
    let config = temp.path().join("config.toml");
    let repo = pipeline_checkout(temp.path(), "alice@company.example");

    cargo_bin_cmd!()
        .current_dir(&repo)
        .env("GITBOUND_CONFIG", &config)
        .args(["verify"])
        .assert()
        .code(0);

    cargo_bin_cmd!()
        .current_dir(&repo)
        .env("GITBOUND_CONFIG", &config)
        .args(["verify", "--require-policy"])
        .assert()
        .code(1)
        .stdout(predicate::str::contains("commits no .gitbound.toml"));

    // A file that declares no rules constrains nothing, so it counts as absent.
    write_policy(
        &repo,
        "schema_version = 1
",
    );
    cargo_bin_cmd!()
        .current_dir(&repo)
        .env("GITBOUND_CONFIG", &config)
        .args(["verify", "--require-policy"])
        .assert()
        .code(1)
        .stdout(predicate::str::contains("declares no rules"));

    write_policy(&repo, COMPANY_ONLY);
    cargo_bin_cmd!()
        .current_dir(&repo)
        .env("GITBOUND_CONFIG", &config)
        .args(["verify", "--require-policy"])
        .assert()
        .code(0);
}

#[test]
fn an_explicit_format_wins_over_the_runner_environment() {
    let temp = tempfile::tempdir().unwrap();
    let config = temp.path().join("config.toml");
    let repo = initialized_repo(temp.path());
    add_profile(&repo, &config);

    cargo_bin_cmd!()
        .current_dir(&repo)
        .env("GITBOUND_CONFIG", &config)
        .env("GITHUB_ACTIONS", "true")
        .args(["status", "--json"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"overall\""))
        .stdout(predicate::str::contains("::error").not());
}

#[test]
fn a_repository_policy_can_reject_an_otherwise_valid_identity() {
    let temp = tempfile::tempdir().unwrap();
    let config = temp.path().join("config.toml");
    let repo = initialized_repo(temp.path());
    add_profile(&repo, &config);
    cargo_bin_cmd!()
        .current_dir(&repo)
        .env("GITBOUND_CONFIG", &config)
        .args(["bind", "work"])
        .assert()
        .success();

    // The profile's own address is work@example.com, which this policy forbids.
    write_policy(&repo, COMPANY_ONLY);

    cargo_bin_cmd!()
        .current_dir(&repo)
        .env("GITBOUND_CONFIG", &config)
        .args(["verify", "--json"])
        .assert()
        .code(1)
        .stdout(predicate::str::contains("policy_email"));

    // ...and --no-policy must put the behaviour back exactly as it was.
    cargo_bin_cmd!()
        .current_dir(&repo)
        .env("GITBOUND_CONFIG", &config)
        .args(["verify", "--json", "--no-policy"])
        .assert()
        .stdout(predicate::str::contains("policy_email").not());
}

#[test]
fn an_absent_policy_changes_nothing() {
    let temp = tempfile::tempdir().unwrap();
    let config = temp.path().join("config.toml");
    let repo = initialized_repo(temp.path());
    add_profile(&repo, &config);

    cargo_bin_cmd!()
        .current_dir(&repo)
        .env("GITBOUND_CONFIG", &config)
        .args(["status", "--json"])
        .assert()
        .success()
        .stdout(predicate::str::contains("policy_").not());
}

#[test]
fn an_unreadable_policy_schema_is_a_usage_error() {
    let temp = tempfile::tempdir().unwrap();
    let config = temp.path().join("config.toml");
    let repo = initialized_repo(temp.path());
    add_profile(&repo, &config);
    write_policy(&repo, "schema_version = 99\n");

    cargo_bin_cmd!()
        .current_dir(&repo)
        .env("GITBOUND_CONFIG", &config)
        .arg("verify")
        .assert()
        .code(2)
        .stderr(predicate::str::contains("schema_version 99"));
}

#[test]
fn audit_names_the_commit_that_breaks_policy() {
    let temp = tempfile::tempdir().unwrap();
    let config = temp.path().join("config.toml");
    let repo = initialized_repo(temp.path());
    add_profile(&repo, &config);

    commit_as(&repo, "Alice", "alice@company.example", "first");
    commit_as(&repo, "Bob", "bob@personal.example", "second");
    commit_as(&repo, "Alice", "alice@company.example", "third");
    write_policy(&repo, COMPANY_ONLY);

    cargo_bin_cmd!()
        .current_dir(&repo)
        .env("GITBOUND_CONFIG", &config)
        .args(["audit", "--range", "HEAD", "--json"])
        .assert()
        .code(1)
        .stdout(predicate::str::contains("commit_author"))
        .stdout(predicate::str::contains("bob@personal.example"));
}

#[test]
fn audit_passes_a_clean_range() {
    let temp = tempfile::tempdir().unwrap();
    let config = temp.path().join("config.toml");
    let repo = initialized_repo(temp.path());
    add_profile(&repo, &config);
    commit_as(&repo, "Alice", "alice@company.example", "first");
    write_policy(&repo, COMPANY_ONLY);

    cargo_bin_cmd!()
        .current_dir(&repo)
        .env("GITBOUND_CONFIG", &config)
        .args(["audit", "--range", "HEAD", "--json"])
        .assert()
        .success()
        .stdout(predicate::str::contains("commit_range"));
}

#[test]
fn audit_enforce_signing_fails_unsigned_commits() {
    let temp = tempfile::tempdir().unwrap();
    let config = temp.path().join("config.toml");
    let repo = initialized_repo(temp.path());
    add_profile(&repo, &config);
    commit_as(&repo, "Alice", "alice@company.example", "unsigned");

    cargo_bin_cmd!()
        .current_dir(&repo)
        .env("GITBOUND_CONFIG", &config)
        .args(["audit", "--range", "HEAD", "--enforce-signing", "--json"])
        .assert()
        .code(1)
        .stdout(predicate::str::contains("commit_signature"));
}

#[test]
fn audit_refuses_an_option_shaped_range() {
    let temp = tempfile::tempdir().unwrap();
    let config = temp.path().join("config.toml");
    let repo = initialized_repo(temp.path());
    add_profile(&repo, &config);
    commit_as(&repo, "Alice", "alice@company.example", "first");

    cargo_bin_cmd!()
        .current_dir(&repo)
        .env("GITBOUND_CONFIG", &config)
        .args(["audit", "--range", "--output=/tmp/pwned"])
        .assert()
        .code(2);
}

#[test]
fn verify_junit_is_well_formed_xml() {
    let temp = tempfile::tempdir().unwrap();
    let config = temp.path().join("config.toml");
    let repo = initialized_repo(temp.path());
    add_profile(&repo, &config);

    let output = cargo_bin_cmd!()
        .current_dir(&repo)
        .env("GITBOUND_CONFIG", &config)
        .args(["verify", "--format", "junit"])
        .output()
        .unwrap();
    let xml = String::from_utf8(output.stdout).unwrap();
    assert!(xml.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>"));
    assert_eq!(
        xml.matches("<testcase").count(),
        xml.matches("</testcase>").count(),
        "every testcase must be closed"
    );
    assert!(xml.trim_end().ends_with("</testsuites>"));
}
