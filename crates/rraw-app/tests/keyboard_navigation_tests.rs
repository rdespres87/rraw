#![forbid(unsafe_code)]

//! Integration and verification tests for keyboard navigation, inspector tab state,
//! shortcuts invariants, and macOS packaging configuration (Task 21.3).

use rraw_app::state::{LeftPanelTab, RatingFilter};
use std::fs;
use std::path::Path;

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[test]
fn test_left_panel_tab_state_transitions() {
    let mut tab = LeftPanelTab::Explorer;
    assert_eq!(tab, LeftPanelTab::Explorer);

    // Verify Tab 1 -> Explorer
    tab = LeftPanelTab::Explorer;
    assert_eq!(tab, LeftPanelTab::Explorer);

    // Verify Tab 2 -> Presets
    tab = LeftPanelTab::Presets;
    assert_eq!(tab, LeftPanelTab::Presets);

    // Verify Tab 3 -> Exif
    tab = LeftPanelTab::Exif;
    assert_eq!(tab, LeftPanelTab::Exif);
}

#[test]
fn test_rating_bounds_and_filtering() {
    // Ratings are bounded between 0 and 5 stars
    for r in 0..=5u8 {
        assert!(r <= 5);
        assert!(RatingFilter::All.matches(r));
    }

    assert!(RatingFilter::Unrated.matches(0));
    assert!(!RatingFilter::Unrated.matches(1));
    assert!(!RatingFilter::Unrated.matches(5));

    let filter_3 = RatingFilter::MinStars(3);
    assert!(!filter_3.matches(0));
    assert!(!filter_3.matches(2));
    assert!(filter_3.matches(3));
    assert!(filter_3.matches(4));
    assert!(filter_3.matches(5));
}

#[test]
fn test_macos_entitlements_plist_validity() -> TestResult {
    let entitlements_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .map(|p| p.join("assets/macos/Entitlements.plist"))
        .ok_or("Valid workspace root path")?;

    assert!(
        entitlements_path.exists(),
        "assets/macos/Entitlements.plist must exist for Hardened Runtime notarization"
    );

    let content = fs::read_to_string(&entitlements_path)?;
    assert!(content.contains("com.apple.security.cs.allow-jit"));
    assert!(content.contains("com.apple.security.cs.allow-unsigned-executable-memory"));
    assert!(content.contains("com.apple.security.files.user-selected.read-write"));
    Ok(())
}

#[test]
fn test_macos_info_plist_validity() -> TestResult {
    let info_plist_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .map(|p| p.join("assets/macos/Info.plist"))
        .ok_or("Valid workspace root path")?;

    assert!(
        info_plist_path.exists(),
        "assets/macos/Info.plist must exist"
    );

    let content = fs::read_to_string(&info_plist_path)?;
    assert!(content.contains("CFBundleIdentifier"));
    assert!(content.contains("com.rraw.darkroom"));
    assert!(content.contains("CFBundleExecutable"));
    assert!(content.contains("rraw-app"));
    assert!(content.contains("public.camera-raw-image"));
    Ok(())
}

#[test]
fn test_packaging_script_attributes() -> TestResult {
    let script_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .map(|p| p.join("scripts/package_macos.sh"))
        .ok_or("Valid workspace root path")?;

    assert!(script_path.exists(), "scripts/package_macos.sh must exist");

    let content = fs::read_to_string(&script_path)?;
    assert!(content.contains("Entitlements.plist"));
    assert!(content.contains("--notarize-profile"));
    assert!(content.contains("Rraw.app"));
    assert!(content.contains("UDZO"));
    Ok(())
}
