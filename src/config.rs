use crate::error::GitBoundError;
use clap::ValueEnum;
use directories::{BaseDirs, ProjectDirs};
use fs4::FileExt;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    env,
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

pub const CURRENT_SCHEMA_VERSION: u32 = 3;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Config {
    pub schema_version: u32,
    #[serde(default)]
    pub profiles: BTreeMap<String, Profile>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub directories: Vec<DirectoryRule>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub repository_roots: Vec<PathBuf>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            schema_version: CURRENT_SCHEMA_VERSION,
            profiles: BTreeMap::new(),
            directories: Vec::new(),
            repository_roots: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DirectoryRule {
    pub profile: String,
    pub path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Profile {
    pub github_user: String,
    pub git_name: String,
    pub git_email: String,
    /// The real GitHub host this profile belongs to, such as `github.com` or a
    /// GitHub Enterprise domain. This is the name the GitHub CLI, the HTTPS
    /// remote URL and the credential helper are all keyed by, so it must be a
    /// host that actually resolves — never an SSH config alias.
    #[serde(default = "default_hostname")]
    pub hostname: String,
    /// The `Host` alias from the user's SSH config, when they keep one account
    /// per alias (`github.com-work`). It is only ever used to build and match
    /// SSH remote URLs; `hostname` stays authoritative everywhere else. `None`
    /// means the profile has no alias and SSH uses `hostname` directly.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh_host: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh_key: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allowed_owners: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signing_key: Option<String>,
    #[serde(default)]
    pub signing_format: SigningFormat,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub require_signing: bool,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum SigningFormat {
    #[default]
    Openpgp,
    Ssh,
}

impl SigningFormat {
    pub fn as_git_value(self) -> &'static str {
        match self {
            Self::Openpgp => "openpgp",
            Self::Ssh => "ssh",
        }
    }
}

fn default_hostname() -> String {
    "github.com".to_string()
}

impl Profile {
    /// The host to put in an SSH remote URL and to authenticate against over
    /// SSH: the alias when the profile defines one, and the real host
    /// otherwise.
    pub fn ssh_host(&self) -> &str {
        self.ssh_host.as_deref().unwrap_or(&self.hostname)
    }

    pub fn validate(&self) -> Result<(), GitBoundError> {
        for (label, value) in [
            ("GitHub user", &self.github_user),
            ("Git name", &self.git_name),
            ("Git email", &self.git_email),
        ] {
            if value.trim().is_empty() {
                return Err(GitBoundError::usage(format!("{label} cannot be empty")));
            }
            if value.contains('\n') || value.contains('\r') || value.contains('\0') {
                return Err(GitBoundError::usage(format!(
                    "{label} cannot contain newlines or null bytes"
                )));
            }
        }
        if self
            .github_user
            .chars()
            .any(|c| c.is_whitespace() || c == '/' || c == '\\' || c == '"')
        {
            return Err(GitBoundError::usage(
                "GitHub user cannot contain whitespace or slashes",
            ));
        }
        validate_hostname(&self.hostname)?;
        if let Some(ssh_host) = &self.ssh_host {
            validate_hostname(ssh_host)?;
        }
        if self
            .ssh_key
            .as_ref()
            .is_some_and(|key| key.as_os_str().is_empty())
        {
            return Err(GitBoundError::usage("SSH key path cannot be empty"));
        }
        if self
            .allowed_owners
            .iter()
            .any(|owner| owner.trim().is_empty())
        {
            return Err(GitBoundError::usage("allowed owners cannot be empty"));
        }
        if let Some(key) = &self.signing_key {
            if key.trim().is_empty() {
                return Err(GitBoundError::usage("signing key cannot be empty"));
            }
            if key.contains('\n') || key.contains('\r') || key.contains('\0') {
                return Err(GitBoundError::usage(
                    "signing key cannot contain newlines or null bytes",
                ));
            }
        }
        if self.require_signing && self.signing_key.is_none() {
            return Err(GitBoundError::usage(
                "required commit signing needs a signing key",
            ));
        }
        Ok(())
    }

    pub fn validate_local_resources(&self) -> Result<(), GitBoundError> {
        if let Some(key) = &self.ssh_key {
            let expanded = expand_path(key)?;
            if !expanded.is_file() {
                return Err(GitBoundError::usage(format!(
                    "SSH key does not exist or is not a file: {}",
                    expanded.display()
                )));
            }
        }
        if self.signing_format == SigningFormat::Ssh
            && let Some(key) = &self.signing_key
            && !key.starts_with("key::")
            && !key.starts_with("ssh-")
            && !key.starts_with("ecdsa-")
        {
            let expanded = expand_path(Path::new(key))?;
            if !expanded.is_file() {
                return Err(GitBoundError::usage(format!(
                    "SSH signing key does not exist or is not a file: {}",
                    expanded.display()
                )));
            }
        }
        Ok(())
    }
}

pub fn validate_profile_name(name: &str) -> Result<(), GitBoundError> {
    static PATTERN: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
        Regex::new(r"^[A-Za-z0-9][A-Za-z0-9._-]{0,63}$").expect("valid regex")
    });
    if PATTERN.is_match(name) {
        Ok(())
    } else {
        Err(GitBoundError::usage(
            "profile name must match [A-Za-z0-9][A-Za-z0-9._-]{0,63}",
        ))
    }
}

