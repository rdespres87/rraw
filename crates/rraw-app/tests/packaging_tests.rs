#![forbid(unsafe_code)]

//! Integration tests for Phase 13.2 — macOS App Bundle & Drag-and-Drop DMG Packaging.
//!
//! Validates:
//!   - `assets/macos/Info.plist`: Apple XML property list template integrity & document types.
//!   - `assets/macos/AppIcon.icns`: Apple ICNS container format, magic bytes, and size.
//!   - `scripts/package_macos.sh`: Packaging script flags, permissions, and CLI help.

use std::path::{Path, PathBuf};
use std::process::Command;

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap_or_else(|_| Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."))
}

#[test]
fn info_plist_template_contains_all_required_keys() -> Result<(), Box<dyn std::error::Error>> {
    let plist_path = workspace_root().join("assets/macos/Info.plist");
    assert!(
        plist_path.exists(),
        "Info.plist not found at {}",
        plist_path.display()
    );

    let content = std::fs::read_to_string(&plist_path)?;

    // XML preamble and Apple doctype
    assert!(
        content.contains("<?xml version=\"1.0\" encoding=\"UTF-8\"?>"),
        "missing XML declaration"
    );
    assert!(
        content.contains("<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\""),
        "missing Apple plist doctype"
    );

    // Essential metadata keys
    let required_pairs = [
        ("<key>CFBundleName</key>", "<string>Rraw</string>"),
        ("<key>CFBundleDisplayName</key>", "<string>Rraw</string>"),
        ("<key>CFBundleExecutable</key>", "<string>rraw-app</string>"),
        ("<key>CFBundleIconFile</key>", "<string>AppIcon</string>"),
        (
            "<key>CFBundleIdentifier</key>",
            "<string>com.rraw.darkroom</string>",
        ),
        ("<key>CFBundlePackageType</key>", "<string>APPL</string>"),
        ("<key>LSMinimumSystemVersion</key>", "<string>13.0</string>"),
        ("<key>NSHighResolutionCapable</key>", "<true/>"),
        ("<key>NSSupportsAutomaticGraphicsSwitching</key>", "<true/>"),
    ];

    for (key, val) in required_pairs {
        assert!(
            content.contains(key),
            "missing required key {key} in Info.plist"
        );
        assert!(
            content.contains(val),
            "missing required value {val} in Info.plist"
        );
    }

    // Dynamic placeholders
    assert!(
        content.contains("${VERSION}"),
        "missing ${{VERSION}} placeholder in Info.plist"
    );
    assert!(
        content.contains("${BUILD_NUMBER}"),
        "missing ${{BUILD_NUMBER}} placeholder in Info.plist"
    );

    // Document types for RAW image extensions
    let raw_extensions = [
        "dng", "arw", "cr2", "cr3", "nef", "orf", "rw2", "raf", "pef",
    ];
    for ext in raw_extensions {
        let tag = format!("<string>{ext}</string>");
        assert!(
            content.contains(&tag),
            "missing RAW file extension {ext} in Info.plist CFBundleDocumentTypes"
        );
    }

    Ok(())
}

#[test]
fn app_icon_icns_header_and_size_validity() -> Result<(), Box<dyn std::error::Error>> {
    let icns_path = workspace_root().join("assets/macos/AppIcon.icns");
    if !icns_path.exists() {
        // In a fresh clean checkout, automatically invoke generate_icon.py if Python 3 is available
        let generator = workspace_root().join("assets/macos/generate_icon.py");
        if generator.exists() {
            let _ = Command::new("python3").arg(&generator).output();
        }
    }
    assert!(
        icns_path.exists(),
        "AppIcon.icns not found at {}",
        icns_path.display()
    );

    let bytes = std::fs::read(&icns_path)?;
    assert!(
        bytes.len() >= 10_000,
        "AppIcon.icns is unexpectedly small: {} bytes",
        bytes.len()
    );

    // Apple ICNS magic bytes: 'i', 'c', 'n', 's' (0x69, 0x63, 0x6E, 0x73)
    assert_eq!(
        &bytes[0..4],
        b"icns",
        "invalid magic header for Apple ICNS container"
    );

    // Big-endian 32-bit file size at offset 4..8
    let encoded_size = u32::from_be_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]) as usize;
    assert_eq!(
        encoded_size,
        bytes.len(),
        "encoded ICNS header length ({encoded_size}) does not match actual byte length ({})",
        bytes.len()
    );

    Ok(())
}

#[test]
fn package_macos_script_exists_and_is_executable() -> Result<(), Box<dyn std::error::Error>> {
    let script_path = workspace_root().join("scripts/package_macos.sh");
    assert!(
        script_path.exists(),
        "package_macos.sh not found at {}",
        script_path.display()
    );

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let metadata = std::fs::metadata(&script_path)?;
        let mode = metadata.permissions().mode();
        assert!(
            mode & 0o111 != 0,
            "package_macos.sh does not have executable permissions (mode {:o})",
            mode
        );
    }

    let script_content = std::fs::read_to_string(&script_path)?;
    let required_tokens = [
        "--skip-build",
        "--skip-dmg",
        "--sign",
        "--target",
        "--out-dir",
        "hdiutil create",
        "codesign",
        "AppIcon.icns",
        "Info.plist",
    ];

    for token in required_tokens {
        assert!(
            script_content.contains(token),
            "missing required packaging token '{token}' in package_macos.sh"
        );
    }

    Ok(())
}

#[test]
fn package_script_help_flag_succeeds() -> Result<(), Box<dyn std::error::Error>> {
    let script_path = workspace_root().join("scripts/package_macos.sh");
    let output = Command::new(&script_path).arg("--help").output()?;

    assert!(
        output.status.success(),
        "package_macos.sh --help failed with status {:?}",
        output.status
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Usage:") && stdout.contains("Rraw"),
        "unexpected help output: {stdout}"
    );

    Ok(())
}
