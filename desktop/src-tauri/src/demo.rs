//! Fixture data for `?demo`, used to produce documentation screenshots without
//! pointing the application at somebody's real configuration.
//!
//! The fixture body is `#[cfg(debug_assertions)]`, so a release build contains
//! `null` where this document would be and none of these bytes reach the
//! shipped executable. That is stricter than the bundler-level gate it
//! replaces, which could only mark the branches unreachable and still emitted
//! the values. The command itself is registered unconditionally so that the
//! handler list stays a single list — which is what the IPC contract test
//! checks the frontend against.

use serde_json::Value;

/// `null` in a release build. The frontend reads that as "not a demo build"
/// and carries on against real configuration.
#[cfg(not(debug_assertions))]
#[tauri::command]
pub fn demo_fixtures() -> Value {
    Value::Null
}

/// Every fixture the frontend needs, in one document. Requested once at boot,
/// and only when `?demo` is present.
#[cfg(debug_assertions)]
#[tauri::command]
pub fn demo_fixtures() -> Value {
    serde_json::json!({
        "profiles": [
            {
                "name": "personal",
                "profile": {
                    "github_user": "mira-dev",
                    "git_name": "Mira Chen",
                    "git_email": "oss@mira.dev",
                    "hostname": "github.com",
                    "ssh_host": "github.com-personal",
                    "ssh_key": "~/.ssh/id_ed25519",
                    "allowed_owners": ["mira-dev", "tauri-apps"],
                    "signing_key": null,
                    "signing_format": "ssh",
                    "require_signing": true
                }
            },
            {
                "name": "work",
                "profile": {
                    "github_user": "mira-acme",
                    "git_name": "Mira Chen",
                    "git_email": "mira.chen@acme.example",
                    "hostname": "github.com",
                    "ssh_key": null,
                    "allowed_owners": ["acme-eng"],
                    "signing_key": null,
                    "signing_format": "openpgp",
                    "require_signing": false
                }
            }
        ],
        "roots": ["C:\\dev"],
        "repositories": [
            {
                "path": "C:\\dev\\gitbound",
                "name": "gitbound",
                "bound_profile": "personal",
                "git_name": "Mira Chen",
                "git_email": "oss@mira.dev",
                "remote": {
                    "url": "git@github.com:tauri-apps/gitbound.git",
                    "protocol": "ssh",
                    "hostname": "github.com",
                    "owner": "tauri-apps",
                    "repository": "gitbound"
                },
                "status": "bound"
            },
            {
                "path": "C:\\dev\\acme-billing",
                "name": "acme-billing",
                "bound_profile": "work",
                "git_name": "Mira Chen",
                "git_email": "oss@mira.dev",
                "remote": {
                    "url": "https://github.com/acme-eng/acme-billing.git",
                    "protocol": "https",
                    "hostname": "github.com",
                    "owner": "acme-eng",
                    "repository": "acme-billing"
                },
                "status": "drifted",
                "detail": "Git email is oss@mira.dev but the bound profile expects mira.chen@acme.example"
            },
            {
                "path": "C:\\dev\\scratch",
                "name": "scratch",
                "status": "unbound"
            }
        ],
        "doctor": {
            "config_path": "C:\\Users\\mira\\AppData\\Roaming\\gitbound\\config.toml",
            "schema_version": 3,
            "profile_count": 2,
            "healthy": false,
            "profile_issues": [],
            "dependencies": [
                { "name": "git", "state": "ok", "detail": "git version 2.49.0" },
                { "name": "gh", "state": "ok", "detail": "gh version 2.76.1" },
                {
                    "name": "ssh",
                    "state": "unavailable",
                    "detail": "OpenSSH client was not found",
                    "remediation": "Install the OpenSSH Client optional feature and restart GitBound."
                }
            ]
        },
        "ssh_test": {
            "profile": "personal",
            "expected_user": "mira-dev",
            "actual_user": "mira-dev",
            "hostname": "github.com",
            "key": "~/.ssh/id_ed25519",
            "status": "verified",
            "message": "SSH authenticates as mira-dev"
        },
        "accounts": [
            { "login": "mira-dev", "active": true, "valid": true },
            { "login": "mira-acme", "active": false, "valid": true }
        ],
        // Deliberately mixed: one green run, one red, one still going. A
        // fixture where everything passes makes the failure styling impossible
        // to review, which is the whole point of having fixtures.
        "ci_status": {
            "available": true,
            "runs": [
                {
                    "id": 1801,
                    "name": "CI",
                    "title": "Bind the release branch to the work identity",
                    "branch": "main",
                    "sha": "9f2c1ad",
                    "created_at": "2026-09-05T08:41:00Z",
                    "url": "https://github.com/tauri-apps/gitbound/actions/runs/1801",
                    "conclusion": "success"
                },
                {
                    "id": 1800,
                    "name": "Identity",
                    "title": "Add the client identity",
                    "branch": "feature/client",
                    "sha": "1b7e04c",
                    "created_at": "2026-09-05T07:02:00Z",
                    "url": "https://github.com/tauri-apps/gitbound/actions/runs/1800",
                    "conclusion": "failure"
                },
                {
                    "id": 1799,
                    "name": "Release",
                    "title": "v1.0.0",
                    "branch": "main",
                    "sha": "44ab902",
                    "created_at": "2026-09-04T19:15:00Z",
                    "url": "https://github.com/tauri-apps/gitbound/actions/runs/1799",
                    "conclusion": "running"
                }
            ]
        }
    })
}
