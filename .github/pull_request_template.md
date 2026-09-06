## What changed

<!-- One or two sentences. Link the issue if there is one. -->

## Why

<!-- The problem this solves. Skip if it is obvious from the title. -->

## Checks

- [ ] Target branch is `develop` (only a release merges into `main`)
- [ ] `cargo fmt --check`
- [ ] `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- [ ] `cargo test --workspace --all-features`
- [ ] `dprint check` — if anything under `desktop/ui/` changed

## Frontend changes only

- [ ] New DOM is built with `h()`; no `innerHTML` or other markup parsing
- [ ] Every new `input`, `select`, and `textarea` carries a `data-k`
- [ ] New icons exist as a `<symbol>` in `index.html`, and `NOTICE` is updated
      if they come from a new source
- [ ] Relevant `docs/QA_CHECKLIST.md` items were walked by hand

## IPC changes only

- [ ] The command is in both `generate_handler!` and `desktop/ui/js/ipc.js`
- [ ] Argument names were checked by eye — the contract test covers command
      names, not arguments
