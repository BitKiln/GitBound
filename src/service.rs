use crate::{
    api::{
        DependencyState, DependencyStatus, DoctorReport, NamedProfile, ProfileDraft,
        RepositoryCiStatus, RepositoryScanEvent, RepositoryStatus, SshTestReport, SshTestStatus,
    },
    check::{self, CheckOptions, CheckReport},
    clone_repo,
    config::{Config, ConfigStore, Profile, SigningFormat, validate_profile_name},
    directory,
    error::GitBoundError,
    git::Git,
    github::{Account, GitHub},
    hooks::{HookManager, HookState},
    policy::Policy,
    process::{Runner, os_args},
    remote::{RemoteProtocol, parse_repository},
    repository,
    ssh::{self, SshIdentity},
};
use std::{
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
    time::Duration,
};

pub struct GitBoundService<'a> {
    store: ConfigStore,
    runner: &'a dyn Runner,
}

impl<'a> GitBoundService<'a> {
    pub fn discover(runner: &'a dyn Runner) -> Result<Self, GitBoundError> {
        Ok(Self::new(ConfigStore::discover()?, runner))
    }

    pub fn new(store: ConfigStore, runner: &'a dyn Runner) -> Self {
        Self { store, runner }
    }

    pub fn config(&self) -> Result<Config, GitBoundError> {
        self.store.load()
    }

    pub fn config_path(&self) -> &Path {
        self.store.path()
    }

    pub fn list_profiles(&self) -> Result<Vec<NamedProfile>, GitBoundError> {
        Ok(self
            .store
            .load()?
            .profiles
            .into_iter()
            .map(|(name, profile)| NamedProfile { name, profile })
            .collect())
    }

    pub fn get_profile(&self, name: &str) -> Result<NamedProfile, GitBoundError> {
        let profile = self
            .store
            .load()?
            .profiles
            .get(name)
            .cloned()
            .ok_or_else(|| GitBoundError::usage(format!("profile '{name}' does not exist")))?;
        Ok(NamedProfile {
            name: name.into(),
            profile,
        })
    }

    pub fn create_profile(
        &self,
        name: &str,
        profile: Profile,
    ) -> Result<NamedProfile, GitBoundError> {
        validate_profile_name(name)?;
        profile.validate()?;
        profile.validate_local_resources()?;
        self.store.update(|config| {
            if config.profiles.contains_key(name) {
                return Err(GitBoundError::usage(format!(
                    "profile '{name}' already exists"
                )));
            }
            config.profiles.insert(name.into(), profile.clone());
            Ok(())
        })?;
        Ok(NamedProfile {
            name: name.into(),
            profile,
        })
    }

    pub fn update_profile(
        &self,
        name: &str,
        profile: Profile,
    ) -> Result<NamedProfile, GitBoundError> {
        validate_profile_name(name)?;
        profile.validate()?;
        profile.validate_local_resources()?;
        self.store.update(|config| {
            if !config.profiles.contains_key(name) {
                return Err(GitBoundError::usage(format!(
                    "profile '{name}' does not exist"
                )));
            }
            config.profiles.insert(name.into(), profile.clone());
            Ok(())
        })?;
        directory::sync_profile(name, &self.store)?;
        Ok(NamedProfile {
            name: name.into(),
            profile,
        })
    }

    pub fn remove_profile(&self, name: &str) -> Result<(), GitBoundError> {
        let config = self.store.load()?;
        if !config.profiles.contains_key(name) {
            return Err(GitBoundError::usage(format!(
                "profile '{name}' does not exist"
            )));
        }
        if config.directories.iter().any(|rule| rule.profile == name) {
            return Err(GitBoundError::usage(format!(
                "profile '{name}' has directory rules; remove them first"
            )));
        }
        self.store.update(|config| {
            config.profiles.remove(name);
            Ok(())
        })
    }

    pub fn rename_profile(
        &self,
        old_name: &str,
        new_name: &str,
    ) -> Result<NamedProfile, GitBoundError> {
        validate_profile_name(new_name)?;
        if old_name == new_name {
            let profile = self.get_profile(old_name)?.profile;
            return Ok(NamedProfile {
                name: new_name.into(),
                profile,
            });
        }
        let profile = self.store.update(|config| {
            if !config.profiles.contains_key(old_name) {
                return Err(GitBoundError::usage(format!(
                    "profile '{old_name}' does not exist"
                )));
            }
            if config.profiles.contains_key(new_name) {
                return Err(GitBoundError::usage(format!(
                    "profile '{new_name}' already exists"
                )));
            }
            let profile = config
                .profiles
                .remove(old_name)
                .expect("old profile exists");
            config.profiles.insert(new_name.into(), profile.clone());
            for rule in &mut config.directories {
                if rule.profile == old_name {
                    rule.profile = new_name.into();
                }
            }
            Ok(profile)
        })?;
        directory::rename_profile(old_name, new_name, &self.store, self.runner)?;
        Ok(NamedProfile {
            name: new_name.into(),
            profile,
        })
    }

