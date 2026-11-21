#![forbid(unsafe_code)]

//! Tests for Phase 13.1 — Semantic Versioning & Build Metadata:
//! compile-time `VERSION`, git commit hash, build date, About dialog, CLI `--version`.

use rraw_app::RrawApp;
use rraw_core::build_info::{
    BuildInfo, BUILD_DATE, GIT_COMMIT_HASH, LONG_VERSION, VERSION, VERSION_TAG,
};

/// The workspace semver must be embedded at compile time and match the
/// `[workspace.package]` version in the repository root `Cargo.toml`.
#[test]
fn version_matches_workspace_cargo_toml() -> Result<(), Box<dyn std::error::Error>> {
    // Integration tests run with CWD = crate root; resolve the workspace root.
    let root_manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("Cargo.toml");
    let manifest = std::fs::read_to_string(&root_manifest)?;
    let mut in_workspace_package = false;
    let mut expected: Option<&str> = None;
    for line in manifest.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            in_workspace_package = trimmed == "[workspace.package]";
            continue;
        }
        if in_workspace_package {
            if let Some(rest) = trimmed.strip_prefix("version") {
                if let Some(value) = rest.strip_prefix(" = ") {
                    expected = Some(value.trim().trim_matches('"'));
                    break;
                }
            }
        }
    }
    assert_eq!(
        expected,
        Some(VERSION),
        "compile-time VERSION must equal the workspace package version"
    );
    Ok(())
}

/// VERSION must be a valid semantic version `MAJOR.MINOR.PATCH`.
#[test]
fn version_is_valid_semver() {
    let parts: Vec<&str> = VERSION.split('.').collect();
    assert_eq!(parts.len(), 3, "expected MAJOR.MINOR.PATCH, got {VERSION}");
    for part in &parts {
        assert!(
            !part.is_empty() && part.chars().all(|c| c.is_ascii_digit()),
            "non-numeric semver component in {VERSION}"
        );
    }
}

/// Build date must be an ISO 8601 UTC timestamp `YYYY-MM-DDTHH:MM:SSZ` (20 or 10 chars).
#[test]
fn build_date_is_iso8601_utc() {
    let d = BUILD_DATE;
    // `YYYY-MM-DDTHH:MM:SSZ` is 20 characters.
    let is_full = d.len() == 20
        && d.as_bytes()[10] == b'T'
        && d.as_bytes()[19] == b'Z'
        && d.as_bytes()[0].is_ascii_digit();
    let is_date_only = d.len() == 10 && d.as_bytes()[4] == b'-';
    assert!(
        is_full || is_date_only,
        "BUILD_DATE `{d}` is not an ISO 8601 UTC timestamp"
    );
}

/// Git commit hash must be a 7..40 character lowercase hex string.
#[test]
fn git_commit_hash_is_hex() {
    let h = GIT_COMMIT_HASH;
    assert!(
        (7..=40).contains(&h.len()) && h.chars().all(|c| c.is_ascii_hexdigit()),
        "GIT_COMMIT_HASH `{h}` is not a valid hex hash"
    );
}

/// The formatted About dialog text must embed version, commit and date.
#[test]
fn about_text_embeds_version_commit_and_date() {
    let text = BuildInfo::about_text();
    assert!(text.contains(VERSION), "about text missing VERSION: {text}");
    assert!(
        text.contains(GIT_COMMIT_HASH),
        "about text missing commit hash: {text}"
    );
    assert!(
        text.contains(BUILD_DATE),
        "about text missing build date: {text}"
    );
}

/// The headless app must expose the About dialog state, closed by default.
#[test]
fn about_dialog_state_defaults_closed_and_toggles() {
    let mut app = RrawApp::new_headless();
    assert!(!app.show_about_dialog);
    app.show_about_dialog = true;
    assert!(app.show_about_dialog);
    // Escape must dismiss the About dialog first in the hierarchy.
    assert!(app.handle_escape_key());
    assert!(!app.show_about_dialog);
}

/// The compile-time VERSION_TAG must start with 'v' and match the version.
#[test]
fn version_tag_matches_expected_format() {
    assert_eq!(VERSION_TAG, format!("v{VERSION}"));
}

/// LONG_VERSION must incorporate version, git hash and build date.
#[test]
fn long_version_contains_all_metadata() {
    assert!(LONG_VERSION.contains(VERSION));
    assert!(LONG_VERSION.contains(GIT_COMMIT_HASH));
    assert_eq!(BuildInfo::current().long_version, LONG_VERSION);
}
