# Release QA checklist

Walk this before tagging a release.

It exists because the frontend has no JavaScript test runner. `cargo test`
covers the backend, the IPC contract, and the frontend's structural invariants,
but not DOM behaviour — focus handling, confirmation gating, and toast timing.
Those are checked here, by hand.

Run against a **throwaway configuration**, not your real one:

```sh
# Windows PowerShell
$env:GITBOUND_CONFIG = "$env:TEMP\gitbound-qa\config.toml"
cargo run -p gitbound-desktop
```

Create two or three scratch git repositories to bind against. Do not point this
at repositories whose git config you care about.

---

## A. First run and shell

1. With no configuration file present, the onboarding view appears — not the
   main views — and offers "Import from a repository" first.
2. "Import from a repository" opens the native picker. Cancelling it leaves the
   chooser unchanged.
3. Picking a git repository opens the wizard on step 1 with author, email, and
   host prefilled from that repository, and shows the chosen path above it.
4. "Start from scratch instead" opens the same wizard with empty fields and no
   path.
5. Completing the wizard lands on the Dashboard with the identity present.
   Started from a repository, that repository is also bound; started from
   scratch, **no repository is touched** — verify with `git config --local -l`.
6. The title bar shows the correct version, the Quick switch trigger, and
   "Local configuration only".
7. All six navigation entries switch views, and the active one is highlighted.
8. **No console window appears at any point** — not at launch, and not during
   any action below.

## B. Theme and layout

9. Switch the OS to dark mode with Settings on "System": the app follows
   without a restart.
10. In Settings, choose Light. The app stays light with the OS in dark mode.
    Restart: the choice survives.
11. Choose the Violet accent. Brand surfaces change; **passing checks stay
    green** and failures stay red. Choosing Dark afterwards keeps the accent,
    and choosing an accent keeps the mode.
12. Narrow the window below 860px: navigation collapses to an icon rail, the
    footer keeps the identity avatar, and the window does not scroll
    horizontally. Wide tables scroll inside their own container.
13. Narrow below 620px: cards and grids go to one column; nothing becomes
    unreachable.
14. With "reduce motion" enabled in the OS, page transitions and the spinner do
    not animate.

## C. Dashboard

15. The Active identity card shows name, author, email, `@user`, host, and
    either the SSH key path or "HTTPS · credential helper".
16. The Current repository card shows the selected repository and its status
    badge. With no repositories discovered it offers "Go to repositories"
    instead.
17. "Run checks" contacts GitHub CLI and SSH **only when pressed** — nothing
    reaches the network on page load. Afterwards "Last checked" appears.
18. "Check CI status" is likewise never automatic. With `gh` unavailable it
    reports the reason in place rather than raising an error toast.
19. The three shortcut cards navigate to Quick switch, Repositories, and
    Diagnostics.
20. Recent repositories lists up to five rows; clicking one opens its detail
    page.

## D. Quick Switch

21. `Ctrl`/`Cmd`+`K` opens the palette from any view. `Escape` closes it. The
    trigger in the title bar does the same.
22. **Type in the search box.** Focus and caret survive every keystroke.
23. Clicking an identity arms it and shows a preview of exactly what will
    change. **Nothing is written yet** — verify with `git config --local -l`.
24. Clicking the same identity again, or pressing Switch, applies it.
25. With "Current repository" selected and no repository chosen, the preview
    says so and applying reports it rather than failing silently.
26. With "GitHub CLI account" selected, the preview names the account gh will
    authenticate as, and confirms no repository is modified. Verify with
    `gh auth status`.
27. The palette does not open before any identity exists.

## E. Identities

28. The grid shows one card per identity with avatar initials, author, email,
    `@user`, SSH key, and a repository count that matches the Repositories
    table.
29. The first identity is marked Default and carries the star.
30. **Type in the search box.** Focus and caret survive every keystroke, and
    filtering narrows the grid.
31. "Use" shows the GitHub CLI switch preview with the real current account.
    Confirming switches it; cancelling does nothing.
32. "Edit" opens the wizard with every field populated, including the signing
    format and the "require signed commits" checkbox.
33. **Tab through the entire wizard using only the keyboard.** No field loses
    focus and the caret never jumps.
34. Step 1 refuses to advance without a name, author, and a plausible email,
    and says which is missing. Step 2 refuses without a GitHub user and host.
    Step 3 is optional and can be skipped entirely.
35. Step 4 shows exactly what will be saved and states that nothing is written
    yet.
36. Changing the signing format updates the signing-key placeholder.
37. "Allowed owners" accepts `a, b, ,c` and stores three owners.
38. "Browse" opens the key picker and fills the SSH key path.
39. Renaming an identity and saving renames it and keeps the grid correct.
40. "…" → Duplicate produces `<name>-copy`; duplicating again produces
    `<name>-copy-2`.