/// Move an SSH config alias out of `hostname` and into `ssh_host`.
///
/// Before schema 3 there was one host field, so anyone running several accounts
/// off SSH aliases had to store the alias — `github.com-work` — as the
/// hostname. That silently broke every GitHub CLI call, because `gh` only knows
/// real hosts and reports the profile as unauthenticated. The split fixes it,
/// but only if the existing value is repaired on the way in.
///
/// An alias is recognised conservatively: the part before the dash must look
/// like a host (it contains a dot) and the part after it must not (a real
/// hostname such as `sub-domain.example.com` therefore never matches, because
/// its dash is followed by more dotted labels). Anything that does not match
/// is left exactly as it was.
fn migrate_ssh_alias(profile: &mut Profile) {
    if profile.ssh_host.is_some() {
        return;
    }
    let Some(host) = split_ssh_alias(&profile.hostname) else {
        return;
    };
    profile.ssh_host = Some(profile.hostname.clone());
    profile.hostname = host;
}

fn split_ssh_alias(value: &str) -> Option<String> {
    value.match_indices('-').find_map(|(index, _)| {
        let (host, alias) = (&value[..index], &value[index + 1..]);
        (host.contains('.') && !alias.is_empty() && !alias.contains('.')).then(|| host.to_string())
    })
}

pub fn validate_hostname(hostname: &str) -> Result<(), GitBoundError> {
    let value = hostname.trim();
    if value.is_empty()
        || value.contains("://")
        || value.contains('/')
        || value.chars().any(char::is_whitespace)
    {
        Err(GitBoundError::usage(
            "hostname must be a host name without a scheme or path",
        ))
    } else {
        Ok(())
    }
}

pub fn expand_path(path: &Path) -> Result<PathBuf, GitBoundError> {
    let text = path.to_string_lossy();
    if text == "~" || text.starts_with("~/") || text.starts_with("~\\") {
        let base = BaseDirs::new()
            .ok_or_else(|| GitBoundError::dependency("could not determine the home directory"))?;
        let suffix = text.trim_start_matches('~').trim_start_matches(['/', '\\']);
        Ok(base.home_dir().join(suffix))
    } else {
        Ok(path.to_path_buf())
    }
}

#[derive(Debug, Clone)]
pub struct ConfigStore {
    path: PathBuf,
}

