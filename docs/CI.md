# GitBound in continuous integration

Everywhere else, GitBound answers "is this working copy configured correctly
right now". That is the right question on a laptop and the wrong one in a
pipeline, where the working copy is a fresh clone with no bindings.

So CI mode asks two questions instead:

1. **`gitbound verify`** — the ordinary identity checks, rendered for whatever
   is going to read them, plus any policy the repository itself commits.
   Because a pipeline checkout has no binding, `verify` reports the absent
   binding as `unverified` and does not fail the run over it; the committed
   policy is what decides. `check`, which runs on the machine that owns the
   binding, still treats an unbound repository as a failure.
2. **`gitbound audit`** — who actually authored the commits in a range, which
   is evidence a developer's machine does not have and a pipeline does.

Nothing here changes the exit-code contract:

| Code | Meaning                                                               |
| ---- | --------------------------------------------------------------------- |
| `0`  | Everything checked passed.                                            |
| `1`  | An identity or policy check failed, or could not be verified.         |
| `2`  | Invalid input or configuration — including an unreadable policy file. |
| `3`  | A required dependency was missing or a subprocess failed.             |

`2` and `3` mean the gate did not run. Treat them as failures even when you are
willing to tolerate a `1`.

## The GitHub Action

```yaml
- uses: BitKiln/GitBound@v1
  with:
    version: "1.0.0"
    command: both
    sarif-file: gitbound.sarif
```

The action downloads a released binary for the runner's platform, **verifies its
SHA-256 before extracting it**, and runs it. There is no Rust toolchain step and
no container pull. A complete workflow is in
[`docs/examples/gitbound-ci.yml`](examples/gitbound-ci.yml).

Pin `version`. `latest` means a new GitBound release can change what your gate
accepts without a commit in your repository.

`audit` compares a range, so `actions/checkout` needs `fetch-depth: 0`; with the
default shallow clone the base commit is not present and the range cannot be
resolved.

### Inputs

| Input               | Default                 | Notes                                                   |
| ------------------- | ----------------------- | ------------------------------------------------------- |
| `version`           | `latest`                | Release tag without the leading `v`.                    |
| `command`           | `verify`                | `verify`, `audit`, or `both`.                           |
| `range`             | PR commits, else `HEAD` | Revision range for `audit`.                             |
| `format`            | `auto`                  | stdout format. `auto` picks `github` in a runner.       |
| `policy`            | committed file          | Path to a policy file.                                  |
| `no-policy`         | `false`                 | Ignore the committed policy file.                       |
| `require-policy`    | `false`                 | Fail when the repository commits no policy.             |
| `offline`           | `false`                 | Skip the checks that need the network. `verify` only.   |
| `enforce-signing`   | `false`                 | Fail when signing is not enabled.                       |
| `sarif-file`        | —                       | Also write SARIF here, for `upload-sarif`.              |
| `fail-on`           | `failure`               | `warning` also fails on warnings; `never` reports only. |
| `working-directory` | `.`                     | Repository to inspect.                                  |

Outputs: `overall` (`ok`/`warning`/`failure`), `report` (path to JSON), `sarif`.

## Report formats

`--format` applies to stdout. `--output` writes files, is repeatable, and each
entry may carry its own format prefix — so one inspection can feed several
consumers without running the checks twice:

```console
gitbound verify \
  --format github \
  --output sarif:gitbound.sarif \
  --output json:report.json
```

A bare path uses the stdout format. A drive letter is not mistaken for a format
prefix, so `--output C:\reports\out.sarif` works on Windows.

| Format     | For                                                                                                |
| ---------- | -------------------------------------------------------------------------------------------------- |
| `auto`     | `github` inside a runner, `human` elsewhere. The default.                                          |
| `human`    | The same output the CLI has always produced.                                                       |
| `json`     | The `CheckReport` structure, unchanged from `--json`.                                              |
| `github`   | `::error` / `::warning` / `::notice` annotations, plus a table appended to `$GITHUB_STEP_SUMMARY`. |
| `sarif`    | SARIF 2.1.0 for `github/codeql-action/upload-sarif`.                                               |
| `junit`    | JUnit XML for test-report viewers.                                                                 |
| `markdown` | A table, for a PR comment.                                                                         |

