# GitBound

[![CI](https://github.com/BitKiln/GitBound/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/BitKiln/GitBound/actions/workflows/ci.yml)
[![CodeQL](https://github.com/BitKiln/GitBound/actions/workflows/codeql.yml/badge.svg?branch=main)](https://github.com/BitKiln/GitBound/actions/workflows/codeql.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![MSRV](https://img.shields.io/badge/rustc-1.88%2B-orange.svg)](#development)

GitBound is a safety-first local identity manager for developers who use personal, work, client, or organization GitHub accounts on the same computer. It binds each repository to an explicit profile and checks the Git author, GitHub CLI account, SSH key, remote host, and optional owner policy before work leaves your machine.

GitBound delegates credentials to GitHub CLI, Git credential helpers, and OpenSSH. It never asks for, reads, or stores GitHub tokens.

GitBound ships a command-line tool, a Tauri 2 desktop application for Windows, macOS, and Linux, and a CI mode that lets a pipeline enforce the same identity policy before a change merges. The desktop app uses the same Rust safety layer and keeps GitHub CLI switching separate from repository binding.

## Documentation

New to GitBound? Start with the [user guide](docs/USER_GUIDE.md). The
[documentation index](docs/README.md) lists everything else.

| I want to...                            | Read                                                         |
| --------------------------------------- | ------------------------------------------------------------ |
| Set it up and use it                    | [User guide](docs/USER_GUIDE.md)                             |
| Enforce identity in a pipeline          | [Continuous integration](docs/CI.md)                         |
| Build the desktop app from source       | [Building the desktop application](docs/BUILDING_DESKTOP.md) |
| Understand how the code fits together   | [Architecture](ARCHITECTURE.md)                              |
| Report a vulnerability, verify an asset | [Security policy](SECURITY.md)                               |
| Contribute a change                     | [Contributing](CONTRIBUTING.md)                              |

## Install

Prebuilt Windows desktop artifacts are published on the
[Releases](https://github.com/BitKiln/GitBound/releases) page, each with a `.sha256` for verification. They
are unsigned, so Windows warns on first run; see
[SECURITY.md](SECURITY.md#release-verification).

Install a Rust toolchain, then build from source:

```console
cargo install --path .
gitbound --help
```

GitBound also expects `git`, `gh`, and `ssh` on `PATH`. Run `gitbound doctor` to inspect the local setup, or `gitbound doctor --json` for a structured report.

Build the desktop app from source. The frontend is checked-in static
HTML, CSS, and JavaScript, so cargo is the entire toolchain - there is no
Node, no npm, and no bundler:

```console
cargo build --release -p gitbound-desktop
```

That produces a standalone executable. Building `.msi` and NSIS installers
additionally needs the Tauri CLI, itself installed through cargo:

```console
cargo install tauri-cli --version "^2" --locked
cd desktop && cargo tauri build --bundles msi,nsis
```

For Windows prerequisites, EXE-only builds, installer output paths, and clean
rebuild instructions, see [Building the desktop application](docs/BUILDING_DESKTOP.md).

## Quick start

Create profiles using flags or omit required flags in an interactive terminal to be prompted:

```console
gitbound profile add personal \
  --github-user alice \
  --git-name "Alice Developer" \
  --git-email alice@example.com \
  --ssh-key ~/.ssh/id_ed25519_personal \
  --allowed-owner alice

gitbound profile add work \
  --github-user alice-company \
  --git-name "Alice Developer" \
  --git-email alice@company.example \
  --ssh-key ~/.ssh/id_ed25519_company \
  --allowed-owner company-name
```

Bind the current repository. Binding does not switch GitHub CLI unless requested explicitly:

```console
gitbound bind work
gitbound bind work --switch
gitbound status
gitbound check
gitbound status --repo ../another-repository
```

Clone and bind a repository in one identity-safe operation. GitBound uses SSH
when the profile has an SSH key and HTTPS otherwise; override that choice with
`--protocol`:

```console
gitbound clone work company-name/device-firmware
gitbound clone personal alice/project ./project --protocol https
```

GitBound validates the host and owner policy before cloning, switches GitHub
CLI explicitly, and restores the previous account if cloning or binding fails.

Import an existing repository identity without reading credentials, then apply a
profile automatically to every repository under a directory using native Git
`includeIf` rules:

```console
gitbound profile import existing-work
gitbound directory add work ~/work
gitbound directory list
gitbound directory sync work
gitbound directory remove ~/work
```

Directory rules write marked profile fragments beside GitBound's configuration
and add an exact global include. Removal deletes only that exact include and only
removes fragments carrying GitBound's marker.

The desktop app lists, adds, and removes the same rules under Settings, and shows
each rule's `includeIf` key so it can be checked against `git config --global --list`.
It also clones (Repositories → Clone) and audits commit authorship (repository
details → Commit authorship), so the CLI and the app now cover the same ground.

For HTTPS remotes, configure GitHub CLI as Git's credential helper:

```console
gh auth setup-git --hostname github.com
```

For SSH remotes, GitBound writes a repository-local `core.sshCommand` using the profile key and `IdentitiesOnly=yes`.

## Safety hooks

Hooks are opt-in and GitBound never replaces or chains an existing hook setup:

```console
gitbound hooks install
gitbound hooks status
gitbound hooks uninstall
```

Each subcommand takes `--repo <path>` to target a repository other than the current directory. The desktop app offers the same three actions on a repository's details page, under Commit hooks.

The pre-commit hook performs local author and policy checks. The pre-push hook performs full GitHub CLI and SSH verification and fails closed when a network-dependent identity cannot be verified.

## Continuous integration

The same checks run in a pipeline. `verify` is `check` with defaults suited to a
runner; `audit` inspects who actually authored a range of commits, which is
evidence a developer's machine does not have.

```yaml
- uses: BitKiln/GitBound@v1
  with:
    version: "1.0.0"
    command: both
    sarif-file: gitbound.sarif
```

```console
gitbound verify --format github --output sarif:gitbound.sarif
gitbound audit --range origin/main..HEAD
```

`--format` accepts `human`, `json`, `sarif`, `junit`, `github`, `markdown`, and
`auto` — which selects GitHub workflow commands inside a runner and human output
everywhere else. `--output` is repeatable and each entry may name its own format,
so one inspection can feed several consumers.

A repository can commit a `.gitbound.toml` declaring which addresses, hosts,
and owners it accepts. GitBound reads that file and never writes it, and an
absent file leaves behaviour unchanged. See [docs/CI.md](docs/CI.md).

## Configuration

Configuration is stored in the platform-native user configuration directory. Override the location with `GITBOUND_CONFIG` for portable or test setups.

```toml
schema_version = 3
repository_roots = ["/home/alice/projects"]

[profiles.work]
github_user = "alice-company"
git_name = "Alice Developer"
git_email = "alice@company.example"
hostname = "github.com"
ssh_host = "github.com-company"
ssh_key = "~/.ssh/id_ed25519_company"
allowed_owners = ["company-name"]
signing_key = "ABC123"
signing_format = "openpgp"
require_signing = true
```

`hostname` is the real GitHub host, and it is what the GitHub CLI, HTTPS remote
URLs, and the credential helper are keyed by. `ssh_host` is optional and holds
the `Host` alias from your SSH configuration when you keep one account per
alias; it is used only to build and match SSH remote URLs, and it defaults to
`hostname` when omitted. A configuration written before schema 3 that stored an
alias in `hostname` is migrated automatically on first read.

Profiles can require OpenPGP or SSH commit signing. Binding snapshots and applies
`user.signingKey`, `gpg.format`, and `commit.gpgSign`; unbinding restores their
original repository-local values exactly.

Generate shell completions without modifying shell configuration:

```console
gitbound completions bash > gitbound.bash
gitbound completions powershell > _gitbound.ps1
```

Repository binding and rollback metadata live only in local Git configuration. `gitbound unbind` restores the exact values that existed before the first bind.

## Exit codes

- `0`: success
- `1`: identity or policy check failed
- `2`: invalid input or configuration
- `3`: missing dependency or subprocess failure

## Development

```console
cargo fmt --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo deny check
cargo build --release -p gitbound-desktop
```

## Contributing

Pull requests are welcome. [CONTRIBUTING.md](CONTRIBUTING.md) covers the branch
model, what CI runs, and the frontend/backend contract that the desktop app
depends on; participation is governed by our
[Code of Conduct](CODE_OF_CONDUCT.md).

One rule is worth repeating outside that document: a check that reports `ok`
while the push would go out under the wrong identity is a security bug, not a
correctness nit. Report those privately through the
[security policy](SECURITY.md).

## License

MIT — see [LICENSE](LICENSE). Third-party notices are recorded in
[NOTICE](NOTICE).
