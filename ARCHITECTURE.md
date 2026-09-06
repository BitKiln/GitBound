# Architecture

This document is for people about to change GitBound. If you only want to use
it, read the [user guide](docs/USER_GUIDE.md) instead; if you are about to open
a pull request, read [CONTRIBUTING.md](CONTRIBUTING.md) as well.

## The shape of the thing

GitBound is one Rust library with two front doors:

```text
gitbound (CLI)              gitbound-desktop (Tauri 2)
src/main.rs                   desktop/src-tauri/ + desktop/ui/
     |                                  |
     v                                  v
src/app.rs                        src/service.rs
     |                                  |
     +----------------+-----------------+
                      v
           the library in src/*.rs
                      |
                      v
              src/process.rs
                      |
                      v
                git . gh . ssh
```

Both front doors share every decision. Neither reimplements a check, and neither
is allowed to. If a behaviour differs between the CLI and the desktop app, that
is a bug rather than a design.

## Crates

There are two, in one workspace.

**`gitbound`** — the root crate, at `src/`. It is both the library
(`src/lib.rs`) and the CLI binary (`src/main.rs`). Everything that decides
anything lives here.

**`gitbound-desktop`** — at `desktop/src-tauri/`. A Tauri 2 shell that depends
on `gitbound` and adds no logic of its own. Its frontend is hand-written
static HTML, CSS, and JavaScript under `desktop/ui/`, embedded into the binary
at compile time by `tauri::generate_context!`. There is no Node, no npm, and no
bundler; `cargo` is the entire toolchain.

## The library, by layer

### Entry points

| Module       | Role                                                                                                                                          |
| ------------ | --------------------------------------------------------------------------------------------------------------------------------------------- |
| `cli.rs`     | The clap command tree. Argument shapes and nothing else.                                                                                      |
| `app.rs`     | The CLI's dispatcher: one arm per subcommand, printing human output and returning an exit code. The largest module, and deliberately shallow. |
| `service.rs` | The same operations as `app.rs`, returning values instead of printing them. This is what the Tauri commands call.                             |
| `api.rs`     | The data transfer types that cross the IPC boundary — `NamedProfile`, `RepositoryStatus`, `DoctorReport`, `SshTestReport`, and friends.       |

`app.rs` and `service.rs` are peers. Adding a capability usually means touching
both, and forgetting one is the most common way for the two front doors to
drift apart.

### Decisions

| Module      | Role                                                                                                                                  |
| ----------- | ------------------------------------------------------------------------------------------------------------------------------------- |
| `check.rs`  | The check engine. Produces a `CheckReport` and is the only place that decides whether an identity is `ok`, `failed`, or `unverified`. |
| `policy.rs` | The optional repository-committed `.gitbound.toml`. Read-only, always opt-in, policy only — never credentials.                        |
| `config.rs` | The user's own configuration: profiles, approved roots, schema version and migrations, and the file lock that guards writes.          |
| `audit.rs`  | Who actually authored a revision range. The question a pipeline can answer and a laptop cannot.                                       |
| `report.rs` | Rendering an existing `CheckReport` as human text, JSON, SARIF, JUnit, GitHub workflow commands, or Markdown. Adds no checks.         |

The split between `check.rs` and `report.rs` is load-bearing: a new output
format must never be able to change a verdict.

### Effects

| Module          | Role                                                                                                         |
| --------------- | ------------------------------------------------------------------------------------------------------------ |
| `git.rs`        | Reading and writing Git configuration, including the snapshot that makes `unbind` exact.                     |
| `github.rs`     | Everything that shells out to `gh`: accounts, switching, workflow runs.                                      |
| `ssh.rs`        | One SSH connection, on request, to ask GitHub who it thinks you are.                                         |
| `remote.rs`     | Parsing and building remote URLs across SSH, HTTPS, and host aliases.                                        |
| `repository.rs` | Discovering repositories beneath an approved root, and summarising one.                                      |
| `directory.rs`  | Native Git `includeIf` rules and the marked fragments they point at.                                         |
| `hooks.rs`      | The opt-in pre-commit and pre-push hooks, recognised by a deliberately version-free marker.                  |
| `clone_repo.rs` | Clone and bind as one operation, restoring the previous account if any step fails.                           |
| `process.rs`    | The subprocess boundary. See below — this one is a security control, not plumbing.                           |
| `error.rs`      | `GitBoundError` and the exit-code contract: `0` ok, `1` check failed, `2` bad input, `3` missing dependency. |

## Two invariants worth knowing before you start

### Every subprocess goes through `Runner`

`process.rs` defines a `Runner` trait with `run_git` and `run_git_in`, and a
`SystemRunner` that implements it. They strip `GIT_DIR`, `GIT_WORK_TREE`,
`GIT_CONFIG_COUNT` and related variables from the child environment, because
those take precedence over the working directory and would silently redirect a
write into a repository the user did not choose.

Calling `run("git", …)` directly reopens that hole. `Runner` is also what makes
the library testable: the test suite substitutes a fake and asserts on the
commands that would have run.

### The frontend/backend contract is enforced by a test, not a compiler

The frontend is plain JavaScript, so nothing checks at build time that it still
matches the serde types it consumes. `desktop/src-tauri/src/contract.rs` stands
in for that: it asserts that every command in `generate_handler!` is invoked
from `ui/js/ipc.js` and vice versa, that every backend status variant has an
entry in `ui/js/status.js`, that no frontend file parses markup, that every form
control carries a stable `data-k`, and that every icon the UI renders exists in
the sprite.

Two gaps are deliberate and documented in
[CONTRIBUTING.md](CONTRIBUTING.md#the-frontendbackend-contract): argument names,
and field-level drift in the DTOs. Renaming a field in `src/api.rs` will not
fail a test — the JavaScript will simply read `undefined`.

## The frontend

`desktop/ui/js/` is small and has no framework.

- `dom.js` — `h()`, the **only** DOM constructor in the codebase. `innerHTML`
  appears nowhere, which is what the project's XSS guarantee rests on.
- `main.js` — application state, routing, the sidebar, and `reload()`.
- `ipc.js` — every `invoke` call, in one file, so the contract test can read it.
- `views/` — one module per screen.
- `components.js`, `icons.js`, `status.js`, `theme.js` — shared pieces.

The visual language those views implement is specified separately, in
[docs/DESIGN_SYSTEM.md](docs/DESIGN_SYSTEM.md).

## Tests

- Unit tests live beside the code they cover, in `#[cfg(test)]` modules.
- `tests/cli.rs` drives the built binary and asserts on output and exit codes.
- `tests/ci.rs` covers `verify`, `audit`, and the report formats.
- `desktop/src-tauri/src/contract.rs` holds the frontend invariants above.

All of it runs on Windows, macOS, and Linux in CI, which is why pull requests
are the only route into `develop`.