41. "…" → Remove requires confirmation on the card before removing.

## F. Repositories

42. With no folder approved, the empty state explains that nothing on disk is
    scanned and offers "Add path".
43. "Add path" opens the folder picker and then scans.
44. "Scan" streams live progress text, then reports the count in a toast.
    During a scan the button reads "Cancel scan"; pressing it stops the scan.
45. **Type in the search box.** Focus stays and the caret stays put across every
    keystroke, including typing in the middle of existing text. _(This is the
    most important check on the page.)_
46. The table shows repository, identity pill, remote host, status, and CI.
47. "Check CI" fills the CI column one repository at a time and is **never**
    automatic. Repositories without workflows read "unavailable" with the
    reason on hover, not an error.
48. Clicking a repository name or its chevron opens the detail page.
49. "Manage approved paths" goes to Settings.

## G. Repository details

50. The header shows the repository name and path, with Open folder and Back.
51. "Open folder" reveals the repository in the OS file manager. It does **not**
    open a shell.
52. The remote row shows the URL; the copy button puts it on the clipboard.
53. The Identity select lists every identity. Commit identity and Authentication
    are read-outs derived from that selection, not separately editable.
54. For an unbound repository "Apply configuration" first shows the preview and
    changes to "Confirm and write". **No git config is written until the second
    click** — verify with `git config --local -l`.
55. For a drifted repository the preview adds the sentence about drifted managed
    values being overwritten.
56. After binding, `git config --local -l` shows `user.name`, `user.email`,
    `gitbound.profile`, and the `gitbound.backup*` entries.
57. "Reset" returns the select to the currently bound identity.
58. "Unbind and restore" requires its own confirmation, and after confirming,
    the repository's original `user.name`/`user.email` are restored exactly —
    including being absent again if they were absent before.
59. "Run checks" fills the Checks card and notes when the network was not
    contacted.
60. A repository with drift shows the dot on the Repositories nav entry.

## H. SSH keys

61. Selecting an identity with no SSH key disables "Test authentication" and
    shows the "No SSH identity file" notice.
62. Selecting one with a key enables the button.
63. Running the test makes exactly one SSH connection and shows the verdict.
64. Switching identities clears the previous verdict rather than showing a
    stale one.

## I. Diagnostics

65. The report shows git, gh, and ssh with real versions.
66. Temporarily rename `gh.exe` on PATH: that row becomes unavailable, the
    banner switches to "needs attention", and a "Fix" button appears.
67. "Fix" reveals the remediation command and **does not run it**.
68. The copy button puts the command on the clipboard.
69. The "Recommended" callout names the most serious problem, and "Show command"
    reveals the same command.
70. "Run again" re-runs the checks.

## J. Settings

71. Approved folders lists every approved root.
72. Removing one asks for confirmation first, and after confirming, repositories
    beneath it disappear from the table.
73. The About panel shows the version, configuration path, schema version, and
    identity count, and the copy button copies the path.

## K. Failure and recovery

74. Corrupt the config file (write `not toml` into it). The app shows
    "Configuration could not be read" and offers only Retry and Diagnostics —
    **no write actions are offered.**
75. Diagnostics is still reachable from that state, and `Ctrl`/`Cmd`+`K` does
    not open the palette.
76. Fixing the file and pressing Retry recovers without a restart.
77. Error toasts persist until dismissed; success toasts disappear after about
    three and a half seconds.

## L. CI surface

78. `gitbound verify` in a bound repository exits 0; in an unbound one it
    exits 1 and names the `binding` check.
79. `gitbound verify --format sarif --output out.sarif` writes valid SARIF and
    still exits on the same rule.
80. With `GITHUB_ACTIONS=true` and `GITHUB_STEP_SUMMARY` set, `verify` prints
    `::error` annotations and appends a table to the summary file.
81. `gitbound audit --range HEAD~3..HEAD` names any commit authored outside
    policy.
82. A `.gitbound.toml` with `schema_version = 99` makes `verify` exit 2 rather
    than ignoring the policy.
83. `gitbound --json` output is unchanged from the previous release for
    `status`, `check`, `doctor`, `profile list`, `profile show`, and `ssh test`.

## M. Packaging

84. `cargo build --release -p gitbound-desktop` succeeds with no Node and no
    tauri CLI on PATH.
85. The release executable runs on a clean machine that has only the WebView2
    runtime.
86. `?demo` does nothing in the release build.
87. Every published asset has a matching `.sha256`, and
    `gh attestation verify` succeeds against the release.