impl ConfigStore {
    pub fn discover() -> Result<Self, GitBoundError> {
        if let Some(path) = env::var_os("GITBOUND_CONFIG") {
            return Ok(Self {
                path: PathBuf::from(path),
            });
        }
        let dirs = ProjectDirs::from("dev", "GitBound", "GitBound").ok_or_else(|| {
            GitBoundError::dependency("could not determine the operating-system config directory")
        })?;
        Ok(Self {
            path: dirs.config_dir().join("config.toml"),
        })
    }

    pub fn at(path: PathBuf) -> Self {
        Self { path }
    }
    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn load(&self) -> Result<Config, GitBoundError> {
        self.load_inner(true)
    }

    fn load_inner(&self, persist_migration: bool) -> Result<Config, GitBoundError> {
        if !self.path.exists() {
            return Ok(Config::default());
        }
        let text = fs::read_to_string(&self.path).map_err(|e| {
            GitBoundError::dependency(format!("could not read {}: {e}", self.path.display()))
        })?;
        let mut config: Config = toml::from_str(&text).map_err(|e| {
            GitBoundError::usage(format!("invalid config {}: {e}", self.path.display()))
        })?;
        if config.schema_version > CURRENT_SCHEMA_VERSION || config.schema_version == 0 {
            return Err(GitBoundError::usage(format!(
                "unsupported config schema version {}",
                config.schema_version
            )));
        }
        let migrated = config.schema_version < CURRENT_SCHEMA_VERSION;
        if migrated {
            if config.schema_version < 3 {
                for profile in config.profiles.values_mut() {
                    migrate_ssh_alias(profile);
                }
            }
            config.schema_version = CURRENT_SCHEMA_VERSION;
        }
        for (name, profile) in &config.profiles {
            validate_profile_name(name)?;
            profile.validate()?;
        }
        for rule in &config.directories {
            if !config.profiles.contains_key(&rule.profile) {
                return Err(GitBoundError::usage(format!(
                    "directory rule references missing profile '{}'",
                    rule.profile
                )));
            }
            if rule.path.as_os_str().is_empty() {
                return Err(GitBoundError::usage("directory rule path cannot be empty"));
            }
        }
        config.repository_roots.sort_by(|left, right| {
            left.to_string_lossy()
                .to_ascii_lowercase()
                .cmp(&right.to_string_lossy().to_ascii_lowercase())
        });
        config.repository_roots.dedup_by(|left, right| {
            if cfg!(windows) {
                left.to_string_lossy()
                    .eq_ignore_ascii_case(&right.to_string_lossy())
            } else {
                left == right
            }
        });
        if migrated && persist_migration {
            self.persist_migration(&config)?;
        }
        Ok(config)
    }

    fn persist_migration(&self, config: &Config) -> Result<(), GitBoundError> {
        let parent = self
            .path
            .parent()
            .ok_or_else(|| GitBoundError::dependency("config path has no parent directory"))?;
        fs::create_dir_all(parent).map_err(|error| {
            GitBoundError::dependency(format!("could not create {}: {error}", parent.display()))
        })?;
        // The lock file is intentionally kept on disk across operations.
        // Deleting it creates a TOCTOU race where concurrent processes can lock different inodes.
        let lock = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(parent.join("config.lock"))
            .map_err(|error| {
                GitBoundError::dependency(format!("could not open config lock: {error}"))
            })?;
        FileExt::lock(&lock).map_err(|error| {
            GitBoundError::dependency(format!("could not lock config: {error}"))
        })?;
        let rendered = toml::to_string_pretty(config).map_err(|error| {
            GitBoundError::dependency(format!("could not serialize config: {error}"))
        })?;
        let mut temp = tempfile::NamedTempFile::new_in(parent).map_err(|error| {
            GitBoundError::dependency(format!("could not create temporary config: {error}"))
        })?;
        temp.write_all(rendered.as_bytes())
            .and_then(|_| temp.as_file().sync_all())
            .map_err(|error| {
                GitBoundError::dependency(format!("could not write temporary config: {error}"))
            })?;
        temp.persist(&self.path).map_err(|error| {
            GitBoundError::dependency(format!("could not persist config: {}", error.error))
        })?;
        FileExt::unlock(&lock).ok();
        Ok(())
    }