    pub fn import_preview(
        &self,
        repository: &Path,
        remote_name: &str,
    ) -> Result<ProfileDraft, GitBoundError> {
        let git = Git::at(self.runner, repository);
        let root = git.ensure_repo()?;
        let remote = git.remote(remote_name)?.ok_or_else(|| {
            GitBoundError::usage(format!("remote '{remote_name}' is not configured"))
        })?;
        let mut warnings = Vec::new();
        let github_user = match GitHub::new(self.runner).active_account(&remote.hostname) {
            Ok(user) => user,
            Err(error) => {
                warnings.push(format!(
                    "GitHub CLI identity could not be imported: {error}"
                ));
                None
            }
        };
        let signing_format = match git.get("gpg.format", false)?.as_deref() {
            Some("ssh") => SigningFormat::Ssh,
            _ => SigningFormat::Openpgp,
        };
        Ok(ProfileDraft {
            repository: root,
            github_user,
            git_name: git.get("user.name", false)?,
            git_email: git.get("user.email", false)?,
            hostname: remote.hostname,
            ssh_host: None,
            allowed_owners: vec![remote.owner],
            signing_key: git.get("user.signingKey", false)?,
            signing_format,
            require_signing: git
                .get("commit.gpgSign", false)?
                .is_some_and(|value| value.eq_ignore_ascii_case("true")),
            warnings,
        })
    }

    pub fn inspect_repository(
        &self,
        repository: &Path,
        remote_name: &str,
        network: bool,
    ) -> Result<RepositoryStatus, GitBoundError> {
        let config = self.store.load()?;
        let mut report = check::inspect_at(
            self.runner,
            &config,
            repository,
            CheckOptions {
                remote_name,
                network,
                // The desktop app runs on the machine that owns the binding, so
                // an unbound repository is something to surface, not to excuse.
                expect_binding: true,
            },
        )?;
        // The repository's own committed policy, which this did not apply — so
        // the app reported a repository as clean while `gitbound check` on the
        // same repository failed on the very rule the repository ships to be
        // judged by.
        Policy::apply_committed(&mut report)?;
        Ok(RepositoryStatus {
            report,
            network_checked: network,
        })
    }

    pub fn bind_repository(
        &self,
        repository: &Path,
        profile_name: &str,
        remote_name: &str,
        force: bool,
    ) -> Result<(), GitBoundError> {
        let config = self.store.load()?;
        let profile = config.profiles.get(profile_name).ok_or_else(|| {
            GitBoundError::usage(format!("profile '{profile_name}' does not exist"))
        })?;
        let git = Git::at(self.runner, repository);
        git.ensure_repo()?;
        let remote = git.remote(remote_name)?;
        let old_name = git.binding_profile(true)?;
        let old_profile = old_name.as_ref().and_then(|name| config.profiles.get(name));
        git.bind(profile_name, profile, remote.as_ref(), force, old_profile)
    }

    pub fn unbind_repository(&self, repository: &Path) -> Result<(), GitBoundError> {
        Git::at(self.runner, repository).unbind()
    }

    /// Clone a repository and bind it in one step, answering with the path it
    /// landed at.
    ///
    /// The binding is the point: a clone that succeeded and a bind that failed
    /// leaves a repository configured as whoever the machine is by default,
    /// which is the outcome this product exists to prevent — so a failed bind
    /// removes the clone rather than leaving it.
    pub fn clone_repository(
        &self,
        profile: &str,
        repository: &str,
        parent: &Path,
        protocol: Option<RemoteProtocol>,
    ) -> Result<PathBuf, GitBoundError> {
        // The caller picks a folder to clone *into*; the repository's own name
        // supplies the leaf, as it does on the command line. Parsing against
        // the profile's host is what makes a bare `owner/repo` resolve the same
        // way here as it does there, including on a GitHub Enterprise host.
        let hostname = self.get_profile(profile)?.profile.hostname;
        let destination = parent.join(parse_repository(repository, &hostname)?.repository);
        clone_repo::clone(
            &clone_repo::CloneRequest {
                profile,
                repository,
                directory: Some(&destination),
                protocol,
                remote: "origin",
                no_switch: false,
            },
            &self.store,
            self.runner,
        )
    }

