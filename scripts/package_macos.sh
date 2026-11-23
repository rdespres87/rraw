#!/usr/bin/env bash
# ==============================================================================
# scripts/package_macos.sh
# Production macOS Application Bundle & Drag-and-Drop DMG Installer for Rraw
#
# Generates:
#   1. Rraw.app (macOS Application Bundle with Info.plist, HiDPI icon, codesigned)
#   2. Rraw-v<VERSION>.dmg (Compressed read-only DMG with /Applications symlink)
#   3. Rraw-v<VERSION>.dmg.sha256 (Cryptographic checksum)
# ==============================================================================

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WORKSPACE_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

# Default configurations
BUILD_MODE="release"
TARGET=""
SKIP_BUILD=false
SKIP_DMG=false
CODESIGN_IDENTITY="-" # Default to ad-hoc codesign
NOTARIZE_PROFILE=""
OUT_DIR="${WORKSPACE_ROOT}/target/dist"
APP_NAME="Rraw"
BUNDLE_ID="com.rraw.darkroom"

usage() {
    cat <<EOF
Usage: $(basename "$0") [OPTIONS]

Options:
    --debug                       Build debug binary instead of release
    --target <TRIPLE>             Cargo target triple (e.g. aarch64-apple-darwin)
    --skip-build                  Reuse existing target binary without invoking cargo
    --skip-dmg                    Assemble Rraw.app bundle only, skip DMG creation
    --sign <IDENTITY>             Codesign identity (default: "-" for ad-hoc signing)
    --notarize-profile <PROFILE>  Keychain profile for xcrun notarytool submit
    --out-dir <DIR>               Output directory for .app and .dmg (default: target/dist)
    -h, --help                    Show this help message

Examples:
    ./scripts/package_macos.sh
    ./scripts/package_macos.sh --skip-build
    ./scripts/package_macos.sh --sign "Developer ID Application: Your Name (TEAMID)"
    ./scripts/package_macos.sh --sign "Developer ID Application: ..." --notarize-profile "AC_PASSWORD"
EOF
    exit 0
}

# Parse CLI arguments
while [[ $# -gt 0 ]]; do
    case "$1" in
        --debug)
            BUILD_MODE="debug"
            shift
            ;;
        --target)
            TARGET="$2"
            shift 2
            ;;
        --skip-build)
            SKIP_BUILD=true
            shift
            ;;
        --skip-dmg)
            SKIP_DMG=true
            shift
            ;;
        --sign)
            CODESIGN_IDENTITY="$2"
            shift 2
            ;;
        --notarize-profile)
            NOTARIZE_PROFILE="$2"
            shift 2
            ;;
        --out-dir)
            OUT_DIR="$2"
            shift 2
            ;;
        -h|--help)
            usage
            ;;
        *)
            echo "Error: Unknown argument: $1" >&2
            usage
            ;;
    esac
done

echo "======================================================================"
echo "  Rraw — macOS Application & DMG Packaging Pipeline"
echo "======================================================================"

cd "${WORKSPACE_ROOT}"

# 1. Verify required macOS toolchain
echo "==> [1/6] Verifying toolchain requirements..."
command -v cargo >/dev/null 2>&1 || { echo "Error: cargo is required." >&2; exit 1; }
command -v iconutil >/dev/null 2>&1 || { echo "Error: iconutil is required on macOS." >&2; exit 1; }
command -v hdiutil >/dev/null 2>&1 || { echo "Error: hdiutil is required on macOS." >&2; exit 1; }
command -v codesign >/dev/null 2>&1 || { echo "Error: codesign is required on macOS." >&2; exit 1; }
command -v python3 >/dev/null 2>&1 || { echo "Error: python3 is required for icon generation." >&2; exit 1; }

