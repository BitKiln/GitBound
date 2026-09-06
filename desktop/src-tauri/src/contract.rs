//! Tests that hold the hand-written frontend to the Rust API it talks to.
//!
//! The old React frontend got this coupling from TypeScript: `types.ts`
//! mirrored the serde types and `api.ts` was checked against them at compile
//! time. Removing the npm toolchain removed that check, so these tests replace
//! the part of it that actually caught bugs — a command or an enum variant
//! changing on the Rust side while the JavaScript kept using the old name.
//!
//! They are plain string checks over `include_str!`, so they need no runtime,
//! no browser, and no test framework beyond `cargo test`.

/// The command names registered with Tauri, read out of the `generate_handler!`
/// list in `main.rs`. Entries may be paths (`demo::demo_fixtures`); only the
/// final segment is the command name Tauri exposes.
fn registered_commands(main_rs: &str) -> Vec<String> {
    let list = main_rs
        .split("generate_handler![")
        .nth(1)
        .expect("main.rs should contain a generate_handler! list")
        .split(']')
        .next()
        .expect("the generate_handler! list should be closed");
    list.split(',')
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .map(|entry| entry.rsplit("::").next().unwrap_or(entry).to_string())
        .collect()
}

#[test]
fn every_registered_command_is_reachable_from_the_ui() {
    let commands = registered_commands(include_str!("main.rs"));
    let ipc = include_str!("../../ui/js/ipc.js");
    assert!(
        commands.len() >= 20,
        "expected the full command surface, parsed only {commands:?}"
    );
    for name in commands {
        assert!(
            ipc.contains(&format!("\"{name}\"")),
            "command `{name}` is registered in main.rs but never invoked from ui/js/ipc.js"
        );
    }
}

#[test]
fn the_ui_does_not_invoke_commands_that_do_not_exist() {
    let commands = registered_commands(include_str!("main.rs"));
    let ipc = include_str!("../../ui/js/ipc.js");
    // Every `invoke("name"` in the IPC layer must name a registered command.
    for (index, _) in ipc.match_indices("invoke(\"") {
        let rest = &ipc[index + "invoke(\"".len()..];
        let name = rest.split('"').next().unwrap_or_default();
        assert!(
            commands.iter().any(|command| command == name),
            "ui/js/ipc.js invokes `{name}`, which main.rs does not register"
        );
    }
}

/// Every status string that can cross the IPC boundary, from
/// `RepositoryLocalStatus`, `CheckStatus`, `DependencyState` and
/// `SshTestStatus`. The UI maps these onto three severities; a variant it does
/// not know about would silently render as a failure.
const BACKEND_STATUS_VARIANTS: &[&str] = &[
    // RepositoryLocalStatus
    "bound",
    "unbound",
    "drifted",
    "missing_profile",
    "unavailable",
    // CheckStatus
    "ok",
    "warning",
    "failure",
    "unverified",
    // SshTestStatus
    "verified",
    "rejected",
    // CiConclusion. `failure` and `unknown` are shared with the enums above.
    "success",
    "cancelled",
    "skipped",
    "running",
    "unknown",
];

#[test]
fn every_backend_status_appears_in_the_ui_severity_table() {
    let table = include_str!("../../ui/js/status.js");
    for variant in BACKEND_STATUS_VARIANTS {
        assert!(
            table.contains(&format!("\"{variant}\"")),
            "ui/js/status.js has no severity entry for the backend status `{variant}`"
        );
    }
}

/// `missing_profile` -> `MissingProfile`. The enums carry
/// `#[serde(rename_all = "snake_case")]`, so the wire string and the Rust
/// identifier differ and the guard below has to bridge them.
fn to_camel_case(snake: &str) -> String {
    snake
        .split('_')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().chain(chars).collect::<String>(),
                None => String::new(),
            }
        })
        .collect()
}

