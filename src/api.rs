use crate::{
    check::CheckReport,
    config::Profile,
    error::{ErrorKind, GitBoundError},
    remote::RemoteInfo,
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NamedProfile {
    pub name: String,
    pub profile: Profile,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProfileDraft {
    pub repository: PathBuf,
    pub github_user: Option<String>,
    pub git_name: Option<String>,
    pub git_email: Option<String>,
    pub hostname: String,
    pub ssh_host: Option<String>,
    pub allowed_owners: Vec<String>,
    pub signing_key: Option<String>,
    pub signing_format: crate::config::SigningFormat,
    pub require_signing: bool,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RepositoryLocalStatus {
    Bound,
    Unbound,
    Drifted,
    MissingProfile,
    Unavailable,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RepositorySummary {
    pub path: PathBuf,
    pub name: String,
    pub bound_profile: Option<String>,
    pub git_name: Option<String>,
    pub git_email: Option<String>,
    pub remote: Option<RemoteInfo>,
    pub status: RepositoryLocalStatus,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RepositoryScanEvent {
    Started {
        roots: usize,
    },
    RootStarted {
        root: PathBuf,
    },
    RepositoryFound {
        repository: Box<RepositorySummary>,
    },
    RootUnavailable {
        root: PathBuf,
        message: String,
    },
    Finished {
        repositories: usize,
        cancelled: bool,
    },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DependencyState {
    Ok,
    Warning,
    Unavailable,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DependencyStatus {
    pub name: String,
    pub state: DependencyState,
    pub detail: String,
    pub remediation: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DoctorReport {
    pub config_path: PathBuf,
    pub schema_version: u32,
    pub profile_count: usize,
    pub dependencies: Vec<DependencyStatus>,
    pub profile_issues: Vec<String>,
    pub healthy: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SshTestStatus {
    Verified,
    Rejected,
    Unavailable,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SshTestReport {
    pub profile: String,
    pub expected_user: String,
    pub actual_user: Option<String>,
    pub hostname: String,
    pub key: Option<PathBuf>,
    pub status: SshTestStatus,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ApiError {
    pub kind: String,
    pub message: String,
    pub exit_code: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
}

impl From<GitBoundError> for ApiError {
    fn from(error: GitBoundError) -> Self {
        let kind = match error.kind() {
            ErrorKind::Check => "check",
            ErrorKind::Usage => "usage",
            ErrorKind::Dependency => "dependency",
        };
        Self {
            kind: kind.into(),
            message: error.to_string(),
            exit_code: error.exit_code(),
            field: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepositoryStatus {
    pub report: CheckReport,
    pub network_checked: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CiConclusion {
    Success,
    Failure,
    Cancelled,
    Skipped,
    Running,
    Unknown,
}

impl CiConclusion {
    /// GitHub reports a run as a `status` while it is going and a `conclusion`
    /// once it is done, and the conclusion is empty until then. Collapsing both
    /// into one value here means the UI never has to decide which field to read.
    pub fn from_gh(status: &str, conclusion: &str) -> Self {
        if !matches!(status, "completed") {
            return Self::Running;
        }
        match conclusion {
            "success" => Self::Success,
            "failure" | "timed_out" | "startup_failure" => Self::Failure,
            "cancelled" => Self::Cancelled,
            "skipped" | "neutral" => Self::Skipped,
            _ => Self::Unknown,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkflowRunSummary {
    pub id: u64,
    pub name: String,
    pub title: String,
    pub branch: String,
    pub sha: String,
    pub created_at: String,
    pub url: String,
    pub conclusion: CiConclusion,
}

/// Recent CI runs for one repository.
///
/// `available: false` is the normal, expected answer when GitHub CLI is absent,
/// unauthenticated, or the repository has no workflows. It carries `detail` so
/// the panel can explain itself rather than showing an empty list, and it is
/// deliberately not an error: a missing CI view must never interrupt what the
/// user was doing.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RepositoryCiStatus {
    pub runs: Vec<WorkflowRunSummary>,
    pub available: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

impl RepositoryCiStatus {
    pub fn unavailable(detail: impl Into<String>) -> Self {
        Self {
            runs: Vec::new(),
            available: false,
            detail: Some(detail.into()),
        }
    }
}
