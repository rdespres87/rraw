//! Compile-time build metadata: semantic version, git commit hash and build date.
//!
//! These constants are populated at compile time (`env!` / `option_env!`) from
//! the package version and the `build.rs` git queries, and are shared by
//! `rraw-app` (top bar + About dialog) and `rraw-cli` (`--version`).

/// Workspace semantic version (`MAJOR.MINOR.PATCH`), from `CARGO_PKG_VERSION`.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Semantic version prefixed with `v` (`vMAJOR.MINOR.PATCH`), evaluated at compile time.
pub const VERSION_TAG: &str = concat!("v", env!("CARGO_PKG_VERSION"));

/// Fallback for build metadata that is unavailable outside a git checkout.
const UNKNOWN: &str = "unknown";

/// Resolve an optional compile-time environment variable to a stable fallback.
const fn or_unknown(value: Option<&str>) -> &str {
    match value {
        Some(value) => value,
        None => UNKNOWN,
    }
}

/// Short git commit hash (7 hex chars) of the checkout, or `"unknown"`.
pub const GIT_COMMIT_HASH: &str = or_unknown(option_env!("GIT_COMMIT_HASH"));

/// Build date as an ISO 8601 UTC timestamp, or `"unknown"`.
pub const BUILD_DATE: &str = or_unknown(option_env!("BUILD_DATE"));

/// Full version string with commit hash and build date, or `"unknown"`.
pub const LONG_VERSION: &str = or_unknown(option_env!("LONG_VERSION"));

/// Immutable snapshot of the build metadata, usable by any UI or CLI surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BuildInfo {
    /// Semantic version string.
    pub version: &'static str,
    /// Short git commit hash.
    pub commit_hash: &'static str,
    /// ISO 8601 UTC build date.
    pub build_date: &'static str,
    /// Full version string including commit hash and build date.
    pub long_version: &'static str,
}

impl BuildInfo {
    /// The current build metadata.
    pub const fn current() -> Self {
        Self {
            version: VERSION,
            commit_hash: GIT_COMMIT_HASH,
            build_date: BUILD_DATE,
            long_version: LONG_VERSION,
        }
    }

    /// Human-readable "About" text embedding all three metadata fields.
    pub fn about_text() -> String {
        format!(
            "Rraw {} — build {commit} ({date})",
            VERSION,
            commit = GIT_COMMIT_HASH,
            date = BUILD_DATE
        )
    }
}

impl Default for BuildInfo {
    fn default() -> Self {
        Self::current()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_matches_workspace_package_version() {
        assert_eq!(VERSION, env!("CARGO_PKG_VERSION"));
    }

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

    #[test]
    fn build_info_current_matches_constants() {
        let info = BuildInfo::current();
        assert_eq!(info.version, VERSION);
        assert_eq!(info.commit_hash, GIT_COMMIT_HASH);
        assert_eq!(info.build_date, BUILD_DATE);
        assert_eq!(info.long_version, LONG_VERSION);
    }

    #[test]
    fn version_tag_starts_with_v() {
        assert_eq!(VERSION_TAG, format!("v{VERSION}"));
    }

    #[test]
    fn long_version_contains_metadata() {
        assert!(LONG_VERSION.contains(VERSION));
        assert!(LONG_VERSION.contains(GIT_COMMIT_HASH));
    }

    #[test]
    fn about_text_embeds_all_metadata_fields() {
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
}
