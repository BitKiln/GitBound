use crate::{
    api::{CiConclusion, RepositoryCiStatus, WorkflowRunSummary},
    error::GitBoundError,
    process::{Runner, os_args},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{ffi::OsString, path::Path, time::Duration};

const TIMEOUT: Duration = Duration::from_secs(20);

pub struct GitHub<'a> {
    runner: &'a dyn Runner,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct Account {
    pub login: String,
    pub active: bool,
    pub valid: bool,
}

/// The shape `gh run list --json` emits. Kept private and separate from the DTO
/// the UI receives so a change in the CLI's field names is a compile error here
/// rather than a silently empty panel.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawRun {
    #[serde(default)]
    database_id: u64,
    #[serde(default)]
    name: String,
    #[serde(default)]
    display_title: String,
    #[serde(default)]
    status: String,
    #[serde(default)]
    conclusion: String,
    #[serde(default)]
    head_branch: String,
    #[serde(default)]
    head_sha: String,
    #[serde(default)]
    created_at: String,
    #[serde(default)]
    url: String,
}

impl From<RawRun> for WorkflowRunSummary {
    fn from(raw: RawRun) -> Self {
        Self {
            id: raw.database_id,
            name: raw.name,
            title: raw.display_title,
            branch: raw.head_branch,
            sha: raw.head_sha,
            created_at: raw.created_at,
            url: raw.url,
            conclusion: CiConclusion::from_gh(&raw.status, &raw.conclusion),
        }
    }
}

impl<'a> GitHub<'a> {
    pub fn new(runner: &'a dyn Runner) -> Self {
        Self { runner }
    }

    pub fn accounts(&self, hostname: &str) -> Result<Vec<Account>, GitBoundError> {
        let args = ["auth", "status", "--hostname", hostname, "--json", "hosts"];
        let output = self.runner.run("gh", &os_args(&args), TIMEOUT)?;
        if output.code.is_none() {
            return Err(GitBoundError::dependency("GitHub CLI status timed out"));
        }
        if !output.success() {
            // `gh auth status` exits non-zero when no account is authenticated
            // for the host. Report its own diagnostic instead of letting the
            // empty body surface as a JSON parse failure.
            let detail = output.stderr.trim();
            return Err(GitBoundError::dependency(if detail.is_empty() {
                format!("GitHub CLI reported no authenticated account on {hostname}")
            } else {
                format!("GitHub CLI status failed for {hostname}: {detail}")
            }));
        }
        let value: Value = serde_json::from_str(&output.stdout).map_err(|e| {
            GitBoundError::dependency(format!("GitHub CLI returned invalid JSON: {e}"))
        })?;
        let mut accounts = Vec::new();
        collect_accounts(&value, 0, &mut accounts);
        accounts.sort();
        accounts.dedup();
        Ok(accounts)
    }

    /// Recent GitHub Actions runs for the repository at `repository`.
    ///
    /// This never reports an error for an absent or unauthenticated `gh`: a
    /// missing CI view is a degraded panel, not a failed operation, and the
    /// caller is a UI that must keep working offline. Genuine transport
    /// failures come back the same way, as `available: false` with the detail
    /// attached, so nothing here can turn a network hiccup into a dialog.
    pub fn workflow_runs(
        &self,
        repository: &Path,
        limit: usize,
    ) -> Result<RepositoryCiStatus, GitBoundError> {
        let limit = limit.clamp(1, 50);
        let args = os_args(&[
            "run",
            "list",
            "--limit",
            &limit.to_string(),
            "--json",
            "databaseId,name,displayTitle,status,conclusion,headBranch,headSha,createdAt,url",
        ]);
        let output = match self.runner.run_in("gh", &args, repository, TIMEOUT) {
            Ok(output) => output,
            Err(error) => return Ok(RepositoryCiStatus::unavailable(error.to_string())),
        };
        if output.code.is_none() {
            return Ok(RepositoryCiStatus::unavailable(
                "GitHub CLI timed out while listing workflow runs",
            ));
        }
        if !output.success() {
            let detail = output.stderr.trim();
            return Ok(RepositoryCiStatus::unavailable(if detail.is_empty() {
                "GitHub CLI could not list workflow runs for this repository".to_string()
            } else {
                detail.to_string()
            }));
        }
        let runs: Vec<RawRun> = match serde_json::from_str(&output.stdout) {
            Ok(runs) => runs,
            Err(error) => {
                return Ok(RepositoryCiStatus::unavailable(format!(
                    "GitHub CLI returned workflow runs this build could not read: {error}"
                )));
            }
        };
        Ok(RepositoryCiStatus {
            runs: runs.into_iter().map(WorkflowRunSummary::from).collect(),
            available: true,
            detail: None,
        })
    }

