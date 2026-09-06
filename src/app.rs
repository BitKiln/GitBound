use crate::{
    check::{self, CheckItem, CheckOptions, CheckReport, CheckStatus},
    cli::{
        AuditArgs, BindArgs, CiCommand, Cli, Command, DoctorArgs, HookMode, HooksCommand,
        InspectArgs, ProfileCommand, ProfileImportArgs, ProfileMutationArgs, SshCommand,
    },
    clone_repo,
    config::{ConfigStore, Profile, validate_profile_name},
    directory,
    error::GitBoundError,
    git::Git,
    github::GitHub,
    hooks::HookManager,
    policy::{POLICY_FILE, Policy},
    process::{Runner, os_args},
    report::{self, ReportFormat},
    service::GitBoundService,
};
use clap::CommandFactory;
use dialoguer::{Confirm, Input};
use std::{
    io::{self, IsTerminal},
    time::Duration,
};

pub fn run(cli: Cli, runner: &dyn Runner) -> Result<u8, GitBoundError> {
    let store = ConfigStore::discover()?;
    match cli.command {
        Command::Profile { command } => profile_command(command, &store, runner),
        Command::Use { profile } => use_profile(&profile, &store, runner),
        Command::Clone(args) => clone_repo::execute(args, &store, runner),
        Command::Bind(args) => bind(args, &store, runner),
        Command::Unbind(args) => {
            git_for(runner, args.repo.as_deref()).unbind()?;
            println!("Repository unbound; original Git settings restored.");
            Ok(0)
        }
        // `expect_binding` is the one real difference between these three.
        // `status` and `check` run on the machine that owns the binding, so an
        // unbound repository is a fault. `verify` runs against a pipeline
        // checkout, which has no user configuration and therefore no binding to
        // find.
        Command::Status(args) => inspect(args, &store, runner, Mode::REPORT),
        Command::Check(args) => inspect(args, &store, runner, Mode::LOCAL_GATE),
        Command::Verify(args) => inspect(args, &store, runner, Mode::PIPELINE_GATE),
        Command::Audit(args) => audit(args, &store, runner),
        Command::Hooks { command } => hooks(command, runner),
        Command::Directory { command } => directory::execute(command, &store, runner),
        Command::Doctor(args) => doctor(args, &store, runner),
        Command::Ci { command } => ci(command, &store, runner),
        Command::Ssh { command } => ssh(command, &store, runner),
        Command::Completions { shell } => {
            clap_complete::generate(shell, &mut Cli::command(), "gitbound", &mut io::stdout());
            Ok(0)
        }
    }
}

