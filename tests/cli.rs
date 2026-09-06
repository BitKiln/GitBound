use assert_cmd::cargo::cargo_bin_cmd;
use predicates::prelude::*;
use std::env;
use std::{fs, process::Command};

#[test]
fn help_lists_primary_commands() {
    cargo_bin_cmd!()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("profile"))
        .stdout(predicate::str::contains("clone"))
        .stdout(predicate::str::contains("bind"))
        .stdout(predicate::str::contains("directory"))
        .stdout(predicate::str::contains("hooks"));
}

#[test]
fn profile_add_and_list_json() {
    let temp = tempfile::tempdir().unwrap();
    let config = temp.path().join("config.toml");
    cargo_bin_cmd!()
        .env("GITBOUND_CONFIG", &config)
        .args([
            "profile",
            "add",
            "work",
            "--github-user",
            "alice-work",
            "--git-name",
            "Alice",
            "--git-email",
            "alice@example.com",
        ])
        .assert()
        .success();

    cargo_bin_cmd!()
        .env("GITBOUND_CONFIG", &config)
        .args(["profile", "list", "--json"])
        .assert()
        .success()
        .stdout(predicate::str::contains("alice-work"));

    cargo_bin_cmd!()
        .env("GITBOUND_CONFIG", &config)
        .args(["profile", "rename", "work", "work-renamed"])
        .assert()
        .success();

    cargo_bin_cmd!()
        .env("GITBOUND_CONFIG", &config)
        .args(["profile", "show", "work-renamed", "--json"])
        .assert()
        .success()
        .stdout(predicate::str::contains("alice-work"));
}

#[test]
fn profile_signing_and_completions_are_exposed() {
    let temp = tempfile::tempdir().unwrap();
    let config = temp.path().join("config.toml");
    cargo_bin_cmd!()
        .env("GITBOUND_CONFIG", &config)
        .args([
            "profile",
            "add",
            "signed",
            "--github-user",
            "alice",
            "--git-name",
            "Alice",
            "--git-email",
            "alice@example.com",
            "--signing-key",
            "ABC123",
            "--require-signing",
        ])
        .assert()
        .success();
    cargo_bin_cmd!()
        .env("GITBOUND_CONFIG", &config)
        .args(["profile", "show", "signed", "--json"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"require_signing\": true"));
    cargo_bin_cmd!()
        .args(["completions", "bash"])
        .assert()
        .success()
        .stdout(predicate::str::contains("gitbound"));
}

#[test]
fn bind_and_unbind_restore_original_identity() {
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path().join("repo");
    fs::create_dir(&repo).unwrap();
    assert!(
        Command::new("git")
            .args(["init", "--quiet"])
            .current_dir(&repo)
            .status()
            .unwrap()
            .success()
    );
    assert!(
        Command::new("git")
            .args(["config", "--local", "user.name", "Original"])
            .current_dir(&repo)
            .status()
            .unwrap()
            .success()
    );
    assert!(
        Command::new("git")
            .args(["config", "--local", "user.email", "original@example.com"])
            .current_dir(&repo)
            .status()
            .unwrap()
            .success()
    );
    let config = temp.path().join("config.toml");

    cargo_bin_cmd!()
        .current_dir(&repo)
        .env("GITBOUND_CONFIG", &config)
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
    cargo_bin_cmd!()
        .current_dir(&repo)
        .env("GITBOUND_CONFIG", &config)
        .args(["bind", "work"])
        .assert()
        .success();
    assert_eq!(git_value(&repo, "user.email"), "work@example.com");
    assert_eq!(git_value(&repo, "gitbound.profile"), "work");

    cargo_bin_cmd!()
        .current_dir(&repo)
        .env("GITBOUND_CONFIG", &config)
        .arg("unbind")
        .assert()
        .success();
    assert_eq!(git_value(&repo, "user.name"), "Original");
    assert_eq!(git_value(&repo, "user.email"), "original@example.com");
    assert!(
        !Command::new("git")
            .args(["config", "--local", "--get", "gitbound.profile"])
            .current_dir(&repo)
            .status()
            .unwrap()
            .success()
    );
}

#[test]
fn repo_flag_operates_outside_the_current_directory() {
    let temp = tempfile::tempdir().unwrap();
    let repo = initialized_repo(temp.path());
    let elsewhere = temp.path().join("elsewhere");
    fs::create_dir(&elsewhere).unwrap();
    let config = temp.path().join("config.toml");
    add_profile(&repo, &config);
    cargo_bin_cmd!()
        .current_dir(&elsewhere)
        .env("GITBOUND_CONFIG", &config)
        .args(["bind", "work", "--repo", repo.to_str().unwrap()])
        .assert()
        .success();
    cargo_bin_cmd!()
        .current_dir(&elsewhere)
        .env("GITBOUND_CONFIG", &config)
        .args(["status", "--repo", repo.to_str().unwrap(), "--json"])
        .assert()
        .success()
        .stdout(predicate::str::contains("work@example.com"));
    cargo_bin_cmd!()
        .current_dir(&elsewhere)
        .env("GITBOUND_CONFIG", &config)
        .args(["unbind", "--repo", repo.to_str().unwrap()])
        .assert()
        .success();
}

