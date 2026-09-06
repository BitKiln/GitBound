use crate::{config::SigningFormat, report::ReportFormat};
use clap::{Args, Parser, Subcommand, ValueEnum};
use clap_complete::Shell;
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(
    name = "gitbound",
    version,
    about = "Manage and verify local GitHub identities"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    Profile {
        #[command(subcommand)]
        command: ProfileCommand,
    },
    Use {
        profile: String,
    },
    Clone(CloneArgs),
    Bind(BindArgs),
    Unbind(RepositoryArgs),
    Status(InspectArgs),
    Check(InspectArgs),
    /// Run the identity checks with defaults suited to a pipeline.
    ///
    /// The difference from `check` is what it does about a binding. A binding
    /// lives in the repository a developer works in; a pipeline checks out a
    /// fresh copy and has no user configuration at all, so `verify` reports an
    /// absent binding as `unverified` and judges the repository against its
    /// committed `.gitbound.toml` instead. `check` continues to treat an
    /// unbound repository as a failure, because on the machine that owns the
    /// binding it is one.
    ///
    /// Add `--require-policy` when the pipeline should also refuse a repository
    /// that commits no policy, since one that declares no rules has nothing to
    /// fail against.
    Verify(InspectArgs),
    /// Audit the authorship of a revision range.
    Audit(AuditArgs),
    Hooks {
        #[command(subcommand)]
        command: HooksCommand,
    },
    Directory {
        #[command(subcommand)]
        command: DirectoryCommand,
    },
    Doctor(DoctorArgs),
    /// Inspect the repository's continuous integration.
    Ci {
        #[command(subcommand)]
        command: CiCommand,
    },
    Ssh {
        #[command(subcommand)]
        command: SshCommand,
    },
    Completions {
        shell: Shell,
    },
}