# 2. Extract Version Metadata
VERSION="$(cargo metadata --no-deps --format-version 1 | grep -o '"version":"[^"]*"' | head -n 1 | cut -d'"' -f4)"
GIT_COMMIT="$(git rev-parse --short=7 HEAD 2>/dev/null || echo "unknown")"
BUILD_NUMBER="${VERSION}.${GIT_COMMIT}"
echo "    App Name:     ${APP_NAME}"
echo "    Version:      ${VERSION} (${GIT_COMMIT})"
echo "    Build Mode:   ${BUILD_MODE}"
echo "    Codesign:     ${CODESIGN_IDENTITY}"
echo "    Output Dir:   ${OUT_DIR}"

# 3. Ensure HiDPI Application Icon is built
echo "==> [2/6] Checking application icon assets..."
ICNS_FILE="${WORKSPACE_ROOT}/assets/macos/AppIcon.icns"
if [[ ! -f "${ICNS_FILE}" ]]; then
    echo "    Generating AppIcon.icns via assets/macos/generate_icon.py..."
    python3 "${WORKSPACE_ROOT}/assets/macos/generate_icon.py"
else
    echo "    Found existing AppIcon.icns ($(stat -f%z "${ICNS_FILE}") bytes)"
fi

# 4. Compile binary via Cargo
BIN_DIR="${WORKSPACE_ROOT}/target"
if [[ -n "${TARGET}" ]]; then
    BIN_DIR="${BIN_DIR}/${TARGET}"
fi
BIN_PATH="${BIN_DIR}/${BUILD_MODE}/rraw-app"

if [[ "${SKIP_BUILD}" = false ]]; then
    echo "==> [3/6] Building rraw-app binary (${BUILD_MODE})..."
    CARGO_ARGS=("build" "-p" "rraw-app")
    if [[ "${BUILD_MODE}" == "release" ]]; then
        CARGO_ARGS+=("--release")
    fi
    if [[ -n "${TARGET}" ]]; then
        CARGO_ARGS+=("--target" "${TARGET}")
    fi
    cargo "${CARGO_ARGS[@]}"
else
    echo "==> [3/6] Skipping cargo build (--skip-build specified)..."
fi

if [[ ! -f "${BIN_PATH}" ]]; then
    echo "Error: Binary not found at ${BIN_PATH}" >&2
    exit 1
fi
echo "    Using binary: ${BIN_PATH} ($(stat -f%z "${BIN_PATH}") bytes)"

# 5. Assemble macOS Application Bundle (.app)
echo "==> [4/6] Assembling ${APP_NAME}.app bundle..."
BUNDLE_DIR="${OUT_DIR}/${APP_NAME}.app"
rm -rf "${BUNDLE_DIR}"
mkdir -p "${BUNDLE_DIR}/Contents/MacOS"
mkdir -p "${BUNDLE_DIR}/Contents/Resources"

# Copy binary
cp "${BIN_PATH}" "${BUNDLE_DIR}/Contents/MacOS/rraw-app"
chmod +x "${BUNDLE_DIR}/Contents/MacOS/rraw-app"

# Copy Icon
cp "${ICNS_FILE}" "${BUNDLE_DIR}/Contents/Resources/AppIcon.icns"

# Copy i18n catalogs
if [[ -d "${WORKSPACE_ROOT}/assets/i18n" ]]; then
    mkdir -p "${BUNDLE_DIR}/Contents/Resources/assets"
    rm -rf "${BUNDLE_DIR}/Contents/Resources/assets/i18n"
    cp -R "${WORKSPACE_ROOT}/assets/i18n" "${BUNDLE_DIR}/Contents/Resources/assets/"
    echo "    Bundled i18n translation catalogs into Contents/Resources/assets/i18n"
fi

# Generate Info.plist with substituted metadata
PLIST_TEMPLATE="${WORKSPACE_ROOT}/assets/macos/Info.plist"
if [[ ! -f "${PLIST_TEMPLATE}" ]]; then
    echo "Error: Info.plist template not found at ${PLIST_TEMPLATE}" >&2
    exit 1
fi

sed \
    -e "s/\${VERSION}/${VERSION}/g" \
    -e "s/\${BUILD_NUMBER}/${BUILD_NUMBER}/g" \
    -e "s/\${COMMIT_HASH}/${GIT_COMMIT}/g" \
    "${PLIST_TEMPLATE}" > "${BUNDLE_DIR}/Contents/Info.plist"

# Write PkgInfo
echo -n "APPL????" > "${BUNDLE_DIR}/Contents/PkgInfo"

# Codesign bundle with Hardened Runtime entitlements
echo "==> [5/6] Signing application bundle..."
ENTITLEMENTS_FILE="${WORKSPACE_ROOT}/assets/macos/Entitlements.plist"
ENTITLEMENTS_ARG=()
if [[ -f "${ENTITLEMENTS_FILE}" ]]; then
    ENTITLEMENTS_ARG=("--entitlements" "${ENTITLEMENTS_FILE}")
    echo "    Using entitlements: ${ENTITLEMENTS_FILE}"
fi

if [[ "${CODESIGN_IDENTITY}" == "-" ]]; then
    echo "    Ad-hoc signing ${BUNDLE_DIR}..."
    codesign --force --deep "${ENTITLEMENTS_ARG[@]}" --sign - "${BUNDLE_DIR}"
else
    echo "    Signing with identity '${CODESIGN_IDENTITY}' and runtime hardening..."
    codesign --force --deep --options runtime --timestamp "${ENTITLEMENTS_ARG[@]}" --sign "${CODESIGN_IDENTITY}" "${BUNDLE_DIR}"
fi

# Verify signature
codesign --verify --deep --strict --verbose=1 "${BUNDLE_DIR}"
echo "    Signature verified successfully."

# 6. Create Drag-and-Drop DMG Disk Image
if [[ "${SKIP_DMG}" = false ]]; then
    echo "==> [6/6] Generating Drag-and-Drop DMG disk image..."
    DMG_NAME="${APP_NAME}-v${VERSION}.dmg"
    DMG_PATH="${OUT_DIR}/${DMG_NAME}"
    DMG_STAGING="${OUT_DIR}/dmg_staging"

    mkdir -p "${OUT_DIR}"
    rm -rf "${DMG_STAGING}" "${DMG_PATH}" "${DMG_PATH}.sha256"
    mkdir -p "${DMG_STAGING}"

    # Copy bundle into staging
    cp -R "${BUNDLE_DIR}" "${DMG_STAGING}/"

    # Create /Applications symlink for drag-and-drop installer
    ln -s /Applications "${DMG_STAGING}/Applications"

    # Build compressed read-only DMG with hdiutil
    echo "    Building compressed UDZO disk image..."
    hdiutil create \
        -volname "${APP_NAME}" \
        -srcfolder "${DMG_STAGING}" \
        -ov \
        -format UDZO \
        "${DMG_PATH}"

    # Cleanup staging directory
    rm -rf "${DMG_STAGING}"

    # Sign DMG container if valid identity provided
    if [[ "${CODESIGN_IDENTITY}" != "-" ]]; then
        echo "    Signing DMG disk image..."
        codesign --force --sign "${CODESIGN_IDENTITY}" --timestamp "${DMG_PATH}"
    fi

    # Optional Apple Notarization submission
    if [[ -n "${NOTARIZE_PROFILE}" ]]; then
        echo "    Submitting DMG to Apple Notarization Service (profile: ${NOTARIZE_PROFILE})..."
        xcrun notarytool submit "${DMG_PATH}" --keychain-profile "${NOTARIZE_PROFILE}" --wait
        echo "    Stapling notarization ticket to DMG..."
        xcrun stapler staple "${DMG_PATH}"
        echo "    Notarization and stapling complete."
    fi

    # Compute SHA-256 Checksum
    (cd "${OUT_DIR}" && shasum -a 256 "${DMG_NAME}" > "${DMG_NAME}.sha256")

    echo "    DMG Created:  ${DMG_PATH} ($(stat -f%z "${DMG_PATH}") bytes)"
    echo "    SHA-256:      $(cat "${DMG_PATH}.sha256")"
else
    echo "==> [6/6] Skipping DMG creation (--skip-dmg specified)."
fi

echo "======================================================================"
echo "  Packaging Complete!"
echo "  App Bundle: ${BUNDLE_DIR}"
if [[ "${SKIP_DMG}" = false ]]; then
    echo "  DMG Image:  ${OUT_DIR}/${APP_NAME}-v${VERSION}.dmg"
fi
echo "======================================================================"