    pub fn update<T>(
        &self,
        operation: impl FnOnce(&mut Config) -> Result<T, GitBoundError>,
    ) -> Result<T, GitBoundError> {
        let parent = self
            .path
            .parent()
            .ok_or_else(|| GitBoundError::dependency("config path has no parent directory"))?;
        fs::create_dir_all(parent).map_err(|e| {
            GitBoundError::dependency(format!("could not create {}: {e}", parent.display()))
        })?;
        // The lock file is intentionally kept on disk across operations.
        // Deleting it creates a TOCTOU race where concurrent processes can lock different inodes.
        let lock_path = parent.join("config.lock");
        let lock = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(&lock_path)
            .map_err(|e| GitBoundError::dependency(format!("could not open config lock: {e}")))?;
        FileExt::lock(&lock)
            .map_err(|e| GitBoundError::dependency(format!("could not lock config: {e}")))?;
        let mut config = self.load_inner(false)?;
        config.schema_version = CURRENT_SCHEMA_VERSION;
        let result = operation(&mut config)?;
        let rendered = toml::to_string_pretty(&config)
            .map_err(|e| GitBoundError::dependency(format!("could not serialize config: {e}")))?;
        let mut temp = tempfile::NamedTempFile::new_in(parent).map_err(|e| {
            GitBoundError::dependency(format!("could not create temporary config: {e}"))
        })?;
        temp.write_all(rendered.as_bytes())
            .and_then(|_| temp.as_file().sync_all())
            .map_err(|e| {
                GitBoundError::dependency(format!("could not write temporary config: {e}"))
            })?;
        temp.persist(&self.path).map_err(|e| {
            GitBoundError::dependency(format!("could not persist config: {}", e.error))
        })?;
        FileExt::unlock(&lock).ok();
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_name_validation() {
        assert!(validate_profile_name("work-1.main").is_ok());
        assert!(validate_profile_name("bad name").is_err());
        assert!(validate_profile_name(&"a".repeat(65)).is_err());
    }

    #[test]
    fn config_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let store = ConfigStore::at(dir.path().join("config.toml"));
        store
            .update(|config| {
                config.profiles.insert(
                    "work".into(),
                    Profile {
                        github_user: "alice".into(),
                        git_name: "Alice".into(),
                        git_email: "a@example.com".into(),
                        hostname: "github.com".into(),
                        ssh_host: None,
                        ssh_key: None,
                        allowed_owners: vec!["Org".into()],
                        signing_key: None,
                        signing_format: SigningFormat::Openpgp,
                        require_signing: false,
                    },
                );
                Ok(())
            })
            .unwrap();
        store
            .update(|config| {
                config.profiles.get_mut("work").unwrap().git_name = "Alice Updated".into();
                Ok(())
            })
            .unwrap();
        assert_eq!(store.load().unwrap().profiles["work"].github_user, "alice");
        assert_eq!(
            store.load().unwrap().profiles["work"].git_name,
            "Alice Updated"
        );
    }