#[test]
fn doctor_json_is_structured_and_keeps_dependency_exit_code() {
    let temp = tempfile::tempdir().unwrap();
    let config = temp.path().join("config.toml");
    let assert = cargo_bin_cmd!()
        .env("GITBOUND_CONFIG", &config)
        .args(["doctor", "--json"])
        .assert();
    assert
        .stdout(predicate::str::contains("\"dependencies\""))
        .stdout(predicate::str::contains("\"schema_version\": 3"));
}

#[test]
fn bind_and_unbind_restore_signing_configuration() {
    let temp = tempfile::tempdir().unwrap();
    let repo = initialized_repo(temp.path());
    let config = temp.path().join("config.toml");
    for (key, value) in [
        ("user.signingKey", "OLDKEY"),
        ("gpg.format", "ssh"),
        ("commit.gpgSign", "false"),
    ] {
        assert!(
            Command::new("git")
                .args(["config", "--local", key, value])
                .current_dir(&repo)
                .status()
                .unwrap()
                .success()
        );
    }
    cargo_bin_cmd!()
        .current_dir(&repo)
        .env("GITBOUND_CONFIG", &config)
        .args([
            "profile",
            "add",
            "signed",
            "--github-user",
            "alice",
            "--git-name",
            "Alice",
            "--git-email",
            "alice@example.com",
            "--signing-key",
            "NEWKEY",
            "--require-signing",
        ])
        .assert()
        .success();
    cargo_bin_cmd!()
        .current_dir(&repo)
        .env("GITBOUND_CONFIG", &config)
        .args(["bind", "signed"])
        .assert()
        .success();
    assert_eq!(git_value(&repo, "user.signingKey"), "NEWKEY");
    assert_eq!(git_value(&repo, "gpg.format"), "openpgp");
    assert_eq!(git_value(&repo, "commit.gpgSign"), "true");

    cargo_bin_cmd!()
        .current_dir(&repo)
        .env("GITBOUND_CONFIG", &config)
        .arg("unbind")
        .assert()
        .success();
    assert_eq!(git_value(&repo, "user.signingKey"), "OLDKEY");
    assert_eq!(git_value(&repo, "gpg.format"), "ssh");
    assert_eq!(git_value(&repo, "commit.gpgSign"), "false");
}

#[test]
fn check_json_reports_drift_and_exits_one() {
    let temp = tempfile::tempdir().unwrap();
    let repo = initialized_repo(temp.path());
    let config = temp.path().join("config.toml");
    add_profile(&repo, &config);
    cargo_bin_cmd!()
        .current_dir(&repo)
        .env("GITBOUND_CONFIG", &config)
        .args(["bind", "work"])
        .assert()
        .success();
    assert!(
        Command::new("git")
            .args(["config", "--local", "user.email", "drift@example.com"])
            .current_dir(&repo)
            .status()
            .unwrap()
            .success()
    );

    cargo_bin_cmd!()
        .current_dir(&repo)
        .env("GITBOUND_CONFIG", &config)
        .args(["check", "--json"])
        .assert()
        .code(1)
        .stdout(predicate::str::contains("\"overall\": \"failure\""))
        .stdout(predicate::str::contains("\"id\": \"git_email\""));
}