#[derive(Debug, Args)]
pub struct CloneArgs {
    pub profile: String,
    pub repository: String,
    pub directory: Option<PathBuf>,
    #[arg(long, value_enum)]
    pub protocol: Option<CloneProtocol>,
    #[arg(long, default_value = "origin")]
    pub remote: String,
    #[arg(long)]
    pub no_switch: bool,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum CloneProtocol {
    Ssh,
    Https,
}

#[derive(Debug, Subcommand)]
pub enum ProfileCommand {
    Add(ProfileMutationArgs),
    Import(ProfileImportArgs),
    Edit(ProfileMutationArgs),
    List {
        #[arg(long)]
        json: bool,
    },
    Show {
        name: String,
        #[arg(long)]
        json: bool,
    },
    Remove {
        name: String,
        #[arg(long)]
        yes: bool,
    },
    Rename {
        old_name: String,
        new_name: String,
    },
}

#[derive(Debug, Args)]
pub struct ProfileImportArgs {
    pub name: String,
    #[arg(long, default_value = "origin")]
    pub remote: String,
    #[arg(long)]
    pub no_owner: bool,
    #[arg(long)]
    pub repo: Option<PathBuf>,
}

#[derive(Debug, Subcommand)]
pub enum DirectoryCommand {
    Add {
        profile: String,
        path: PathBuf,
    },
    List {
        #[arg(long)]
        json: bool,
    },
    Sync {
        profile: Option<String>,
    },
    Remove {
        path: PathBuf,
    },
}

#[derive(Debug, Args)]
pub struct ProfileMutationArgs {
    pub name: String,
    #[arg(long)]
    pub github_user: Option<String>,
    #[arg(long)]
    pub git_name: Option<String>,
    #[arg(long)]
    pub git_email: Option<String>,
    #[arg(long)]
    pub hostname: Option<String>,
    #[arg(long, conflicts_with = "clear_ssh_host")]
    pub ssh_host: Option<String>,
    #[arg(long, conflicts_with = "ssh_host")]
    pub clear_ssh_host: bool,
    #[arg(long)]
    pub ssh_key: Option<PathBuf>,
    #[arg(long = "allowed-owner")]
    pub allowed_owners: Vec<String>,
    #[arg(long)]
    pub clear_ssh_key: bool,
    #[arg(long)]
    pub clear_allowed_owners: bool,
    #[arg(long)]
    pub signing_key: Option<String>,
    #[arg(long, value_enum)]
    pub signing_format: Option<SigningFormat>,
    #[arg(long, conflicts_with = "no_require_signing")]
    pub require_signing: bool,
    #[arg(long, conflicts_with = "require_signing")]
    pub no_require_signing: bool,
    #[arg(long)]
    pub clear_signing_key: bool,
}

#[derive(Debug, Args)]
pub struct BindArgs {
    pub profile: String,
    #[arg(long, default_value = "origin")]
    pub remote: String,
    #[arg(long, action = clap::ArgAction::SetTrue)]
    pub switch: bool,
    #[arg(long)]
    pub force: bool,
    #[arg(long)]
    pub repo: Option<PathBuf>,
}

#[derive(Debug, Args)]
pub struct InspectArgs {
    #[arg(long, default_value = "origin")]
    pub remote: String,
    /// Shorthand for `--format json`, kept so existing scripts keep working.
    #[arg(long, conflicts_with = "format")]
    pub json: bool,
    /// How to render the report.
    #[arg(long, value_enum)]
    pub format: Option<ReportFormat>,
    /// Also write the report to a file. Repeatable, and each entry may name its
    /// own format: `--output sarif:out.sarif --output json:report.json`. A bare
    /// path uses whatever format stdout is using.
    #[arg(long, value_name = "[FORMAT:]PATH")]
    pub output: Vec<String>,
    /// Read repository policy from this file instead of looking for
    /// `.gitbound.toml` at the repository root. A named file that is missing
    /// is an error.
    #[arg(long, value_name = "PATH")]
    pub policy: Option<PathBuf>,
    /// Ignore any `.gitbound.toml` the repository commits.
    #[arg(long, conflicts_with = "policy")]
    pub no_policy: bool,
    /// Fail when the repository declares no policy at all.
    ///
    /// Without this, a repository that commits no `.gitbound.toml` has nothing
    /// to be judged against, and `verify` passes because there was no rule to
    /// break. In a pipeline that is a gate reporting success while checking
    /// nothing, and deleting the policy file is enough to turn it off.
    #[arg(long, conflicts_with = "no_policy")]
    pub require_policy: bool,
    /// Skip the checks that need the network.
    ///
    /// `ssh_identity` asks GitHub who the key authenticates as, and
    /// `github_cli` validates the stored token against the API. Neither can
    /// answer on a plane or in a sandboxed runner, and an unanswered check is
    /// fatal by design — so without this the only way past it is to stop
    /// running the gate. Everything local, the credential-helper check
    /// included, still runs.
    #[arg(long)]
    pub offline: bool,
    /// Treat an unsigned commit configuration as a failure even when the
    /// profile does not require signing.
    #[arg(long)]
    pub enforce_signing: bool,
    #[arg(long, value_enum, hide = true)]
    pub hook: Option<HookMode>,
    #[arg(long)]
    pub repo: Option<PathBuf>,
}

#[derive(Debug, Args)]
pub struct AuditArgs {
    /// A Git revision range, for example `origin/main..HEAD`.
    #[arg(long, default_value = "HEAD")]
    pub range: String,
    /// Stop after this many commits.
    #[arg(long, default_value_t = crate::audit::DEFAULT_MAX_COMMITS)]
    pub max_commits: usize,
    #[arg(long, conflicts_with = "format")]
    pub json: bool,
    #[arg(long, value_enum)]
    pub format: Option<ReportFormat>,
    /// Also write the report to a file. See `verify --help`.
    #[arg(long, value_name = "[FORMAT:]PATH")]
    pub output: Vec<String>,
    #[arg(long, value_name = "PATH")]
    pub policy: Option<PathBuf>,
    #[arg(long, conflicts_with = "policy")]
    pub no_policy: bool,
    /// Fail when the repository declares no policy at all. See `verify --help`.
    ///
    /// `audit` accepts this for the same reason it accepts `--policy`: without
    /// a policy there is nothing to judge the range against, so it reports
    /// success having checked nothing.
    #[arg(long, conflicts_with = "no_policy")]
    pub require_policy: bool,
    /// Require every commit in the range to carry a verified signature.
    #[arg(long)]
    pub enforce_signing: bool,
    #[arg(long)]
    pub repo: Option<PathBuf>,
}

#[derive(Debug, Args)]
pub struct RepositoryArgs {
    #[arg(long)]
    pub repo: Option<PathBuf>,
}

#[derive(Debug, Args)]
pub struct DoctorArgs {
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Subcommand)]
pub enum CiCommand {
    /// Show recent GitHub Actions runs for a repository.
    Status {
        #[arg(long, default_value_t = 5)]
        limit: usize,
        #[arg(long)]
        json: bool,
        #[arg(long)]
        repo: Option<PathBuf>,
    },
}

#[derive(Debug, Subcommand)]
pub enum SshCommand {
    Test {
        profile: String,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum HookMode {
    PreCommit,
    PrePush,
}

#[derive(Debug, Subcommand)]
pub enum HooksCommand {
    Install(RepositoryArgs),
    Status(RepositoryArgs),
    Uninstall(RepositoryArgs),
}