fn profile_command(
    command: ProfileCommand,
    store: &ConfigStore,
    runner: &dyn Runner,
) -> Result<u8, GitBoundError> {
    match command {
        ProfileCommand::Add(args) => {
            validate_profile_name(&args.name)?;
            let profile = profile_from_args(&args, None, true)?;
            profile.validate()?;
            profile.validate_local_resources()?;
            store.update(|config| {
                if config.profiles.contains_key(&args.name) {
                    return Err(GitBoundError::usage(format!(
                        "profile '{}' already exists",
                        args.name
                    )));
                }
                config.profiles.insert(args.name.clone(), profile);
                Ok(())
            })?;
            println!("Profile '{}' added.", args.name);
            Ok(0)
        }
        ProfileCommand::Import(args) => import_profile(args, store, runner),
        ProfileCommand::Edit(args) => {
            validate_profile_name(&args.name)?;
            store.update(|config| {
                let old = config.profiles.get(&args.name).cloned().ok_or_else(|| {
                    GitBoundError::usage(format!("profile '{}' does not exist", args.name))
                })?;
                let profile = profile_from_args(&args, Some(&old), false)?;
                profile.validate()?;
                profile.validate_local_resources()?;
                config.profiles.insert(args.name.clone(), profile);
                Ok(())
            })?;
            println!(
                "Profile '{}' updated. Rebind repositories to apply changed settings.",
                args.name
            );
            directory::sync_profile(&args.name, store)?;
            Ok(0)
        }
        ProfileCommand::List { json } => {
            let config = store.load()?;
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&config.profiles).map_err(json_error)?
                );
            } else if config.profiles.is_empty() {
                println!("No profiles configured.");
            } else {
                for (name, profile) in config.profiles {
                    println!("{name}\t{}@{}", profile.github_user, profile.hostname);
                }
            }
            Ok(0)
        }
        ProfileCommand::Show { name, json } => {
            let config = store.load()?;
            let profile = config
                .profiles
                .get(&name)
                .ok_or_else(|| GitBoundError::usage(format!("profile '{name}' does not exist")))?;
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(profile).map_err(json_error)?
                );
            } else {
                println!(
                    "Profile:        {name}\nGitHub user:    {}\nHostname:       {}\nSSH host:       {}\nGit author:     {}\nGit email:      {}\nSSH key:        {}\nAllowed owners: {}\nSigning key:    {}\nSigning format: {}\nRequire signing: {}",
                    profile.github_user,
                    profile.hostname,
                    profile
                        .ssh_host
                        .as_deref()
                        .map_or_else(|| "(same as hostname)".into(), String::from),
                    profile.git_name,
                    profile.git_email,
                    profile
                        .ssh_key
                        .as_ref()
                        .map_or_else(|| "(none)".into(), |p| p.display().to_string()),
                    if profile.allowed_owners.is_empty() {
                        "(any)".into()
                    } else {
                        profile.allowed_owners.join(", ")
                    },
                    profile.signing_key.as_deref().unwrap_or("(none)"),
                    profile.signing_format.as_git_value(),
                    profile.require_signing,
                );
            }
            Ok(0)
        }
        ProfileCommand::Remove { name, yes } => {
            let config = store.load()?;
            if !config.profiles.contains_key(&name) {
                return Err(GitBoundError::usage(format!(
                    "profile '{name}' does not exist"
                )));
            }
            if config.directories.iter().any(|rule| rule.profile == name) {
                return Err(GitBoundError::usage(format!(
                    "profile '{name}' has directory rules; remove them first"
                )));
            }
            if !yes {
                if !io::stdin().is_terminal() {
                    return Err(GitBoundError::usage(
                        "profile removal requires --yes in non-interactive mode",
                    ));
                }
                if !Confirm::new()
                    .with_prompt(format!(
                        "Remove profile '{name}'? Bound repositories will report a missing profile"
                    ))
                    .default(false)
                    .interact()
                    .map_err(prompt_error)?
                {
                    return Ok(0);
                }
            }
            store.update(|config| {
                config.profiles.remove(&name);
                Ok(())
            })?;
            println!("Profile '{name}' removed.");
            Ok(0)
        }
        ProfileCommand::Rename { old_name, new_name } => {
            let service = GitBoundService::new(store.clone(), runner);
            service.rename_profile(&old_name, &new_name)?;
            println!("Profile '{old_name}' renamed to '{new_name}'.");
            Ok(0)
        }
    }
}

fn import_profile(
    args: ProfileImportArgs,
    store: &ConfigStore,
    runner: &dyn Runner,
) -> Result<u8, GitBoundError> {
    validate_profile_name(&args.name)?;
    if store.load()?.profiles.contains_key(&args.name) {
        return Err(GitBoundError::usage(format!(
            "profile '{}' already exists",
            args.name
        )));
    }
    let git = git_for(runner, args.repo.as_deref());
    git.ensure_repo()?;
    let remote = git.remote(&args.remote)?.ok_or_else(|| {
        GitBoundError::usage(format!("remote '{}' is not configured", args.remote))
    })?;
    let required_git = |key: &str, label: &str| {
        git.get(key, false)?.ok_or_else(|| {
            GitBoundError::usage(format!("cannot import profile: {label} is not configured"))
        })
    };
    let github_user = GitHub::new(runner)
        .active_account(&remote.hostname)?
        .ok_or_else(|| {
            GitBoundError::usage(format!(
                "cannot import profile: GitHub CLI has no active account on {}",
                remote.hostname
            ))
        })?;
    let signing_format = match git.get("gpg.format", false)?.as_deref() {
        Some("ssh") => crate::config::SigningFormat::Ssh,
        _ => crate::config::SigningFormat::Openpgp,
    };
    let profile = Profile {
        github_user,
        git_name: required_git("user.name", "Git author name")?,
        git_email: required_git("user.email", "Git author email")?,
        hostname: remote.hostname,
        ssh_host: None,
        ssh_key: None,
        allowed_owners: if args.no_owner {
            Vec::new()
        } else {
            vec![remote.owner]
        },
        signing_key: git.get("user.signingKey", false)?,
        signing_format,
        require_signing: git
            .get("commit.gpgSign", false)?
            .is_some_and(|value| value.eq_ignore_ascii_case("true")),
    };
    profile.validate()?;
    profile.validate_local_resources()?;
    store.update(|config| {
        if config.profiles.contains_key(&args.name) {
            return Err(GitBoundError::usage(format!(
                "profile '{}' already exists",
                args.name
            )));
        }
        config.profiles.insert(args.name.clone(), profile);
        Ok(())
    })?;
    println!(
        "Profile '{}' imported from the current repository. SSH keys are never inferred; add one explicitly if needed.",
        args.name
    );
    Ok(0)
}

