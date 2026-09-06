# Building GitBound Desktop

GitBound Desktop is Rust and Tauri 2 with a hand-written HTML/CSS/JavaScript
frontend. **There is no Node, no npm, and no bundler.** The frontend is a set of
static files under `desktop/ui/` that are checked into the repository and
embedded into the executable at compile time.

## Requirements

- Rust 1.88 or newer (edition 2024)
- On Windows: the WebView2 runtime, which ships with Windows 11 and with
  Windows 10 since 2021
- On Linux: `libwebkit2gtk-4.1-dev`, `libappindicator3-dev`, `librsvg2-dev`,
  `patchelf`

Nothing else. `cargo build` is the whole toolchain.

## Everyday commands

```sh
# Run the desktop application
cargo run -p gitbound-desktop

# Run the CLI
cargo run --bin gitbound -- doctor

# Everything CI checks
cargo fmt --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```

## Building a release binary

```sh
cargo build --release -p gitbound-desktop
```

That alone produces a complete, standalone `target/release/gitbound-desktop.exe`.
`tauri::generate_context!` embeds `desktop/ui/` into the binary, so the
executable needs no companion files.

## Building installers

Bundling `.msi` and NSIS installers is the only step that needs the Tauri CLI,
which is itself installed through cargo:

```sh
cargo install tauri-cli --version "^2" --locked
cd desktop
cargo tauri build --bundles msi,nsis
```

Artifacts land in:

| Artifact | Path                                                          |
| -------- | ------------------------------------------------------------- |
| Portable | `target/release/gitbound-desktop.exe`                         |
| NSIS     | `target/release/bundle/nsis/GitBound_<version>_x64-setup.exe` |
| MSI      | `target/release/bundle/msi/GitBound_<version>_x64_en-US.msi`  |

Released artifacts are **unsigned**. SmartScreen and Gatekeeper will warn about
them; the published SHA-256 is the verification path. See `SECURITY.md`.

## Working on the frontend

> **Read this before you lose an hour.** Cargo does not reliably re-run the
> asset embedding when only a file under `desktop/ui/` changes, so
> `cargo run` can happily launch a stale UI.

Use the Tauri dev server, which serves `desktop/ui/` directly and reloads on
save:

```sh
cd desktop
cargo tauri dev
```

If you would rather use `cargo run`, touch the Rust entry point first to force
the rebuild:

```sh
# Windows PowerShell
(Get-Item desktop/src-tauri/src/main.rs).LastWriteTime = Get-Date
cargo run -p gitbound-desktop
```

### Frontend layout

```
desktop/ui/
├── index.html          shell + the Lucide <symbol> sprite
├── css/app.css         the stylesheet, unchanged from the React version
└── js/
    ├── main.js         boot, title bar, navigation, toasts, demo gate
    ├── dom.js          h() and createView() - the entire render layer
    ├── ipc.js          the Tauri command surface
    ├── icons.js        <use> factory over the sprite
    ├── status.js       backend status -> severity
    ├── components.js   pieces shared by more than one view
    └── views/          one file per view
```

### Four rules the frontend depends on

These are asserted by `desktop/src-tauri/src/contract.rs` and by CI, so breaking
one fails the build rather than shipping.

1. **Build DOM only with `h()`.** No `innerHTML`, `outerHTML`,
   `insertAdjacentHTML`, `document.write`, `eval`, or `new Function`. This is
   what makes untrusted strings — repository paths, author names, raw `git`
   stderr — structurally unable to become markup. There is no escaping helper
   because no call site needs one.
2. **Every `input`, `select`, and `textarea` needs a stable `data-k`.** Each
   keystroke rebuilds its whole view; `data-k` is how the caret and focus are
   restored. Omit it and the field feels broken.
3. **No inline `<script>` and no external assets.** The content security policy
   is `script-src 'self'` with `default-src 'self'`. Everything is same-origin
   and same-document, including the icon sprite.
4. **No inline styles.** `style-src 'self'` blocks them, which is why `h()` has
   no `style` prop. Styling comes from `css/app.css` via `class`.

### Adding an icon

Copy the inner elements of the icon's `.svg` from
[Lucide](https://github.com/lucide-icons/lucide/tree/0.468.0/icons) into a new
`<symbol id="i-NAME" viewBox="0 0 24 24">` in `index.html`, then call
`icon("NAME", size)`. Do not put presentation attributes on the `<symbol>`;
they are set on the outer `<svg>` by `icons.js` so they inherit across the
`<use>` boundary. Update `NOTICE` if you add icons from a new source.

### Adding a command

1. Write the `#[tauri::command]` in `desktop/src-tauri/src/main.rs` and add it
   to the `generate_handler!` list.
2. Add the matching entry to `desktop/ui/js/ipc.js`.

`cargo test` fails if you do one without the other. Note that it checks command
_names_, not argument names — Tauri maps camelCase JavaScript keys onto
snake_case Rust parameters, and getting that wrong is not caught automatically.

### Demo fixtures

`cargo run -p gitbound-desktop` accepts `?demo` for documentation
screenshots. The fixture document lives in `desktop/src-tauri/src/demo.rs`
behind `#[cfg(debug_assertions)]`, so a release build answers `null` and none of
those bytes are in the shipped executable.

## Testing

`cargo test --workspace --all-features` runs everything:

- `tests/cli.rs` — the CLI integration suite, which covers the core
- `desktop/src-tauri/src/main.rs` — folder-authorization unit tests
- `desktop/src-tauri/src/contract.rs` — the frontend/backend contract

There is deliberately **no JavaScript test runner**; adding one would put Node
back into CI. DOM-level behaviour is covered by `docs/QA_CHECKLIST.md`, which is
walked before tagging a release.

## Formatting

```sh
cargo fmt
cargo install dprint --locked && dprint fmt   # JS, CSS, JSON, Markdown
```