    /// Who actually authored a range of commits, judged against the
    /// repository's committed policy.
    ///
    /// This is evidence the identity checks cannot give: they describe how the
    /// repository is configured *now*, and say nothing about the commits
    /// already in it. A repository can pass every check and still carry a
    /// commit authored under the wrong address last week.
    pub fn audit_repository(
        &self,
        repository: &Path,
        range: &str,
        max_commits: usize,
    ) -> Result<CheckReport, GitBoundError> {
        let config = self.store.load()?;
        let git = Git::at(self.runner, repository);
        let root = git.ensure_repo()?;
        // The bound profile only ever widens what is acceptable: it lets the
        // developer's own address through. A missing binding is not an error.
        let profile = git
            .binding_profile(false)?
            .and_then(|name| config.profiles.get(&name).cloned());
        let policy = Policy::discover(&root)?;
        crate::audit::audit(
            self.runner,
            &root,
            policy.as_ref(),
            profile.as_ref(),
            &crate::audit::AuditOptions {
                range,
                max_commits,
                require_signature: false,
            },
        )
    }

    /// Directory rules: a folder assigned to a profile, applied by Git's own
    /// `includeIf` before GitBound is in the picture at all. They were CLI-only,
    /// which left the desktop app unable to show a user why a repository they
    /// never bound already had an identity.
    pub fn directory_rules(&self) -> Result<Vec<directory::RuleView>, GitBoundError> {
        directory::rules(&self.store)
    }

    pub fn add_directory_rule(&self, profile: &str, path: &Path) -> Result<PathBuf, GitBoundError> {
        directory::add_rule(profile, path, &self.store, self.runner)
    }

    pub fn remove_directory_rule(&self, path: &Path) -> Result<PathBuf, GitBoundError> {
        directory::remove_rule(path, &self.store, self.runner)
    }

    /// The pre-commit and pre-push hooks are what stop a wrong-identity commit
    /// from being made at all, rather than reporting it afterwards. They were
    /// reachable only from the CLI, which left the desktop app presenting the
    /// whole product minus its enforcement.
    pub fn hook_state(&self, repository: &Path) -> Result<HookState, GitBoundError> {
        HookManager::at(self.runner, repository).state()
    }

    pub fn install_hooks(&self, repository: &Path) -> Result<HookState, GitBoundError> {
        let hooks = HookManager::at(self.runner, repository);
        hooks.install()?;
        hooks.state()
    }

    pub fn uninstall_hooks(&self, repository: &Path) -> Result<HookState, GitBoundError> {
        let hooks = HookManager::at(self.runner, repository);
        hooks.uninstall()?;
        hooks.state()
    }

    pub fn github_accounts(&self, hostname: &str) -> Result<Vec<Account>, GitBoundError> {
        GitHub::new(self.runner).accounts(hostname)
    }

    /// Recent GitHub Actions runs for a repository.
    ///
    /// This talks to the network, so it is only ever called in response to an
    /// explicit user action — there is no polling and no refresh on load. See
    /// PRODUCT.md: automatic network checks are a non-goal, and showing CI
    /// status must not quietly become one.
    pub fn repository_ci_status(
        &self,
        repository: &Path,
        limit: usize,
    ) -> Result<RepositoryCiStatus, GitBoundError> {
        // Confirm this is a worktree first, so a mistyped path produces a clear
        // error rather than gh's own confusing one.
        Git::at(self.runner, repository).ensure_repo()?;
        GitHub::new(self.runner).workflow_runs(repository, limit)
    }

    pub fn switch_github_account(&self, profile_name: &str) -> Result<(), GitBoundError> {
        let named = self.get_profile(profile_name)?;
        GitHub::new(self.runner).switch(&named.profile.hostname, &named.profile.github_user)
    }

    pub fn ssh_test(&self, profile_name: &str) -> Result<SshTestReport, GitBoundError> {
        let named = self.get_profile(profile_name)?;
        let key = named.profile.ssh_key.clone();
        let (status, actual_user, message) = match ssh::verify(self.runner, &named.profile)? {
            SshIdentity::Verified(user) => (
                SshTestStatus::Verified,
                Some(user.clone()),
                format!("SSH authenticates as {user}"),
            ),
            SshIdentity::Rejected(user) => (
                SshTestStatus::Rejected,
                Some(user.clone()),
                format!(
                    "SSH authenticates as {user}, not {}",
                    named.profile.github_user
                ),
            ),
            SshIdentity::Unavailable(reason) => (
                SshTestStatus::Unavailable,
                None,
                if reason.is_empty() {
                    "SSH identity could not be verified".into()
                } else {
                    reason
                },
            ),
        };
        let hostname = named.profile.ssh_host().to_string();
        Ok(SshTestReport {
            profile: named.name,
            expected_user: named.profile.github_user,
            actual_user,
            hostname,
            key,
            status,
            message,
        })
    }