fn profile_from_args(
    args: &ProfileMutationArgs,
    old: Option<&Profile>,
    require: bool,
) -> Result<Profile, GitBoundError> {
    let github_user = required_value(
        "GitHub user",
        args.github_user
            .clone()
            .or_else(|| old.map(|p| p.github_user.clone())),
        require,
    )?;
    let git_name = required_value(
        "Git author name",
        args.git_name
            .clone()
            .or_else(|| old.map(|p| p.git_name.clone())),
        require,
    )?;
    let git_email = required_value(
        "Git author email",
        args.git_email
            .clone()
            .or_else(|| old.map(|p| p.git_email.clone())),
        require,
    )?;
    let hostname = args
        .hostname
        .clone()
        .or_else(|| old.map(|p| p.hostname.clone()))
        .unwrap_or_else(|| "github.com".into());
    let ssh_host = if args.clear_ssh_host {
        None
    } else {
        args.ssh_host
            .clone()
            .or_else(|| old.and_then(|p| p.ssh_host.clone()))
    };
    let ssh_key = if args.clear_ssh_key {
        None
    } else {
        args.ssh_key
            .clone()
            .or_else(|| old.and_then(|p| p.ssh_key.clone()))
    };
    let allowed_owners = if args.clear_allowed_owners {
        vec![]
    } else if args.allowed_owners.is_empty() {
        old.map_or_else(Vec::new, |p| p.allowed_owners.clone())
    } else {
        args.allowed_owners.clone()
    };
    let signing_key = if args.clear_signing_key {
        None
    } else {
        args.signing_key
            .clone()
            .or_else(|| old.and_then(|profile| profile.signing_key.clone()))
    };
    let signing_format = args
        .signing_format
        .or_else(|| old.map(|profile| profile.signing_format))
        .unwrap_or_default();
    let require_signing = if args.require_signing {
        true
    } else if args.no_require_signing || args.clear_signing_key {
        false
    } else {
        old.is_some_and(|profile| profile.require_signing)
    };
    Ok(Profile {
        github_user,
        git_name,
        git_email,
        hostname,
        ssh_host,
        ssh_key,
        allowed_owners,
        signing_key,
        signing_format,
        require_signing,
    })
}

fn required_value(
    label: &str,
    value: Option<String>,
    require: bool,
) -> Result<String, GitBoundError> {
    if let Some(value) = value {
        return Ok(value);
    }
    if !require {
        return Err(GitBoundError::usage(format!(
            "{label} is missing from the existing profile"
        )));
    }
    if !io::stdin().is_terminal() {
        return Err(GitBoundError::usage(format!(
            "missing required {label}; provide the corresponding flag"
        )));
    }
    Input::<String>::new()
        .with_prompt(label)
        .interact_text()
        .map_err(prompt_error)
}

fn use_profile(name: &str, store: &ConfigStore, runner: &dyn Runner) -> Result<u8, GitBoundError> {
    let config = store.load()?;
    let profile = config
        .profiles
        .get(name)
        .ok_or_else(|| GitBoundError::usage(format!("profile '{name}' does not exist")))?;
    GitHub::new(runner).switch(&profile.hostname, &profile.github_user)?;
    println!(
        "GitHub CLI is now using {} on {}.",
        profile.github_user, profile.hostname
    );
    Ok(0)
}

