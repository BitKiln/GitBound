use crate::error::GitBoundError;
use regex::Regex;
use serde::{Deserialize, Serialize};
use url::Url;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum RemoteProtocol {
    Ssh,
    Https,
    Http,
}

impl RemoteInfo {
    pub fn as_url(&self, protocol: RemoteProtocol, hostname: &str) -> String {
        match protocol {
            RemoteProtocol::Ssh => format!("git@{hostname}:{}/{}.git", self.owner, self.repository),
            RemoteProtocol::Https => {
                format!("https://{hostname}/{}/{}.git", self.owner, self.repository)
            }
            RemoteProtocol::Http => {
                format!("http://{hostname}/{}/{}.git", self.owner, self.repository)
            }
        }
    }
}

pub fn parse_repository(input: &str, hostname: &str) -> Result<RemoteInfo, GitBoundError> {
    if input.contains("://") || {
        static SCP_PATTERN: std::sync::LazyLock<Regex> =
            std::sync::LazyLock::new(|| Regex::new(r"^(?:[^@]+@)?[^:]+:.+$").expect("valid regex"));
        SCP_PATTERN.is_match(input)
    } {
        return parse_remote(input);
    }
    build(input, RemoteProtocol::Https, hostname.to_string(), input)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RemoteInfo {
    pub url: String,
    pub protocol: RemoteProtocol,
    pub hostname: String,
    pub owner: String,
    pub repository: String,
}

pub fn parse_remote(input: &str) -> Result<RemoteInfo, GitBoundError> {
    let input = input.trim();
    if input.contains("://") {
        let url = Url::parse(input)
            .map_err(|e| GitBoundError::usage(format!("invalid remote URL: {e}")))?;
        let protocol = match url.scheme() {
            "ssh" => RemoteProtocol::Ssh,
            "https" => RemoteProtocol::Https,
            "http" => RemoteProtocol::Http,
            other => {
                return Err(GitBoundError::usage(format!(
                    "unsupported remote protocol: {other}"
                )));
            }
        };
        let hostname = url
            .host_str()
            .ok_or_else(|| GitBoundError::usage("remote URL has no hostname"))?
            .to_string();
        let displayable = redact(input, &url, protocol);
        return build(&displayable, protocol, hostname, url.path());
    }
    static SCP: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
        Regex::new(r"^(?:[^@]+@)?(?P<host>[^:]+):(?P<path>.+)$").expect("valid regex")
    });
    if let Some(captures) = SCP.captures(input) {
        return build(
            input,
            RemoteProtocol::Ssh,
            captures["host"].to_string(),
            &captures["path"],
        );
    }
    Err(GitBoundError::usage(
        "unsupported remote URL; expected SSH, HTTPS, or HTTP",
    ))
}

/// The remote URL as it is safe to print.
///
/// A remote may carry credentials in its userinfo — `https://x-access-token:
/// ghp_…@github.com/owner/repo.git` is what a pipeline that writes its own
/// remote ends up with. `RemoteInfo::url` reaches human output, the JSON and
/// SARIF reports, and the Markdown table that `verify` appends to
/// `$GITHUB_STEP_SUMMARY`, which on a public repository anyone can read. So the
/// secret is dropped here, at the only place a remote is ever parsed, rather
/// than in each of the renderers.
///
/// Not every `user@` is a secret, so this strips two cases and leaves the rest
/// alone:
///
/// - **A password, under any scheme.** There is no legitimate reason for one to
///   be in a remote GitBound is reporting on.
/// - **A bare username over HTTP or HTTPS**, which is how GitHub takes a token:
///   `https://ghp_…@github.com/owner/repo.git` authenticates with the username
///   alone.
///
/// A bare username over SSH is kept. It is a login name — `ssh://git@host/…`
/// and `git@host:owner/repo.git` are the ordinary shapes — and removing it
/// would make the report disagree with `git remote -v` while hiding nothing.
///
/// Anything left untouched is returned exactly as the user wrote it, so a
/// remote that needed no redaction is not also reformatted by url's
/// normalisation on its way into the report.
fn redact(original: &str, url: &Url, protocol: RemoteProtocol) -> String {
    let credentialed = url.password().is_some()
        || (!url.username().is_empty()
            && matches!(protocol, RemoteProtocol::Https | RemoteProtocol::Http));
    if !credentialed {
        return original.to_string();
    }
    let mut sanitized = url.clone();
    // Both setters only fail for a URL that cannot have a host, and this one
    // has already been shown to have one.
    let _ = sanitized.set_password(None);
    let _ = sanitized.set_username("");
    sanitized.to_string()
}