`--json` still means `--format json` and takes precedence over the runner
environment, so existing scripts are unaffected.

## Repository policy

A repository can commit a `.gitbound.toml` declaring who it accepts commits
from. This is the only part of GitBound that reads a file belonging to the
repository rather than to the user.

```toml
schema_version = 1

[identity]
allowed_email_domains = ["company.example"]
allowed_emails = ["release-bot@ci.example"]
deny_noreply = true
require_author_matches_committer = false

[remote]
allowed_hosts = ["github.com"]
allowed_owners = ["company-name"]

[signing]
require = true
format = "ssh"
```

Three properties hold, and are worth stating plainly because this file is the
one place GitBound touches shared repository state:

- **Policy only.** Allowed addresses, hosts, owners, signing requirements. Never
  a token, never key material, never a path outside the repository.
- **Read-only.** Nothing in GitBound writes this file. There is no
  `gitbound policy init`.
- **Opt-in.** No file means behaviour is exactly what it was without this
  feature. `--no-policy` disables it explicitly; `--policy <path>` points at a
  different file, and a named file that is missing is an error rather than a
  silent skip.

Being opt-in cuts both ways in a gate: a repository with no policy has no rule
to break, so `verify` passes it. That is the correct answer to the question
asked, but it is not usually the question a pipeline means — and a pull request
that deletes `.gitbound.toml` would switch the gate off silently. Pass
`--require-policy` (or `require-policy: "true"` to the action) to make an absent
or empty policy a failure in its own right.

An empty allowlist means "no restriction", matching how a profile's
`allowed_owners` already behaves. `deny_noreply` applies even so, because a
`users.noreply.github.com` address hides who authored a commit behind an account
alias.

An unknown `schema_version` fails closed with exit code `2`. A future GitBound
may add rules this build would silently ignore, and silently ignoring a rule in
a gate is worse than refusing to run.

### What the policy adds to a report

| Check id                | Fails when                                               |
| ----------------------- | -------------------------------------------------------- |
| `policy_present`        | `--require-policy` is set and no policy declares a rule. |
| `policy_email`          | The configured author address is outside the allowlist.  |
| `policy_host`           | The remote host is not in `allowed_hosts`.               |
| `policy_owner`          | The remote owner is not in `allowed_owners`.             |
| `policy_signing`        | Signing is required but not enabled.                     |
| `policy_signing_format` | The signing format differs from the required one.        |

## Auditing a range

```console
gitbound audit --range origin/main..HEAD --format github
```

Per commit, this reports:

| Check id           | Fails when                                                                                          |
| ------------------ | --------------------------------------------------------------------------------------------------- |
| `commit_author`    | The author address is outside policy, and is not the locally bound profile's own address.           |
| `commit_committer` | `require_author_matches_committer` is set and the two differ.                                       |
| `commit_signature` | `--enforce-signing` or `[signing] require` is set and the commit has no signature Git could verify. |
| `commit_range`     | Informational: the range was clean, empty, or truncated.                                            |

Git's `%G?` code decides the signature question. `G` (good) and `U` (good, key
untrusted) count as signed; `N`, `B`, `X`, `Y`, `R`, and `E` do not, because a
signature that cannot be trusted is not evidence.

The range is validated before it reaches Git — anything beginning with `-` is
refused — so a range cannot smuggle in extra `git log` options.

`--max-commits` bounds the work and defaults to 1000. When a range is truncated
the report says so rather than quietly passing.

## Running it without the action

```console
cargo binstall gitbound          # or: cargo install gitbound
gitbound verify --format junit --output junit:results.xml
```

GitBound still needs `git` on `PATH`. `gh` and `ssh` are only needed for the
network-dependent checks; without them those report `unverified`, which `verify`
treats as a failure.

Pass `--offline` (or `offline: "true"` to the action) when the runner cannot
answer them — no egress, or no GitHub CLI credentials. It drops `github_cli` and
`ssh_identity` and nothing else: `credential_helper` reads `git config`, so it
still runs, as does every identity, signing, transport and policy check. This is
a real narrowing of the gate, so prefer providing `gh` where you can; the point
of the flag is that the alternative was not running the gate at all.

`audit` needs Git alone and has no network checks, so it does not take the flag
and the action passes it only to `verify`.