fn bind(args: BindArgs, store: &ConfigStore, runner: &dyn Runner) -> Result<u8, GitBoundError> {
    let config = store.load()?;
    let profile = config.profiles.get(&args.profile).ok_or_else(|| {
        GitBoundError::usage(format!("profile '{}' does not exist", args.profile))
    })?;
    let git = git_for(runner, args.repo.as_deref());
    git.ensure_repo()?;
    let remote = git.remote(&args.remote)?;
    let old_name = git.binding_profile(true)?;
    let old_profile = old_name.as_ref().and_then(|name| config.profiles.get(name));
    let github = GitHub::new(runner);
    let previous = if args.switch {
        github.active_account(&profile.hostname)?
    } else {
        None
    };
    if args.switch {
        github.switch(&profile.hostname, &profile.github_user)?;
    }
    if let Err(error) = git.bind(
        &args.profile,
        profile,
        remote.as_ref(),
        args.force,
        old_profile,
    ) {
        if args.switch
            && let Some(previous) = previous
            && let Err(rollback) = github.switch(&profile.hostname, &previous)
        {
            return Err(GitBoundError::dependency(format!(
                "{error}; GitHub CLI rollback also failed: {rollback}"
            )));
        }
        return Err(error);
    }
    println!("Repository bound to profile '{}'.", args.profile);
    if !args.switch {
        println!(
            "GitHub CLI was not switched. Run 'gitbound use {}' or rebind with --switch if needed.",
            args.profile
        );
    }
    Ok(0)
}

/// How one of `status`, `check`, or `verify` differs from the others. Everything
/// else about the three is identical, and keeping the differences in one struct
/// is what stops them drifting into three copies of the same function.
#[derive(Debug, Clone, Copy)]
struct Mode {
    /// Whether a failing report should set a non-zero exit code.
    enforce: bool,
    /// Passed through to [`CheckOptions::expect_binding`].
    expect_binding: bool,
}

impl Mode {
    /// `status`: describe the repository, never fail.
    const REPORT: Self = Self {
        enforce: false,
        expect_binding: true,
    };
    /// `check`: the gate on the machine that owns the binding.
    const LOCAL_GATE: Self = Self {
        enforce: true,
        expect_binding: true,
    };
    /// `verify`: the gate in a pipeline, where there is no binding to find.
    const PIPELINE_GATE: Self = Self {
        enforce: true,
        expect_binding: false,
    };
}

fn inspect(
    args: InspectArgs,
    store: &ConfigStore,
    runner: &dyn Runner,
    mode: Mode,
) -> Result<u8, GitBoundError> {
    let config = store.load()?;
    // A pre-commit hook runs on every commit, so it stays off the network
    // whether or not the user asked; `--offline` is the same decision made
    // explicitly.
    let network = !args.offline && !matches!(args.hook, Some(HookMode::PreCommit));
    let repository = args
        .repo
        .clone()
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    let mut report = check::inspect_at(
        runner,
        &config,
        &repository,
        CheckOptions {
            remote_name: &args.remote,
            network,
            expect_binding: mode.expect_binding,
        },
    )?;

    // `report.repository` is the resolved worktree root, which is where a
    // committed policy file lives — not necessarily the directory the user
    // pointed at.
    let root = std::path::PathBuf::from(&report.repository);
    let policy = resolve_policy(&root, args.policy.as_deref(), args.no_policy)?;
    if args.require_policy {
        report.extend(policy_presence(policy.as_ref()));
    }
    if let Some(policy) = &policy {
        report.extend(policy.evaluate(&report));
    }
    if args.enforce_signing {
        report.extend(signing_enforcement(&report));
    }

    emit(&report, args.json, args.format, &args.output)?;
    if mode.enforce && !report.enforceable() {
        Ok(1)
    } else {
        Ok(0)
    }
}

fn audit(args: AuditArgs, store: &ConfigStore, runner: &dyn Runner) -> Result<u8, GitBoundError> {
    let config = store.load()?;
    let git = git_for(runner, args.repo.as_deref());
    let root = git.ensure_repo()?;

    // The bound profile, when there is one, is only used to treat the local
    // developer's own address as acceptable. A missing binding is normal in CI.
    let profile = git
        .binding_profile(false)?
        .and_then(|name| config.profiles.get(&name).cloned());

    let policy = resolve_policy(&root, args.policy.as_deref(), args.no_policy)?;
    let mut report = crate::audit::audit(
        runner,
        &root,
        policy.as_ref(),
        profile.as_ref(),
        &crate::audit::AuditOptions {
            range: &args.range,
            max_commits: args.max_commits,
            require_signature: args.enforce_signing,
        },
    )?;
    if args.require_policy {
        report.extend(policy_presence(policy.as_ref()));
    }

    emit(&report, args.json, args.format, &args.output)?;
    if report.enforceable() { Ok(0) } else { Ok(1) }
}

