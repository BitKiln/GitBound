# Product

<!-- impeccable:product-schema 1 -->

## Platform

web

## Stack

Tauri 2 desktop shell with a hand-written HTML, CSS, and JavaScript frontend embedded into the binary at compile time, backed by the existing Rust core and CLI. The build depends on cargo alone: no Node, no npm, and no bundler.

## Users

Individual developers who use personal, work, client, or organization GitHub accounts on the same Windows, macOS, or Linux computer. A second audience is served: continuous integration pipelines, which run the same verification against a change before it merges. Team-managed policy is expressed as an optional repository-committed policy file that GitBound reads and never writes.

## Product Purpose

GitBound binds repositories to explicit identities and verifies Git author, GitHub CLI account, SSH key, remote host and owner, and optional signing policy before work leaves the machine. Success means a developer can understand and correct identity drift without GitBound storing credentials or silently changing unrelated repositories.

## Positioning

GitBound treats commit identity, authentication identity, remote identity, and signing identity as separate, independently verifiable concerns. Repository-local binding, exact rollback, and explicit GitHub CLI switching are the product's safety mechanism.

## Operating Context

Users work in local Git repositories and already rely on Git, GitHub CLI, OpenSSH, and platform credential helpers. Profiles and approved repository-scan roots live in the platform-native GitBound configuration directory. Repository discovery is limited to folders selected by the user.

## Capabilities and Constraints

- Preserve the existing CLI, exit codes, hooks, clone flow, directory rules, signing support, and rollback behavior.
- Provide profile management, repository discovery and binding, SSH/signing inspection, repository status, and diagnostics in a desktop interface.
- Never store GitHub tokens, passwords, or other credentials.
- Never mutate global Git identity or manage `~/.ssh/config`.
- Keep repository binding and GitHub CLI switching as separate explicit actions.
- Do not perform broad automatic filesystem scans or automatic network checks.

### CI amendment

The same verification engine is exposed to pipelines. This extends the product
without relaxing any rule above.

- Render an existing check report into SARIF, JUnit, GitHub workflow commands,
  Markdown, or JSON. The report itself is unchanged; only its presentation is
  new, and the exit-code contract is untouched.
- Audit the commit authorship of a revision range, because that is the question
  a pipeline can answer that a developer's machine cannot.
- Read an optional `.gitbound.toml` committed to the repository under
  inspection. That file carries **policy only** — allowed email domains, hosts,
  owners, signing requirements. It never carries credentials, key material, or
  paths outside the repository. GitBound reads it and never writes it, it is
  entirely opt-in, and an absent file means behaviour is exactly as before.
- Show GitHub Actions run status for a repository in the desktop app only on an
  explicit, user-initiated refresh. This does not weaken the no-automatic-network
  rule; nothing is fetched until the user asks.

## Brand Commitments

The product name is GitBound. Its voice is precise, calm, safety-first, and explanatory without being alarmist.

## Evidence on Hand

The Rust CLI and core already implement profiles, rollback-safe repository binding, status/check reports, GitHub CLI switching, SSH verification, signing, hooks, cloning, and directory rules. The repository has cross-platform CI and a passing Rust test suite.

## Product Principles

- Make the active and expected identity visible before mutation.
- Prefer repository-local and directory-scoped configuration over global switching.
- Require explicit confirmation for external or destructive state changes.
- Fail closed when identity cannot be verified, while explaining the remediation.
- Keep credentials delegated to GitHub CLI, credential helpers, and OpenSSH.

## Accessibility & Inclusion

The desktop interface targets WCAG 2.2 AA, full keyboard operation, visible focus, reduced-motion preferences, system light/dark themes, and status communication that never relies on color alone.