#[test]
fn the_status_variants_this_test_guards_still_exist_in_the_rust_source() {
    // Guards the guard: if a variant is renamed in Rust, the list above goes
    // stale and would keep passing against a UI that is equally stale.
    let sources = format!(
        "{}{}",
        include_str!("../../../src/check.rs"),
        include_str!("../../../src/api.rs")
    );
    for variant in BACKEND_STATUS_VARIANTS {
        // `pass` is a UI-side alias with no Rust variant of its own.
        if *variant == "pass" {
            continue;
        }
        let rust_name = to_camel_case(variant);
        assert!(
            sources.contains(&rust_name),
            "`{variant}` (Rust `{rust_name}`) is guarded by contract.rs but no longer appears in src/check.rs or src/api.rs"
        );
    }
}

/// Every hand-written frontend source, in one place.
///
/// Three separate tests below used to carry three hand-maintained copies of
/// this list, and all three had drifted: `views/diagnostics.js` appeared in
/// none of them, so its controls were never checked for `data-k` and its markup
/// guarantee was never enforced. A single list means adding a view is one edit,
/// and forgetting to add it is visible in one place.
const UI_SOURCES: &[(&str, &str)] = &[
    ("main.js", include_str!("../../ui/js/main.js")),
    ("dom.js", include_str!("../../ui/js/dom.js")),
    ("icons.js", include_str!("../../ui/js/icons.js")),
    ("ipc.js", include_str!("../../ui/js/ipc.js")),
    ("status.js", include_str!("../../ui/js/status.js")),
    ("theme.js", include_str!("../../ui/js/theme.js")),
    ("components.js", include_str!("../../ui/js/components.js")),
    (
        "views/dashboard.js",
        include_str!("../../ui/js/views/dashboard.js"),
    ),
    (
        "views/diagnostics.js",
        include_str!("../../ui/js/views/diagnostics.js"),
    ),
    (
        "views/identities.js",
        include_str!("../../ui/js/views/identities.js"),
    ),
    (
        "views/identity-wizard.js",
        include_str!("../../ui/js/views/identity-wizard.js"),
    ),
    (
        "views/onboarding.js",
        include_str!("../../ui/js/views/onboarding.js"),
    ),
    (
        "views/quick-switch.js",
        include_str!("../../ui/js/views/quick-switch.js"),
    ),
    (
        "views/repositories.js",
        include_str!("../../ui/js/views/repositories.js"),
    ),
    (
        "views/repository-details.js",
        include_str!("../../ui/js/views/repository-details.js"),
    ),
    (
        "views/settings.js",
        include_str!("../../ui/js/views/settings.js"),
    ),
    ("views/ssh.js", include_str!("../../ui/js/views/ssh.js")),
    (
        "views/status.js",
        include_str!("../../ui/js/views/status.js"),
    ),
];

#[test]
fn the_ui_source_list_covers_every_file_on_disk() {
    // Guards the guard. A new view that nobody adds to UI_SOURCES would be
    // silently exempt from every check in this file.
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../ui/js");
    let mut found = Vec::new();
    collect_js(&root, &root, &mut found);
    found.sort();
    for path in &found {
        assert!(
            UI_SOURCES.iter().any(|(name, _)| name == path),
            "ui/js/{path} exists but is not listed in UI_SOURCES, so no contract test covers it"
        );
    }
    assert_eq!(
        found.len(),
        UI_SOURCES.len(),
        "UI_SOURCES lists {} files but {} exist on disk: {found:?}",
        UI_SOURCES.len(),
        found.len()
    );
}

fn collect_js(root: &std::path::Path, dir: &std::path::Path, out: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_js(root, &path, out);
        } else if path.extension().is_some_and(|ext| ext == "js") {
            let relative = path
                .strip_prefix(root)
                .expect("under root")
                .to_string_lossy()
                .replace('\\', "/");
            out.push(relative);
        }
    }
}

#[test]
fn every_form_control_in_the_ui_carries_a_focus_key() {
    // A control rebuilt without a `data-k` loses focus and caret on every
    // keystroke, because each keystroke rebuilds its whole view. This is the
    // single easiest mistake to make in this frontend, so it is checked here as
    // well as in CI.
    for (path, source) in UI_SOURCES.iter().copied() {
        for control in ["h(\"input\"", "h(\"select\"", "h(\"textarea\""] {
            for (index, _) in source.match_indices(control) {
                // The props object always follows on the same or next lines;
                // 400 characters covers the longest control in this codebase.
                let window = &source[index..source.len().min(index + 400)];
                let props_end = window.find("}").unwrap_or(window.len());
                assert!(
                    window[..props_end].contains("data-k"),
                    "a {control} in ui/js/{path} has no data-k, so it will lose focus on re-render"
                );
            }
        }
    }
}