    pub fn doctor(&self) -> Result<DoctorReport, GitBoundError> {
        let config = self.store.load()?;
        let mut dependencies = Vec::new();
        for (program, args, remediation) in [
            (
                "git",
                vec!["--version"],
                "Install Git and ensure it is on PATH.",
            ),
            (
                "gh",
                vec!["--version"],
                "Install GitHub CLI and authenticate the accounts used by your profiles.",
            ),
            (
                "ssh",
                vec!["-V"],
                "Install OpenSSH and ensure it is on PATH.",
            ),
        ] {
            match self
                .runner
                .run(program, &os_args(&args), Duration::from_secs(10))
            {
                // The test is whether the program ran *successfully*, not
                // merely whether it exited. A `gh` that exits 127 because its
                // own libraries are missing still reports an exit code, and
                // calling that OK is how `doctor` came to bless a broken
                // install right up until the command that needed it failed.
                Ok(output) if output.success() => dependencies.push(DependencyStatus {
                    name: program.into(),
                    state: DependencyState::Ok,
                    detail: output
                        .combined()
                        .trim()
                        .lines()
                        .next()
                        .unwrap_or("available")
                        .into(),
                    remediation: None,
                }),
                Ok(output) => dependencies.push(DependencyStatus {
                    name: program.into(),
                    state: DependencyState::Unavailable,
                    detail: match output.code {
                        Some(code) => match output.combined().trim().lines().next() {
                            Some(line) if !line.is_empty() => format!("exited {code}: {line}"),
                            _ => format!("exited {code}"),
                        },
                        None => "timed out".into(),
                    },
                    remediation: Some(remediation.into()),
                }),
                Err(error) => dependencies.push(DependencyStatus {
                    name: program.into(),
                    state: DependencyState::Unavailable,
                    detail: error.to_string(),
                    remediation: Some(remediation.into()),
                }),
            }
        }
        let profile_issues = config
            .profiles
            .iter()
            .filter_map(|(name, profile)| {
                profile
                    .validate_local_resources()
                    .err()
                    .map(|error| format!("{name}: {error}"))
            })
            .collect::<Vec<_>>();
        let healthy = dependencies
            .iter()
            .all(|item| item.state == DependencyState::Ok)
            && profile_issues.is_empty();
        Ok(DoctorReport {
            config_path: self.store.path().to_path_buf(),
            schema_version: config.schema_version,
            profile_count: config.profiles.len(),
            dependencies,
            profile_issues,
            healthy,
        })
    }

    pub fn repository_roots(&self) -> Result<Vec<PathBuf>, GitBoundError> {
        Ok(self.store.load()?.repository_roots)
    }

    pub fn add_repository_root(&self, root: &Path) -> Result<PathBuf, GitBoundError> {
        let canonical = std::fs::canonicalize(root).map_err(|error| {
            GitBoundError::usage(format!(
                "could not resolve repository root {}: {error}",
                root.display()
            ))
        })?;
        if !canonical.is_dir() {
            return Err(GitBoundError::usage(
                "repository root must be an existing directory",
            ));
        }
        self.store.update(|config| {
            if !config
                .repository_roots
                .iter()
                .any(|existing| paths_equal(existing, &canonical))
            {
                config.repository_roots.push(canonical.clone());
            }
            Ok(())
        })?;
        Ok(canonical)
    }

    pub fn remove_repository_root(&self, root: &Path) -> Result<(), GitBoundError> {
        // Roots are stored canonicalised by `add_repository_root`, so a caller
        // passing a relative path, a `~` prefix, or a trailing separator would
        // otherwise never match. Fall back to the raw path when the directory
        // no longer exists, so a deleted root can still be removed.
        let target = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
        self.store.update(|config| {
            let before = config.repository_roots.len();
            config
                .repository_roots
                .retain(|existing| !paths_equal(existing, &target) && !paths_equal(existing, root));
            if config.repository_roots.len() == before {
                return Err(GitBoundError::usage(
                    "approved repository root does not exist",
                ));
            }
            Ok(())
        })
    }

    pub fn scan_repositories(
        &self,
        cancel: &AtomicBool,
        emit: impl FnMut(RepositoryScanEvent),
    ) -> Result<Vec<crate::api::RepositorySummary>, GitBoundError> {
        let config = self.store.load()?;
        Ok(repository::scan_roots(self.runner, &config, cancel, emit))
    }
}

fn paths_equal(left: &Path, right: &Path) -> bool {
    if cfg!(windows) {
        left.to_string_lossy()
            .eq_ignore_ascii_case(&right.to_string_lossy())
    } else {
        left == right
    }
}