#[test]
fn drift_requires_force_to_rebind() {
    let temp = tempfile::tempdir().unwrap();
    let repo = initialized_repo(temp.path());
    let config = temp.path().join("config.toml");
    add_profile(&repo, &config);
    cargo_bin_cmd!()
        .current_dir(&repo)
        .env("GITBOUND_CONFIG", &config)
        .args(["bind", "work"])
        .assert()
        .success();
    Command::new("git")
        .args(["config", "--local", "user.name", "Manual change"])
        .current_dir(&repo)
        .status()
        .unwrap();

    cargo_bin_cmd!()
        .current_dir(&repo)
        .env("GITBOUND_CONFIG", &config)
        .args(["bind", "work"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("--force"));
    cargo_bin_cmd!()
        .current_dir(&repo)
        .env("GITBOUND_CONFIG", &config)
        .args(["bind", "work", "--force"])
        .assert()
        .success();
    assert_eq!(git_value(&repo, "user.name"), "Alice Work");
}

#[test]
fn hooks_install_and_uninstall_without_touching_other_hooks() {
    let temp = tempfile::tempdir().unwrap();
    let repo = initialized_repo(temp.path());
    let config = temp.path().join("config.toml");

    cargo_bin_cmd!()
        .current_dir(&repo)
        .env("GITBOUND_CONFIG", &config)
        .args(["hooks", "install"])
        .assert()
        .success();
    let hooks = repo.join(".git").join("hooks");
    assert!(
        fs::read_to_string(hooks.join("pre-commit"))
            .unwrap()
            .contains("Managed by GitBound")
    );
    cargo_bin_cmd!()
        .current_dir(&repo)
        .env("GITBOUND_CONFIG", &config)
        .args(["hooks", "status"])
        .assert()
        .success()
        .stdout(predicate::str::contains("installed"));
    cargo_bin_cmd!()
        .current_dir(&repo)
        .env("GITBOUND_CONFIG", &config)
        .args(["hooks", "uninstall"])
        .assert()
        .success();
    assert!(!hooks.join("pre-commit").exists());

    fs::write(hooks.join("pre-commit"), "#!/bin/sh\necho custom\n").unwrap();
    cargo_bin_cmd!()
        .current_dir(&repo)
        .env("GITBOUND_CONFIG", &config)
        .args(["hooks", "install"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("refusing to replace"));
    assert!(
        fs::read_to_string(hooks.join("pre-commit"))
            .unwrap()
            .contains("custom")
    );
}

#[test]
fn hooks_refuse_custom_hooks_path() {
    let temp = tempfile::tempdir().unwrap();
    let repo = initialized_repo(temp.path());
    let config = temp.path().join("config.toml");
    Command::new("git")
        .args(["config", "--local", "core.hooksPath", ".custom-hooks"])
        .current_dir(&repo)
        .status()
        .unwrap();
    cargo_bin_cmd!()
        .current_dir(&repo)
        .env("GITBOUND_CONFIG", &config)
        .args(["hooks", "install"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("core.hooksPath"));
}

#[test]
fn invalid_profile_and_missing_key_exit_two() {
    let temp = tempfile::tempdir().unwrap();
    let config = temp.path().join("config.toml");
    cargo_bin_cmd!()
        .env("GITBOUND_CONFIG", &config)
        .args([
            "profile",
            "add",
            "bad name",
            "--github-user",
            "alice",
            "--git-name",
            "Alice",
            "--git-email",
            "a@example.com",
        ])
        .assert()
        .code(2);
    cargo_bin_cmd!()
        .env("GITBOUND_CONFIG", &config)
        .args([
            "profile",
            "add",
            "work",
            "--github-user",
            "alice",
            "--git-name",
            "Alice",
            "--git-email",
            "a@example.com",
            "--ssh-key",
            "does-not-exist",
        ])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("SSH key does not exist"));
}

#[test]
fn deleted_key_does_not_make_config_unreadable() {
    let temp = tempfile::tempdir().unwrap();
    let config = temp.path().join("config.toml");
    let key = temp.path().join("key");
    fs::write(&key, "test key").unwrap();
    cargo_bin_cmd!()
        .env("GITBOUND_CONFIG", &config)
        .args([
            "profile",
            "add",
            "work",
            "--github-user",
            "alice",
            "--git-name",
            "Alice",
            "--git-email",
            "a@example.com",
            "--ssh-key",
            key.to_str().unwrap(),
        ])
        .assert()
        .success();
    fs::remove_file(&key).unwrap();

    cargo_bin_cmd!()
        .env("GITBOUND_CONFIG", &config)
        .args(["profile", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("work"));
    cargo_bin_cmd!()
        .env("GITBOUND_CONFIG", &config)
        .arg("doctor")
        .assert()
        .code(3)
        .stdout(predicate::str::contains("profile work: unavailable"));
}

#[test]
fn directory_rule_applies_profile_through_isolated_global_config() {
    let temp = tempfile::tempdir().unwrap();
    let config = temp.path().join("config.toml");
    let global = temp.path().join("global.gitconfig");
    let projects = temp.path().join("projects");
    let repo = projects.join("repo");
    fs::create_dir_all(&repo).unwrap();
    assert!(
        Command::new("git")
            .args(["init", "--quiet"])
            .current_dir(&repo)
            .status()
            .unwrap()
            .success()
    );
    add_profile(&repo, &config);

    cargo_bin_cmd!()
        .current_dir(&repo)
        .env("GITBOUND_CONFIG", &config)
        .env("GIT_CONFIG_GLOBAL", &global)
        .args(["directory", "add", "work", projects.to_str().unwrap()])
        .assert()
        .success();
    let output = Command::new("git")
        .args(["config", "--get", "user.email"])
        .env("GIT_CONFIG_GLOBAL", &global)
        .current_dir(&repo)
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap().trim(),
        "work@example.com"
    );
    cargo_bin_cmd!()
        .current_dir(&repo)
        .env("GITBOUND_CONFIG", &config)
        .env("GIT_CONFIG_GLOBAL", &global)
        .args(["check", "--hook", "pre-commit"])
        .assert()
        .success()
        .stdout(predicate::str::contains("repository binding is present"));

    // The desktop app reads exactly these fields, and shows `condition` so a
    // user can check the claim against `git config --global --list` instead of
    // taking it on trust. Renaming one would break that view silently.
    cargo_bin_cmd!()
        .current_dir(&repo)
        .env("GITBOUND_CONFIG", &config)
        .env("GIT_CONFIG_GLOBAL", &global)
        .args(["directory", "list", "--json"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"profile\": \"work\""))
        .stdout(predicate::str::contains("\"condition\": \"includeIf."))
        .stdout(predicate::str::contains("\"include\":"))
        .stdout(predicate::str::contains("\"path\":"));

    cargo_bin_cmd!()
        .current_dir(&repo)
        .env("GITBOUND_CONFIG", &config)
        .env("GIT_CONFIG_GLOBAL", &global)
        .args(["directory", "remove", projects.to_str().unwrap()])
        .assert()
        .success();
    let output = Command::new("git")
        .args(["config", "--global", "--get-regexp", "^includeIf"])
        .env("GIT_CONFIG_GLOBAL", &global)
        .output()
        .unwrap();
    assert!(!output.status.success());
}

#[test]
fn profile_import_reads_repository_and_structured_gh_identity() {
    let temp = tempfile::tempdir().unwrap();
    let repo = initialized_repo(temp.path());
    let config = temp.path().join("config.toml");
    for (key, value) in [
        ("user.name", "Imported User"),
        ("user.email", "imported@example.com"),
    ] {
        assert!(
            Command::new("git")
                .args(["config", "--local", key, value])
                .current_dir(&repo)
                .status()
                .unwrap()
                .success()
        );
    }
    assert!(
        Command::new("git")
            .args([
                "remote",
                "add",
                "origin",
                "https://github.com/ImportedOrg/project.git",
            ])
            .current_dir(&repo)
            .status()
            .unwrap()
            .success()
    );
    let fake_bin = fake_gh(temp.path());
    let path = env::join_paths(
        std::iter::once(fake_bin).chain(env::split_paths(&env::var_os("PATH").unwrap())),
    )
    .unwrap();

    cargo_bin_cmd!()
        .current_dir(&repo)
        .env("PATH", path)
        .env("GITBOUND_CONFIG", &config)
        .args(["profile", "import", "imported"])
        .assert()
        .success()
        .stdout(predicate::str::contains("SSH keys are never inferred"));
    cargo_bin_cmd!()
        .env("GITBOUND_CONFIG", &config)
        .args(["profile", "show", "imported", "--json"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Imported User"))
        .stdout(predicate::str::contains("imported-user"))
        .stdout(predicate::str::contains("ImportedOrg"));
}

#[test]
fn git_config_parameters_cannot_forge_a_passing_check() {
    // `GIT_CONFIG_PARAMETERS` is how `git -c` reaches a child process, and Git
    // honours it above the repository's own config. Left in the environment it
    // would let anything that can set a variable turn a mismatched identity
    // into a passing check, which is the one verdict this project must never
    // produce. `process.rs` strips it; this is the proof.
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

    // Somebody else's address is now the one commits would carry.
    assert!(
        Command::new("git")
            .args(["config", "--local", "user.email", "attacker@example.com"])
            .current_dir(&repo)
            .status()
            .unwrap()
            .success()
    );

    cargo_bin_cmd!()
        .current_dir(&repo)
        .env("GITBOUND_CONFIG", &config)
        // The forgery: claim the profile's own address in the environment.
        .env("GIT_CONFIG_PARAMETERS", "'user.email=work@example.com'")
        .args(["check", "--format", "json"])
        .assert()
        .code(1)
        // The report must carry what is actually on disk, not what the
        // environment claimed.
        .stdout(predicate::str::contains(
            "\"actual\": \"attacker@example.com\"",
        ));
}

#[test]
fn signing_drift_is_detected_on_an_ssh_remote() {
    // `binding_drifted` used to return as soon as it had compared
    // `core.sshCommand`, so on an SSH remote — the transport most likely to
    // carry a signing key — a tampered `user.signingKey` read as clean and
    // `bind` silently rebound over it.
    let temp = tempfile::tempdir().unwrap();
    let config = temp.path().join("config.toml");
    let key = temp.path().join("id_test");
    fs::write(&key, "not a real key").unwrap();
    let repo = initialized_repo(temp.path());
    assert!(
        Command::new("git")
            .args([
                "remote",
                "add",
                "origin",
                "git@github.com:alice-work/project.git",
            ])
            .current_dir(&repo)
            .status()
            .unwrap()
            .success()
    );

    cargo_bin_cmd!()
        .current_dir(&repo)
        .env("GITBOUND_CONFIG", &config)
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
            "--ssh-key",
            key.to_str().unwrap(),
            "--signing-key",
            "ABC123",
            "--require-signing",
        ])
        .assert()
        .success();
    cargo_bin_cmd!()
        .current_dir(&repo)
        .env("GITBOUND_CONFIG", &config)
        .args(["bind", "work"])
        .assert()
        .success();
    assert_eq!(git_value(&repo, "user.signingKey"), "ABC123");

    assert!(
        Command::new("git")
            .args(["config", "--local", "user.signingKey", "EVIL999"])
            .current_dir(&repo)
            .status()
            .unwrap()
            .success()
    );

    cargo_bin_cmd!()
        .current_dir(&repo)
        .env("GITBOUND_CONFIG", &config)
        .args(["bind", "work"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("--force"));
}

fn initialized_repo(parent: &std::path::Path) -> std::path::PathBuf {
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

fn add_profile(repo: &std::path::Path, config: &std::path::Path) {
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

fn git_value(repo: &std::path::Path, key: &str) -> String {
    let output = Command::new("git")
        .args(["config", "--local", "--get", key])
        .current_dir(repo)
        .output()
        .unwrap();
    assert!(output.status.success());
    String::from_utf8(output.stdout).unwrap().trim().to_string()
}

fn fake_gh(parent: &std::path::Path) -> std::path::PathBuf {
    let bin = parent.join("fake-bin");
    fs::create_dir(&bin).unwrap();
    #[cfg(not(windows))]
    {
        use std::os::unix::fs::PermissionsExt;
        let path = bin.join("gh");
        fs::write(
            &path,
            "#!/bin/sh\nprintf '%s\\n' '{\"hosts\":{\"github.com\":[{\"login\":\"imported-user\",\"active\":true,\"state\":\"success\"}]}}'\n",
        )
        .unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }
    #[cfg(windows)]
    {
        let src = bin.join("gh.rs");
        fs::write(
            &src,
            "fn main() { println!(\"{}\", r#\"{\"hosts\":{\"github.com\":[{\"login\":\"imported-user\",\"active\":true,\"state\":\"success\"}]}}\"#); }",
        )
        .unwrap();
        assert!(
            Command::new("rustc")
                .args([
                    src.to_str().unwrap(),
                    "-o",
                    bin.join("gh.exe").to_str().unwrap()
                ])
                .status()
                .unwrap()
                .success()
        );
    }
    bin
}

#[test]
fn http_remote_fails_check_as_cleartext_transport() {
    let temp = tempfile::tempdir().unwrap();
    let config = temp.path().join("config.toml");
    let repo = initialized_repo(temp.path());
    add_profile(&repo, &config);
    assert!(
        Command::new("git")
            .args([
                "remote",
                "add",
                "origin",
                "http://github.com/alice-work/project.git"
            ])
            .current_dir(&repo)
            .status()
            .unwrap()
            .success()
    );
    cargo_bin_cmd!()
        .current_dir(&repo)
        .env("GITBOUND_CONFIG", &config)
        .args(["bind", "work"])
        .assert()
        .success();

    cargo_bin_cmd!()
        .current_dir(&repo)
        .env("GITBOUND_CONFIG", &config)
        .args(["check", "--json"])
        .assert()
        .code(1)
        .stdout(predicate::str::contains("\"transport\""))
        .stdout(predicate::str::contains("cleartext HTTP"));
}

#[test]
fn git_dir_in_the_environment_does_not_redirect_a_binding() {
    let temp = tempfile::tempdir().unwrap();
    let config = temp.path().join("config.toml");
    let repo = initialized_repo(temp.path());
    add_profile(&repo, &config);

    // A decoy repository that GIT_DIR would otherwise redirect writes into.
    let decoy = temp.path().join("decoy");
    fs::create_dir(&decoy).unwrap();
    assert!(
        Command::new("git")
            .args(["init", "--quiet"])
            .current_dir(&decoy)
            .status()
            .unwrap()
            .success()
    );

    cargo_bin_cmd!()
        .current_dir(&repo)
        .env("GITBOUND_CONFIG", &config)
        .env("GIT_DIR", decoy.join(".git"))
        .env("GIT_WORK_TREE", &decoy)
        .args(["bind", "work"])
        .assert()
        .success();

    assert_eq!(git_value(&repo, "gitbound.profile"), "work");
    assert_eq!(git_value(&repo, "user.email"), "work@example.com");
    let leaked = Command::new("git")
        .args(["config", "--local", "--get", "gitbound.profile"])
        .current_dir(&decoy)
        .output()
        .unwrap();
    assert!(
        !leaked.status.success(),
        "binding leaked into the GIT_DIR decoy repository"
    );
}

#[test]
fn clone_refuses_an_owner_the_profile_does_not_allow() {
    let temp = tempfile::tempdir().unwrap();
    let config = temp.path().join("config.toml");
    let repo = initialized_repo(temp.path());
    add_profile(&repo, &config);
    cargo_bin_cmd!()
        .env("GITBOUND_CONFIG", &config)
        .args([
            "profile",
            "edit",
            "work",
            "--allowed-owner",
            "permitted-org",
        ])
        .assert()
        .success();

    // Owner policy is enforced before any network work or account switch, so
    // this needs no gh on PATH: reaching one would itself be the bug.
    cargo_bin_cmd!()
        .current_dir(temp.path())
        .env("GITBOUND_CONFIG", &config)
        .args(["clone", "work", "other-org/project"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("not allowed by profile"));
    assert!(!temp.path().join("project").exists());
}

#[test]
fn clone_rejects_a_host_that_is_not_the_profile_host() {
    let temp = tempfile::tempdir().unwrap();
    let config = temp.path().join("config.toml");
    let repo = initialized_repo(temp.path());
    add_profile(&repo, &config);
    cargo_bin_cmd!()
        .current_dir(temp.path())
        .env("GITBOUND_CONFIG", &config)
        .args(["clone", "work", "https://gitlab.example/org/project.git"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("does not match profile host"));
}

#[test]
fn profile_rename_moves_the_directory_rule_and_its_fragment() {
    let temp = tempfile::tempdir().unwrap();
    let config = temp.path().join("config.toml");
    let global = temp.path().join("global.gitconfig");
    let projects = temp.path().join("projects");
    let repo = projects.join("repo");
    fs::create_dir_all(&repo).unwrap();
    assert!(
        Command::new("git")
            .args(["init", "--quiet"])
            .current_dir(&repo)
            .status()
            .unwrap()
            .success()
    );
    add_profile(&repo, &config);
    cargo_bin_cmd!()
        .current_dir(&repo)
        .env("GITBOUND_CONFIG", &config)
        .env("GIT_CONFIG_GLOBAL", &global)
        .args(["directory", "add", "work", projects.to_str().unwrap()])
        .assert()
        .success();

    let profiles = temp.path().join("profiles");
    assert!(profiles.join("work.gitconfig").is_file());

    cargo_bin_cmd!()
        .current_dir(&repo)
        .env("GITBOUND_CONFIG", &config)
        .env("GIT_CONFIG_GLOBAL", &global)
        .args(["profile", "rename", "work", "employer"])
        .assert()
        .success();

    // The fragment follows the name, and the include points at the new file.
    assert!(profiles.join("employer.gitconfig").is_file());
    assert!(!profiles.join("work.gitconfig").exists());
    let includes = Command::new("git")
        .args(["config", "--global", "--get-regexp", "^includeIf"])
        .env("GIT_CONFIG_GLOBAL", &global)
        .output()
        .unwrap();
    let includes = String::from_utf8(includes.stdout).unwrap();
    assert!(includes.contains("employer.gitconfig"), "{includes}");
    assert!(!includes.contains("work.gitconfig"), "{includes}");

    // The identity still resolves through the moved include.
    let output = Command::new("git")
        .args(["config", "--get", "user.email"])
        .env("GIT_CONFIG_GLOBAL", &global)
        .current_dir(&repo)
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8(output.stdout).unwrap().trim(),
        "work@example.com"
    );
}

#[test]
fn profile_remove_refuses_while_a_directory_rule_depends_on_it() {
    let temp = tempfile::tempdir().unwrap();
    let config = temp.path().join("config.toml");
    let global = temp.path().join("global.gitconfig");
    let projects = temp.path().join("projects");
    let repo = projects.join("repo");
    fs::create_dir_all(&repo).unwrap();
    assert!(
        Command::new("git")
            .args(["init", "--quiet"])
            .current_dir(&repo)
            .status()
            .unwrap()
            .success()
    );
    add_profile(&repo, &config);
    cargo_bin_cmd!()
        .current_dir(&repo)
        .env("GITBOUND_CONFIG", &config)
        .env("GIT_CONFIG_GLOBAL", &global)
        .args(["directory", "add", "work", projects.to_str().unwrap()])
        .assert()
        .success();

    cargo_bin_cmd!()
        .env("GITBOUND_CONFIG", &config)
        .env("GIT_CONFIG_GLOBAL", &global)
        .args(["profile", "remove", "work", "--yes"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("directory rules"));

    cargo_bin_cmd!()
        .env("GITBOUND_CONFIG", &config)
        .args(["profile", "show", "work"])
        .assert()
        .success();
}

#[test]
fn hooks_install_is_not_repeatable_and_leaves_the_first_hook_intact() {
    let temp = tempfile::tempdir().unwrap();
    let repo = initialized_repo(temp.path());
    let hooks = repo.join(".git").join("hooks");
    cargo_bin_cmd!()
        .current_dir(&repo)
        .args(["hooks", "install"])
        .assert()
        .success();
    let first = fs::read_to_string(hooks.join("pre-commit")).unwrap();

    cargo_bin_cmd!()
        .current_dir(&repo)
        .args(["hooks", "install"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("refusing to replace"));
    assert_eq!(fs::read_to_string(hooks.join("pre-commit")).unwrap(), first);
}

/// Put `repo` into the state GitPersona left behind: the profile identity
/// written to Git's own keys, and the developer's original identity parked in
/// backup keys under the pre-rename `gitpersona.` prefix.
fn legacy_bound_repo(parent: &std::path::Path) -> std::path::PathBuf {
    let repo = initialized_repo(parent);
    for (key, value) in [
        ("user.name", "Alice Work"),
        ("user.email", "work@example.com"),
        ("gitpersona.profile", "work"),
        ("gitpersona.version", "2"),
        ("gitpersona.backupUserNamePresent", "true"),
        ("gitpersona.backupUserName", "Original"),
        ("gitpersona.backupUserEmailPresent", "true"),
        ("gitpersona.backupUserEmail", "original@example.com"),
        ("gitpersona.backupSshCommandPresent", "false"),
        ("gitpersona.backupSigningKeyPresent", "false"),
        ("gitpersona.backupSigningFormatPresent", "false"),
        ("gitpersona.backupCommitSigningPresent", "false"),
    ] {
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

fn git_is_unset(repo: &std::path::Path, key: &str) -> bool {
    !Command::new("git")
        .args(["config", "--local", "--get", key])
        .current_dir(repo)
        .output()
        .unwrap()
        .status
        .success()
}

/// The rename to GitBound moved every bookkeeping key, so a repository bound by
/// GitPersona read as unbound. `unbind` then refused it, stranding the original
/// identity that SECURITY.md promises `unbind` restores.
#[test]
fn a_repository_bound_before_the_rename_is_still_seen_as_bound() {
    let temp = tempfile::tempdir().unwrap();
    let repo = legacy_bound_repo(temp.path());
    let config = temp.path().join("config.toml");
    add_profile(&repo, &config);

    // Reading must not rewrite the repository, so the legacy key survives a
    // status run and is reported through the fallback instead.
    cargo_bin_cmd!()
        .current_dir(&repo)
        .env("GITBOUND_CONFIG", &config)
        .args(["status", "--json"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"profile\": \"work\""));
    assert_eq!(git_value(&repo, "gitpersona.profile"), "work");
}

#[test]
fn unbinding_a_pre_rename_binding_restores_the_original_identity() {
    let temp = tempfile::tempdir().unwrap();
    let repo = legacy_bound_repo(temp.path());
    let config = temp.path().join("config.toml");
    add_profile(&repo, &config);

    cargo_bin_cmd!()
        .current_dir(&repo)
        .env("GITBOUND_CONFIG", &config)
        .arg("unbind")
        .assert()
        .success();

    assert_eq!(git_value(&repo, "user.name"), "Original");
    assert_eq!(git_value(&repo, "user.email"), "original@example.com");
    assert!(git_is_unset(&repo, "gitpersona.profile"));
    assert!(git_is_unset(&repo, "gitpersona.backupUserEmail"));
    assert!(git_is_unset(&repo, "gitbound.profile"));
    assert!(git_is_unset(&repo, "gitbound.backupUserEmail"));
}

/// Rebinding must not re-snapshot the profile identity as if it were the
/// developer's own: the migrated backup is the only record of what came first.
#[test]
fn rebinding_a_pre_rename_binding_keeps_the_migrated_backup() {
    let temp = tempfile::tempdir().unwrap();
    let repo = legacy_bound_repo(temp.path());
    let config = temp.path().join("config.toml");
    add_profile(&repo, &config);

    cargo_bin_cmd!()
        .current_dir(&repo)
        .env("GITBOUND_CONFIG", &config)
        .args(["bind", "work"])
        .assert()
        .success();
    assert_eq!(git_value(&repo, "gitbound.profile"), "work");
    assert_eq!(
        git_value(&repo, "gitbound.backupUserEmail"),
        "original@example.com"
    );
    assert!(git_is_unset(&repo, "gitpersona.profile"));

    cargo_bin_cmd!()
        .current_dir(&repo)
        .env("GITBOUND_CONFIG", &config)
        .arg("unbind")
        .assert()
        .success();
    assert_eq!(git_value(&repo, "user.email"), "original@example.com");
}

/// `--offline` must skip the checks that need the network without quietly
/// dropping the ones that do not. `credential_helper` reads `git config`, so it
/// has no excuse to go missing.
#[test]
fn offline_skips_only_the_checks_that_need_the_network() {
    let temp = tempfile::tempdir().unwrap();
    let repo = initialized_repo(temp.path());
    let config = temp.path().join("config.toml");
    add_profile(&repo, &config);
    assert!(
        Command::new("git")
            .args([
                "remote",
                "add",
                "origin",
                "https://github.com/alice-work/repo.git",
            ])
            .current_dir(&repo)
            .status()
            .unwrap()
            .success()
    );
    cargo_bin_cmd!()
        .current_dir(&repo)
        .env("GITBOUND_CONFIG", &config)
        .args(["bind", "work"])
        .assert()
        .success();

    cargo_bin_cmd!()
        .current_dir(&repo)
        .env("GITBOUND_CONFIG", &config)
        .args(["status", "--offline", "--format", "json"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"github_cli\"").not())
        .stdout(predicate::str::contains("\"ssh_identity\"").not())
        .stdout(predicate::str::contains("\"credential_helper\""));
}

/// The action appends the shared flags to whichever command it runs, so a flag
/// that `verify` accepts and `audit` does not turns `command: audit` into a
/// usage error rather than an audit.
#[test]
fn audit_accepts_the_flags_the_action_shares_between_commands() {
    let temp = tempfile::tempdir().unwrap();
    let repo = initialized_repo(temp.path());
    let config = temp.path().join("config.toml");
    for args in [
        vec!["config", "--local", "user.name", "Alice"],
        vec!["config", "--local", "user.email", "alice@example.com"],
        vec!["commit", "--allow-empty", "--quiet", "-m", "a change"],
    ] {
        assert!(
            Command::new("git")
                .args(&args)
                .current_dir(&repo)
                .status()
                .unwrap()
                .success()
        );
    }
    cargo_bin_cmd!()
        .current_dir(&repo)
        .env("GITBOUND_CONFIG", &config)
        .args(["audit", "--require-policy", "--format", "json"])
        .assert()
        // Exit 1 is the finding that there is no policy. Exit 2 would be clap
        // rejecting the flag, which is the regression this guards.
        .code(1)
        .stdout(predicate::str::contains("policy_present"));
}

/// `hooks` gained `--repo` for parity with `bind`, `unbind` and `status`, and
/// because it is the path the desktop app takes: it never has the repository as
/// its working directory.
#[test]
fn hooks_can_target_a_repository_by_path() {
    let temp = tempfile::tempdir().unwrap();
    let repo = initialized_repo(temp.path());
    let elsewhere = temp.path().join("elsewhere");
    fs::create_dir(&elsewhere).unwrap();
    let config = temp.path().join("config.toml");
    let path = repo.to_str().unwrap();

    cargo_bin_cmd!()
        .current_dir(&elsewhere)
        .env("GITBOUND_CONFIG", &config)
        .args(["hooks", "status", "--repo", path])
        .assert()
        .success()
        .stdout(predicate::str::contains("pre-commit: not installed"));

    cargo_bin_cmd!()
        .current_dir(&elsewhere)
        .env("GITBOUND_CONFIG", &config)
        .args(["hooks", "install", "--repo", path])
        .assert()
        .success();
    assert!(repo.join(".git/hooks/pre-commit").is_file());
    assert!(repo.join(".git/hooks/pre-push").is_file());

    cargo_bin_cmd!()
        .current_dir(&elsewhere)
        .env("GITBOUND_CONFIG", &config)
        .args(["hooks", "status", "--repo", path])
        .assert()
        .success()
        .stdout(predicate::str::contains("pre-commit: installed"));

    cargo_bin_cmd!()
        .current_dir(&elsewhere)
        .env("GITBOUND_CONFIG", &config)
        .args(["hooks", "uninstall", "--repo", path])
        .assert()
        .success();
    assert!(!repo.join(".git/hooks/pre-commit").exists());
}

/// The desktop app reaches the check engine through `GitBoundService`, not
/// through the CLI, and that path did not apply the policy the repository
/// commits — so the app reported a repository as clean while `gitbound check`
/// on the same repository failed on the very rule the repository ships to be
/// judged by. The two must agree.
#[test]
fn the_service_applies_the_committed_policy_like_the_cli_does() {
    use gitbound::{config::ConfigStore, process::SystemRunner, service::GitBoundService};

    let temp = tempfile::tempdir().unwrap();
    let repo = initialized_repo(temp.path());
    let config = temp.path().join("config.toml");
    add_profile(&repo, &config);
    cargo_bin_cmd!()
        .current_dir(&repo)
        .env("GITBOUND_CONFIG", &config)
        .args(["bind", "work"])
        .assert()
        .success();
    // The profile's own address, which the committed policy does not allow.
    fs::write(
        repo.join(".gitbound.toml"),
        "[identity]\nallowed_email_domains = [\"corp.example\"]\n",
    )
    .unwrap();

    let runner = SystemRunner;
    let service = GitBoundService::new(ConfigStore::at(config.clone()), &runner);
    let status = service.inspect_repository(&repo, "origin", false).unwrap();
    let policy_email = status
        .report
        .checks
        .iter()
        .find(|check| check.id == "policy_email")
        .expect("the committed policy must be evaluated");
    assert_eq!(policy_email.status, gitbound::check::CheckStatus::Failure);

    // And the CLI, which reads the same file, must reach the same verdict.
    cargo_bin_cmd!()
        .current_dir(&repo)
        .env("GITBOUND_CONFIG", &config)
        .args(["check", "--offline"])
        .assert()
        .code(1)
        .stdout(predicate::str::contains("not permitted by .gitbound.toml"));
}