    pub fn active_account(&self, hostname: &str) -> Result<Option<String>, GitBoundError> {
        let active = self
            .accounts(hostname)?
            .into_iter()
            .find(|account| account.active);
        match active {
            Some(account) if account.valid => Ok(Some(account.login)),
            Some(account) => Err(GitBoundError::dependency(format!(
                "GitHub CLI could not validate the active account '{}' on {hostname}",
                account.login
            ))),
            None => Ok(None),
        }
    }

    pub fn is_authenticated(&self, hostname: &str, user: &str) -> Result<bool, GitBoundError> {
        Ok(self
            .accounts(hostname)?
            .iter()
            .any(|account| account.valid && account.login.eq_ignore_ascii_case(user)))
    }

    pub fn switch(&self, hostname: &str, user: &str) -> Result<(), GitBoundError> {
        if !self.is_authenticated(hostname, user)? {
            return Err(GitBoundError::usage(format!(
                "GitHub CLI is not authenticated as {user} on {hostname}"
            )));
        }
        let args = vec![
            OsString::from("auth"),
            OsString::from("switch"),
            OsString::from("--hostname"),
            OsString::from(hostname),
            OsString::from("--user"),
            OsString::from(user),
        ];
        let output = self.runner.run("gh", &args, TIMEOUT)?;
        if !output.success() {
            return Err(GitBoundError::dependency(format!(
                "GitHub CLI account switch failed: {}",
                output.stderr.trim()
            )));
        }
        let active = self.active_account(hostname)?;
        if active
            .as_deref()
            .is_some_and(|value| value.eq_ignore_ascii_case(user))
        {
            Ok(())
        } else {
            Err(GitBoundError::dependency(format!(
                "GitHub CLI did not activate {user} after switching"
            )))
        }
    }
}

/// Bound on how deep `gh`'s JSON is walked. The real shape nests three levels;
/// the cap keeps a malformed or hostile document from overflowing the stack.
const MAX_JSON_DEPTH: usize = 32;

fn collect_accounts(value: &Value, depth: usize, output: &mut Vec<Account>) {
    if depth >= MAX_JSON_DEPTH {
        return;
    }
    match value {
        Value::Object(map) => {
            if let Some(login) = map.get("login").and_then(Value::as_str) {
                let active = map.get("active").and_then(Value::as_bool).unwrap_or(false);
                let valid = map
                    .get("state")
                    .and_then(Value::as_str)
                    .is_none_or(|state| state.eq_ignore_ascii_case("success"));
                output.push(Account {
                    login: login.to_string(),
                    active,
                    valid,
                });
            }
            for child in map.values() {
                collect_accounts(child, depth + 1, output);
            }
        }
        Value::Array(values) => {
            for child in values {
                collect_accounts(child, depth + 1, output);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::process::ProcessOutput;

    struct FakeRunner {
        output: ProcessOutput,
    }

    impl Runner for FakeRunner {
        fn run(
            &self,
            _: &str,
            _: &[OsString],
            _: Duration,
        ) -> Result<ProcessOutput, GitBoundError> {
            Ok(self.output.clone())
        }
    }

    #[test]
    fn extracts_accounts_from_gh_shape() {
        let value: Value = serde_json::json!({"hosts":{"github.com":[{"login":"alice","active":true,"state":"success"},{"login":"bob","active":false,"state":"error"}]}});
        let mut accounts = vec![];
        collect_accounts(&value, 0, &mut accounts);
        assert_eq!(
            accounts,
            vec![
                Account {
                    login: "alice".into(),
                    active: true,
                    valid: true,
                },
                Account {
                    login: "bob".into(),
                    active: false,
                    valid: false,
                }
            ]
        );
    }

    #[test]
    fn rejects_malformed_and_offline_status() {
        let malformed = FakeRunner {
            output: ProcessOutput {
                code: Some(0),
                stdout: "not-json".into(),
                stderr: String::new(),
            },
        };
        assert!(
            GitHub::new(&malformed)
                .active_account("github.com")
                .is_err()
        );

        let offline = FakeRunner {
            output: ProcessOutput {
                code: Some(0),
                stdout: serde_json::json!({"hosts":{"github.com":[{"login":"alice","active":true,"state":"error"}]}}).to_string(),
                stderr: String::new(),
            },
        };
        assert!(GitHub::new(&offline).active_account("github.com").is_err());
    }
}