#[test]
fn the_ui_never_parses_markup() {
    // The frontend's XSS guarantee is structural: `h()` is the only way DOM is
    // created and it never parses markup, so repository paths, author names and
    // raw git stderr cannot become elements. That holds only while none of
    // these APIs appear.
    const FORBIDDEN: &[&str] = &[
        "innerHTML",
        "outerHTML",
        "insertAdjacentHTML",
        "document.write",
        "eval(",
        "new Function",
    ];
    for (path, source) in UI_SOURCES.iter().copied() {
        // Comments legitimately name these APIs when explaining why they are
        // absent, so only executable code is checked.
        let code = strip_js_comments(source);
        for needle in FORBIDDEN {
            assert!(
                !code.contains(needle),
                "ui/js/{path} uses `{needle}`, which breaks the no-markup-parsing guarantee"
            );
        }
    }
}

#[test]
fn the_page_has_no_inline_script_and_no_external_assets() {
    // Comments in the page legitimately mention these tags while explaining why
    // they are absent, so only real markup is checked.
    let html = strip_html_comments(include_str!("../../ui/index.html"));
    // `script-src 'self'` blocks inline script bodies and on* attributes, and
    // `default-src 'self'` blocks anything fetched from another origin.
    for (index, _) in html.match_indices("<script") {
        let rest = &html[index..];
        let open_end = rest.find('>').expect("a <script tag should be closed");
        let tag = &rest[..open_end];
        assert!(
            tag.contains("src="),
            "index.html has a <script> without src=, which `script-src 'self'` blocks"
        );
        let body_end = rest
            .find("</script>")
            .expect("a <script> should have a closing tag");
        assert!(
            rest[open_end + 1..body_end].trim().is_empty(),
            "index.html has a <script> with an inline body, which `script-src 'self'` blocks"
        );
    }
    for needle in ["src=\"http", "href=\"http", "onclick=", "onload="] {
        assert!(
            !html.contains(needle),
            "index.html contains `{needle}`, which the content security policy forbids"
        );
    }
}

#[test]
fn every_icon_the_ui_asks_for_exists_in_the_sprite() {
    let html = include_str!("../../ui/index.html");
    let mut checked = 0;
    for (_, source) in UI_SOURCES.iter().copied() {
        for (index, _) in source.match_indices("icon(\"") {
            let rest = &source[index + "icon(\"".len()..];
            let name = rest.split('"').next().unwrap_or_default();
            assert!(
                html.contains(&format!("id=\"i-{name}\"")),
                "the UI renders icon `{name}`, which has no <symbol id=\"i-{name}\"> in index.html"
            );
            checked += 1;
        }
    }
    assert!(
        checked > 10,
        "expected to find icon call sites, found {checked}"
    );
}

/// Remove `//` line comments and `/* */` block comments so the source checks
/// above see only executable code. Deliberately simple: it is not a JavaScript
/// parser, and the frontend contains no string literal holding a comment token.
/// Remove `<!-- -->` comments, so the markup checks see only real elements.
fn strip_html_comments(source: &str) -> String {
    let mut out = String::with_capacity(source.len());
    let mut rest = source;
    while let Some(start) = rest.find("<!--") {
        out.push_str(&rest[..start]);
        rest = rest[start + 4..]
            .find("-->")
            .map_or("", |end| &rest[start + 4 + end + 3..]);
    }
    out.push_str(rest);
    out
}

fn strip_js_comments(source: &str) -> String {
    let mut out = String::with_capacity(source.len());
    let mut rest = source;
    while !rest.is_empty() {
        if let Some(tail) = rest.strip_prefix("//") {
            rest = tail.find('\n').map_or("", |end| &tail[end..]);
        } else if let Some(tail) = rest.strip_prefix("/*") {
            rest = tail.find("*/").map_or("", |end| &tail[end + 2..]);
        } else {
            let next = rest.chars().next().expect("rest is not empty");
            out.push(next);
            rest = &rest[next.len_utf8()..];
        }
    }
    out
}
