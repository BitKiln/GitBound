# Contributing to GitBound

Thanks for helping make multiple-account Git workflows safer.

Everyone taking part is expected to follow our
[Code of Conduct](CODE_OF_CONDUCT.md).

New to the codebase? [ARCHITECTURE.md](ARCHITECTURE.md) explains the crate
layout and the two invariants — the subprocess boundary and the
frontend/backend contract — that most surprise people on a first change.

## Ground rules

1. Open an issue for significant behavioral or configuration-format changes.
2. Keep authentication delegated to GitHub CLI, the operating-system credential store, or OpenSSH. Code that reads or stores tokens is out of scope.
3. Add tests for changes to profile validation, Git configuration, remote parsing, account checks, or hook behavior.
4. Run formatting, Clippy with warnings denied, and the complete test suite before submitting a pull request.
5. Avoid changing existing hooks, global Git identity settings, SSH configuration, or repository remotes as a side effect.
6. Route every `git` subprocess through `Runner::run_git` or `Runner::run_git_in`. They strip the environment variables that would redirect Git away from the selected repository; calling `run("git", ...)` directly reopens that hole.
7. Never build DOM in `desktop/ui/` except through `h()`, and give every `input`, `select`, and `textarea` a stable `data-k`. Both rules are asserted by `desktop/src-tauri/src/contract.rs`; see "The frontend/backend contract" below for why they matter.

## Branches

| Branch      | Purpose                                                                   |
| ----------- | ------------------------------------------------------------------------- |
| `main`      | Released code. Only ever updated by merging `develop`. Tags are cut here. |
| `develop`   | Integration branch. The default target for pull requests.                 |
| `feature/*` | One branch per change, cut from `develop`.                                |

Also in use where they help: `fix/*` for bug fixes, `docs/*` for
documentation-only changes, and `release/*` when a release needs stabilising
before it reaches `main`.

**Feature branches merge through pull requests only.** Pushing directly to
`develop` or `main` bypasses CI, which is where the contract tests in
`desktop/src-tauri/src/contract.rs` run on all three platforms.

Both branches are protected on the remote, and the protection is what enforces
the model rather than convention:

- a pull request is required, and it must be up to date with its base
- the ten checks that run on every pull request must all pass -- the three
  `test` matrix jobs, `format`, `msrv`, `coverage`, `action-lint`,
  `frontend-invariants`, and both CodeQL `Analyze` jobs
- history stays linear, so merges are squash or rebase; merge commits are
  rejected
- force pushes and branch deletion are refused
- review conversations must be resolved before merging

`consume-run-ci` is deliberately **not** a required check: it skips on ordinary
pull requests, and requiring a check that does not always run would block every
merge.

```console
git switch develop
git pull
git switch -c feature/short-description
# ... work, commit ...
git push -u origin feature/short-description
gh pr create --base develop
```

Squash on merge, so `develop` keeps one commit per change. Delete the branch
afterwards.

## Commit messages

Commits follow [Conventional Commits](https://www.conventionalcommits.org):
`feat:`, `fix:`, `docs:`, `build:`, `ci:`, `refactor:`, `test:`, `perf:`, and
`chore:`. Because pull requests are squashed, it is the **pull request title**
that becomes the commit on `develop` -- so that is the line that has to be
right.

Write the body to explain why, not what. The diff already says what changed; it
cannot say which wrong behaviour you observed, or why the obvious fix was not
the one you took.

## Running CI

Continuous integration runs automatically when a pull request is opened,
updated, or reopened. To re-run it without pushing a commit, add the `run-ci`
label; the workflow removes the label once it has consumed it.

Documentation links are checked too, by `lychee` against `lychee.toml`. A moved
file breaks a relative link silently, so that job exists to catch what nothing
else reads.

Before submitting, run what CI runs:

```console
cargo fmt --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo deny check
cargo build --release -p gitbound-desktop
```

## The frontend/backend contract

The frontend is plain JavaScript, so there is no compile-time check that it
still matches the serde types it consumes. `desktop/src-tauri/src/contract.rs`
replaces the part of that coupling which actually caught bugs:

- every command in `generate_handler!` is invoked from `ui/js/ipc.js`, and
  vice versa
- every backend status variant has an entry in `ui/js/status.js`
- no frontend file parses markup, every form control carries a `data-k`, and
  every icon the UI renders exists in the sprite

Two gaps are deliberate and are not caught automatically:

- **Argument names.** Tauri maps camelCase JavaScript keys onto snake_case Rust
  parameters. The contract test checks command names, not argument names.
- **Field-level DTO drift.** If a field is renamed in `src/api.rs`, the
  JavaScript will read `undefined` rather than failing. Changing a type that
  crosses the IPC boundary means grepping `desktop/ui/js` for its fields.

See [Building the desktop application](docs/BUILDING_DESKTOP.md) for the four
rules the frontend depends on.

## Security

Report vulnerabilities privately - see [SECURITY.md](SECURITY.md). A check that
reports `ok` when the identity is actually wrong is a security bug, not a
correctness nit.

All contributions are licensed under the MIT License.