    #[test]
    fn v1_config_is_atomically_migrated_with_empty_roots() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        fs::write(&path, "schema_version = 1\n").unwrap();
        let config = ConfigStore::at(path.clone()).load().unwrap();
        assert_eq!(config.schema_version, CURRENT_SCHEMA_VERSION);
        assert!(config.repository_roots.is_empty());
        let persisted = fs::read_to_string(path).unwrap();
        assert!(persisted.contains("schema_version = 3"));
    }

    #[test]
    fn a_v2_ssh_alias_hostname_is_split_into_a_real_host_and_an_alias() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        fs::write(
            &path,
            "schema_version = 2

[profiles.work]
github_user = \"BitKiln\"
git_name = \"B\"
git_email = \"b@example.com\"
hostname = \"github.com-BitKiln\"
",
        )
        .unwrap();
        let profile = ConfigStore::at(path)
            .load()
            .unwrap()
            .profiles
            .remove("work")
            .unwrap();
        assert_eq!(profile.hostname, "github.com");
        assert_eq!(profile.ssh_host.as_deref(), Some("github.com-BitKiln"));
        assert_eq!(profile.ssh_host(), "github.com-BitKiln");
    }

    #[test]
    fn a_hostname_whose_dash_is_part_of_a_label_is_left_alone() {
        for hostname in [
            "github.com",
            "sub-domain.example.com",
            "git-host.example.com",
        ] {
            let mut profile = Profile {
                github_user: "alice".into(),
                git_name: "Alice".into(),
                git_email: "alice@example.com".into(),
                hostname: hostname.into(),
                ssh_host: None,
                ssh_key: None,
                allowed_owners: vec![],
                signing_key: None,
                signing_format: SigningFormat::Openpgp,
                require_signing: false,
            };
            migrate_ssh_alias(&mut profile);
            assert_eq!(profile.hostname, hostname);
            assert_eq!(profile.ssh_host, None);
            assert_eq!(profile.ssh_host(), hostname);
        }
    }

    #[test]
    fn an_explicit_ssh_host_survives_migration_untouched() {
        let mut profile = Profile {
            github_user: "alice".into(),
            git_name: "Alice".into(),
            git_email: "alice@example.com".into(),
            hostname: "github.com-alice".into(),
            ssh_host: Some("chosen-alias".into()),
            ssh_key: None,
            allowed_owners: vec![],
            signing_key: None,
            signing_format: SigningFormat::Openpgp,
            require_signing: false,
        };
        migrate_ssh_alias(&mut profile);
        assert_eq!(profile.hostname, "github.com-alice");
        assert_eq!(profile.ssh_host.as_deref(), Some("chosen-alias"));
    }

    #[test]
    fn future_schema_fails_closed() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        fs::write(&path, "schema_version = 99\n").unwrap();
        assert!(ConfigStore::at(path).load().is_err());
    }

    #[test]
    fn rejects_control_chars_and_whitespace_in_profiles() {
        let base = Profile {
            github_user: "alice".into(),
            git_name: "Alice".into(),
            git_email: "alice@example.com".into(),
            hostname: "github.com".into(),
            ssh_host: None,
            ssh_key: None,
            allowed_owners: vec![],
            signing_key: None,
            signing_format: SigningFormat::Openpgp,
            require_signing: false,
        };

        let mut newline_user = base.clone();
        newline_user.github_user = "alice\nadmin".into();
        assert!(newline_user.validate().is_err());

        let mut space_user = base.clone();
        space_user.github_user = "alice admin".into();
        assert!(space_user.validate().is_err());

        let mut slash_user = base.clone();
        slash_user.github_user = "alice/admin".into();
        assert!(slash_user.validate().is_err());

        let mut crlf_email = base.clone();
        crlf_email.git_email = "alice@example.com\r\ncc:bad@example.com".into();
        assert!(crlf_email.validate().is_err());
    }

    #[test]
    fn accepts_literal_ssh_public_key_for_signing() {
        let profile = Profile {
            github_user: "alice".into(),
            git_name: "Alice".into(),
            git_email: "alice@example.com".into(),
            hostname: "github.com".into(),
            ssh_host: None,
            ssh_key: None,
            allowed_owners: vec![],
            signing_key: Some(
                "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAI12345 alice@example.com".into(),
            ),
            signing_format: SigningFormat::Ssh,
            require_signing: true,
        };
        assert!(profile.validate().is_ok());
        assert!(profile.validate_local_resources().is_ok());
    }
}
