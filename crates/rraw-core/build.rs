//! Build script for `rraw-core`: embeds git commit hash and build date
//! as compile-time environment variables for the `build_info` module.
//!
//! Both values fall back to `"unknown"` when the git metadata is not
//! available (e.g. building from a source archive), so compilation
//! never fails outside a git checkout.

use std::process::Command;

fn git_output(args: &[&str]) -> Option<String> {
    // `TZ=UTC` forces UTC dates regardless of the builder's locale.
    let output = Command::new("git")
        .args(args)
        .env("TZ", "UTC")
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8(output.stdout).ok()?;
    let trimmed = text.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

fn main() {
    // Re-run only when this script changes; the values are baked at build time.
    println!("cargo:rerun-if-changed=build.rs");

    let version = std::env::var("CARGO_PKG_VERSION").unwrap_or_else(|_| "0.1.0".to_string());

    let commit_hash =
        git_output(&["rev-parse", "--short=7", "HEAD"]).unwrap_or_else(|| "unknown".to_string());
    println!("cargo:rustc-env=GIT_COMMIT_HASH={commit_hash}");

    // The committer date as a genuine ISO 8601 UTC timestamp: `TZ=UTC`
    // (set in `git_output`) plus a literal `Z` suffix renders
    // `YYYY-MM-DDTHH:MM:SSZ` regardless of the builder's locale.
    let build_date = git_output(&[
        "log",
        "-1",
        "--format=%cd",
        "--date=format:%Y-%m-%dT%H:%M:%SZ",
    ])
    .unwrap_or_else(|| "unknown".to_string());
    println!("cargo:rustc-env=BUILD_DATE={build_date}");

    let long_version = format!("{version} ({commit_hash} {build_date})");
    println!("cargo:rustc-env=LONG_VERSION={long_version}");
}