/// Which policy applies: an explicitly named file, the repository's committed
/// one, or none. `--no-policy` wins over both.
fn resolve_policy(
    root: &std::path::Path,
    named: Option<&std::path::Path>,
    disabled: bool,
) -> Result<Option<Policy>, GitBoundError> {
    if disabled {
        return Ok(None);
    }
    match named {
        Some(path) => Policy::load(path).map(Some),
        None => Policy::discover(root),
    }
}

/// `--require-policy` makes the absence of a policy a finding.
///
/// A repository that commits no `.gitbound.toml` has no rule to break, so
/// without this the gate passes having checked nothing — and a pull request
/// that deletes the file silently turns the gate off. An empty policy counts as
/// absent: a file that declares no rules constrains nothing.
fn policy_presence(policy: Option<&Policy>) -> Vec<CheckItem> {
    let (status, message) = match policy {
        Some(policy) if !policy.is_empty() => (
            CheckStatus::Ok,
            "the repository declares a policy".to_string(),
        ),
        Some(_) => (
            CheckStatus::Failure,
            format!(
                "{} declares no rules, so it constrains nothing",
                POLICY_FILE
            ),
        ),
        None => (
            CheckStatus::Failure,
            format!("--require-policy was given but the repository commits no {POLICY_FILE}"),
        ),
    };
    vec![CheckItem {
        id: "policy_present".into(),
        status,
        expected: Some(POLICY_FILE.into()),
        actual: None,
        message,
    }]
}

/// `--enforce-signing` turns "signing is not required by this profile" into a
/// failure without changing the profile, which is what a pipeline wants when
/// the repository's rule is stricter than any one developer's setup.
fn signing_enforcement(report: &CheckReport) -> Vec<CheckItem> {
    let actual = report
        .checks
        .iter()
        .find(|check| check.id == "commit_signing")
        .and_then(|check| check.actual.clone());
    if actual.as_deref() == Some("true") {
        return Vec::new();
    }
    vec![CheckItem {
        id: "commit_signing".into(),
        status: CheckStatus::Failure,
        expected: Some("true".into()),
        actual,
        message: "commit signing is required by --enforce-signing but is not enabled".into(),
    }]
}

/// Render a report and send it everywhere it was asked to go: stdout in the
/// primary format, plus any number of files, each of which may ask for its own
/// format. One inspection, several consumers — which is what a pipeline needs,
/// since re-running the checks to get a second format would double the git and
/// network work and could legitimately produce a different answer.
fn emit(
    report: &CheckReport,
    json: bool,
    format: Option<ReportFormat>,
    outputs: &[String],
) -> Result<(), GitBoundError> {
    let primary = match (json, format) {
        (true, _) => ReportFormat::Json,
        (false, Some(format)) => format,
        (false, None) => ReportFormat::Auto,
    }
    .resolve();

    let rendered = report::render(report, primary);
    print!("{rendered}");
    if !rendered.ends_with('\n') {
        println!();
    }
    if primary == ReportFormat::Github {
        report::write_step_summary(report);
    }

    for entry in outputs {
        let (requested, path) = report::parse_output(entry);
        let format = requested.unwrap_or(primary).resolve();
        let body = if format == primary {
            rendered.clone()
        } else {
            report::render(report, format)
        };
        std::fs::write(&path, body).map_err(|error| {
            GitBoundError::usage(format!("cannot write {}: {error}", path.display()))
        })?;
    }
    Ok(())
}

fn ci(command: CiCommand, store: &ConfigStore, runner: &dyn Runner) -> Result<u8, GitBoundError> {
    let CiCommand::Status { limit, json, repo } = command;
    let service = GitBoundService::new(store.clone(), runner);
    let repository = repo.unwrap_or_else(|| std::path::PathBuf::from("."));
    let status = service.repository_ci_status(&repository, limit)?;

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&status).map_err(json_error)?
        );
        return Ok(0);
    }

    if !status.available {
        println!(
            "CI status unavailable: {}",
            status.detail.as_deref().unwrap_or("no detail reported")
        );
        // Not an error. A repository without workflows, or a machine without
        // GitHub CLI, is a perfectly ordinary state.
        return Ok(0);
    }
    if status.runs.is_empty() {
        println!("No workflow runs found.");
        return Ok(0);
    }
    for run in &status.runs {
        println!(
            "{:<10} {:<24} {:<18} {}",
            format!("{:?}", run.conclusion).to_lowercase(),
            truncate(&run.name, 24),
            truncate(&run.branch, 18),
            run.title
        );
    }
    Ok(0)
}

