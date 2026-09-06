# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [1.0.0] - 2026-09-06

First public release.

### Added

- **Identity binding.** A repository is bound to an explicit named profile,
  which is written to its local Git configuration — author name and email,
  signing key and format, and the SSH command used to reach the remote. The
  identity a commit will carry is a property of the repository, not of whatever
  the shell happened to be configured as.

- **Verification that fails closed.** `gitbound check` reports on the binding,
  the author identity, signing, the remote's host and owner, and the credential
  helper. A check that cannot be evaluated is `Unverified` and blocks, rather
  than passing quietly. Exit codes are stable: `0` ok, `1` check failed, `2` bad
  input, `3` a missing dependency.

- **A desktop application.** A Tauri 2 app for Windows, macOS, and Linux with no
  npm toolchain, sharing the Rust core with the command-line tool. It covers
  identities, repositories and their bindings, cloning, commit hooks, directory
  rules, the commit-authorship audit, SSH keys, and diagnostics. It only reads
  folders the user has explicitly approved.

- **CI mode.** `gitbound verify` runs the same check engine with defaults suited
  to a pipeline, and writes JSON, SARIF, and a Markdown job summary. A composite
  `action.yml` exposes `check`, `verify`, and `audit` to other repositories.

- **Commit-authorship audit.** `gitbound audit --range <revspec>` judges who
  authored a range of commits, and their signatures, against the policy.

- **A committed repository policy.** An optional `.gitbound.toml` lets a
  repository state the identity rules it expects to be judged by, so the policy
  travels with the code. It is schema-versioned and fails closed on a version it
  does not understand.

- **Commit hooks.** `pre-commit` and `pre-push` hooks refuse a mismatched
  identity before the commit or push exists, rather than reporting it afterwards.

- **Directory rules.** A folder can be assigned to an identity through Git's own
  `includeIf`, so every repository below it — including ones cloned later, by any
  tool — uses that identity.

- **Clone and bind in one step.** `gitbound clone` authenticates the clone as the
  profile, so the very first network call is made under the right identity and a
  mismatched key fails loudly instead of quietly cloning as somebody else.

- **`--offline`** to skip the checks that need the network, and
  **`--require-policy`** to fail when a repository ships no policy at all.

### Changed

- **macOS releases are Apple Silicon only.** The `macos-13` runner pool, the
  only GitHub-hosted way to build `x86_64-apple-darwin`, is being wound down;
  jobs queued there long enough to be cancelled, and because `publish` needs
  every matrix leg, a single stuck leg meant no release at all. The Intel leg is
  dropped rather than left to block the others. The composite action now fails
  with a clear message on an Intel macOS runner instead of trying to download an
  asset that is not published.

### Security

- **No credentials are stored.** GitBound never asks for, reads, or stores a
  GitHub token. Authentication is delegated to GitHub CLI, Git credential
  helpers, and OpenSSH.

- **Git's environment is not trusted.** `GIT_CONFIG_PARAMETERS` is stripped from
  every `git` invocation, along with `GIT_ALTERNATE_OBJECT_DIRECTORIES`,
  `GIT_EXEC_PATH`, `GIT_PROXY_COMMAND`, and `GIT_ASKPASS`. Git honours those
  above a repository's own configuration, so anything able to set one could
  otherwise turn a mismatched identity into a passing check while the commit
  still went out under the wrong address.

- **Credentials in a remote URL never reach a report.** A remote written as
  `https://x-access-token:<token>@github.com/owner/repo.git` has its userinfo
  dropped where the remote is parsed, so it cannot appear in human output, in the
  JSON or SARIF reports, or in the job summary a CI run publishes.

### Migrating from GitPersona

A repository bound by the earlier GitPersona releases is still recognised. Its
`gitpersona.*` configuration keys, managed hooks, and generated `includeIf`
fragments are migrated to their `gitbound.*` equivalents on the first `bind` or
`unbind`, which preserves the original identity the binding had backed up.