fn build(
    original: &str,
    protocol: RemoteProtocol,
    hostname: String,
    path: &str,
) -> Result<RemoteInfo, GitBoundError> {
    let cleaned = path
        .trim_matches('/')
        .strip_suffix(".git")
        .unwrap_or(path.trim_matches('/'));
    let mut parts = cleaned.split('/').filter(|part| !part.is_empty());
    let owner = parts
        .next()
        .ok_or_else(|| GitBoundError::usage("remote URL has no owner"))?;
    let repository = parts
        .next()
        .ok_or_else(|| GitBoundError::usage("remote URL has no repository"))?;
    if parts.next().is_some()
        || owner == "."
        || owner == ".."
        || repository == "."
        || repository == ".."
    {
        return Err(GitBoundError::usage(
            "repository must identify exactly one owner and repository",
        ));
    }
    Ok(RemoteInfo {
        url: original.to_string(),
        protocol,
        hostname,
        owner: owner.to_string(),
        repository: repository.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_supported_remotes() {
        for value in [
            "git@github.com:Org/repo.git",
            "ssh://git@github.example/Org/repo.git",
            "https://github.com/Org/repo.git",
        ] {
            let remote = parse_remote(value).unwrap();
            assert_eq!(remote.owner, "Org");
            assert_eq!(remote.repository, "repo");
        }
    }

    #[test]
    fn credentials_in_a_remote_never_survive_parsing() {
        // This URL reaches human output, the JSON and SARIF reports, and the
        // Markdown summary a pipeline publishes. The token must not be in any
        // of them.
        let remote =
            parse_remote("https://x-access-token:ghp_SECRET@github.com/Org/repo.git").unwrap();
        assert!(!remote.url.contains("ghp_SECRET"), "{}", remote.url);
        assert!(!remote.url.contains("x-access-token"), "{}", remote.url);
        assert_eq!(remote.url, "https://github.com/Org/repo.git");
        assert_eq!(remote.hostname, "github.com");
        assert_eq!(remote.owner, "Org");
        assert_eq!(remote.repository, "repo");

        // GitHub takes a token as the username alone, so a bare one over HTTPS
        // is a credential too.
        let remote = parse_remote("https://ghp_SECRET@github.com/Org/repo.git").unwrap();
        assert_eq!(remote.url, "https://github.com/Org/repo.git");

        // An SSH login name is kept, but a password alongside it is not.
        let remote = parse_remote("ssh://git:ghp_SECRET@github.com/Org/repo.git").unwrap();
        assert!(!remote.url.contains("ghp_SECRET"), "{}", remote.url);
    }

    #[test]
    fn a_remote_without_credentials_is_reported_verbatim() {
        for value in [
            "https://github.com/Org/repo.git",
            "git@github.com:Org/repo.git",
            "ssh://git@github.example/Org/repo.git",
        ] {
            assert_eq!(parse_remote(value).unwrap().url, value);
        }
    }

    #[test]
    fn builds_protocol_specific_urls() {
        let remote = parse_repository("Org/repo", "github.example").unwrap();
        assert_eq!(
            remote.as_url(RemoteProtocol::Ssh, "github.example"),
            "git@github.example:Org/repo.git"
        );
        assert!(parse_repository("Org/group/repo", "github.com").is_err());
    }
}