/// Keep a column aligned without cutting a character in half.
fn truncate(value: &str, width: usize) -> String {
    if value.chars().count() <= width {
        return value.to_string();
    }
    let kept: String = value.chars().take(width.saturating_sub(1)).collect();
    format!("{kept}…")
}

fn hooks(command: HooksCommand, runner: &dyn Runner) -> Result<u8, GitBoundError> {
    // `--repo` for parity with `bind`, `unbind` and `status`, all of which
    // already accept it. It is also the path the desktop app takes, which never
    // has the repository as its working directory.
    let manager = |args: &crate::cli::RepositoryArgs| match &args.repo {
        Some(path) => HookManager::at(runner, path),
        None => HookManager::new(runner),
    };
    match &command {
        HooksCommand::Install(args) => {
            manager(args).install()?;
            println!("GitBound pre-commit and pre-push hooks installed.");
        }
        HooksCommand::Status(args) => println!("{}", manager(args).status()?),
        HooksCommand::Uninstall(args) => {
            manager(args).uninstall()?;
            println!("GitBound hooks removed.");
        }
    }
    Ok(0)
}

fn doctor(args: DoctorArgs, store: &ConfigStore, runner: &dyn Runner) -> Result<u8, GitBoundError> {
    let service = GitBoundService::new(store.clone(), runner);
    let report = service.doctor()?;
    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&report).map_err(json_error)?
        );
        return Ok(if report.healthy { 0 } else { 3 });
    }
    let config = store.load()?;
    println!(
        "Config: OK ({}, {} profiles)",
        store.path().display(),
        config.profiles.len()
    );
    let mut unavailable = false;
    for (program, args) in [
        ("git", vec!["--version"]),
        ("gh", vec!["--version"]),
        ("ssh", vec!["-V"]),
    ] {
        match runner.run(program, &os_args(&args), Duration::from_secs(10)) {
            // Exiting is not the same as working: see `GitBoundService::doctor`.
            Ok(output) if output.success() => println!("{program}: OK"),
            Ok(output) => {
                unavailable = true;
                match output.code {
                    Some(code) => println!("{program}: unavailable (exited {code})"),
                    None => println!("{program}: unavailable (timed out)"),
                }
            }
            Err(error) => {
                unavailable = true;
                println!("{program}: unavailable ({error})");
            }
        }
    }
    for (name, profile) in &config.profiles {
        if let Err(error) = profile.validate_local_resources() {
            unavailable = true;
            println!("profile {name}: unavailable ({error})");
        }
    }
    println!(
        "HTTPS profiles require GitHub CLI's credential helper. Run 'gh auth setup-git --hostname <host>' if status reports it missing."
    );
    Ok(if unavailable { 3 } else { 0 })
}

fn ssh(command: SshCommand, store: &ConfigStore, runner: &dyn Runner) -> Result<u8, GitBoundError> {
    match command {
        SshCommand::Test { profile, json } => {
            let report = GitBoundService::new(store.clone(), runner).ssh_test(&profile)?;
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&report).map_err(json_error)?
                );
            } else {
                println!("{}", report.message);
            }
            Ok(
                if matches!(report.status, crate::api::SshTestStatus::Verified) {
                    0
                } else {
                    3
                },
            )
        }
    }
}

fn git_for<'a>(runner: &'a dyn Runner, repository: Option<&std::path::Path>) -> Git<'a> {
    repository.map_or_else(|| Git::new(runner), |path| Git::at(runner, path))
}

fn prompt_error(error: dialoguer::Error) -> GitBoundError {
    GitBoundError::dependency(format!("interactive prompt failed: {error}"))
}
fn json_error(error: serde_json::Error) -> GitBoundError {
    GitBoundError::dependency(format!("could not serialize JSON: {error}"))
}
