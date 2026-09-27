#!/usr/bin/env bash
set -euo pipefail

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "This script must be run on macOS." >&2
  exit 1
fi

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$PROJECT_ROOT"

BINARY_NAME="${BINARY_NAME:-gcn-static-patcher-gui}"
OUTPUT_DIR="${OUTPUT_DIR:-target/macos-release}"
BUNDLE_ID="${BUNDLE_ID:-com.pwootage.${BINARY_NAME}}"
VERSION="$(sed -n 's/^version = "\(.*\)"$/\1/p' Cargo.toml | head -n 1)"

CODESIGN_IDENTITY="${CODESIGN_IDENTITY:-}"
if [[ -z "$CODESIGN_IDENTITY" ]]; then
  echo "Set CODESIGN_IDENTITY to your Developer ID Application certificate name." >&2
  exit 1
fi

if [[ -n "${NOTARY_PROFILE:-}" ]]; then
  NOTARY_AUTH=(--keychain-profile "$NOTARY_PROFILE")
elif [[ -n "${APPLE_ID:-}" && -n "${TEAM_ID:-}" && -n "${APP_SPECIFIC_PASSWORD:-}" ]]; then
  NOTARY_AUTH=(--apple-id "$APPLE_ID" --team-id "$TEAM_ID" --password "$APP_SPECIFIC_PASSWORD")
else
  echo "Set NOTARY_PROFILE (recommended) or APPLE_ID, TEAM_ID, and APP_SPECIFIC_PASSWORD." >&2
  exit 1
fi

X86_TARGET="x86_64-apple-darwin"
ARM_TARGET="aarch64-apple-darwin"

echo "Adding Rust targets (if needed)..."
rustup target add "$X86_TARGET" "$ARM_TARGET"

echo "Building release binaries..."
cargo build --release --target "$X86_TARGET"
cargo build --release --target "$ARM_TARGET"

mkdir -p "$OUTPUT_DIR"
UNIVERSAL_BIN="$OUTPUT_DIR/$BINARY_NAME"
APP_BUNDLE="$OUTPUT_DIR/${BINARY_NAME}.app"

echo "Creating universal binary at $UNIVERSAL_BIN..."
lipo -create \
  "target/$X86_TARGET/release/$BINARY_NAME" \
  "target/$ARM_TARGET/release/$BINARY_NAME" \
  -output "$UNIVERSAL_BIN"

echo "Creating .app bundle..."
APP_CONTENTS="$APP_BUNDLE/Contents"
APP_MACOS="$APP_CONTENTS/MacOS"
APP_RESOURCES="$APP_CONTENTS/Resources"

rm -rf "$APP_BUNDLE"
mkdir -p "$APP_MACOS" "$APP_RESOURCES"
cp "$UNIVERSAL_BIN" "$APP_MACOS/$BINARY_NAME"

INFO_PLIST="$APP_CONTENTS/Info.plist"
cat > "$INFO_PLIST" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
  <dict>
    <key>CFBundleName</key>
    <string>${BINARY_NAME}</string>
    <key>CFBundleDisplayName</key>
    <string>${BINARY_NAME}</string>
    <key>CFBundleIdentifier</key>
    <string>${BUNDLE_ID}</string>
    <key>CFBundleVersion</key>
    <string>${VERSION}</string>
    <key>CFBundleShortVersionString</key>
    <string>${VERSION}</string>
    <key>CFBundleExecutable</key>
    <string>${BINARY_NAME}</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>LSMinimumSystemVersion</key>
    <string>15.0</string>
  </dict>
</plist>
EOF

echo "Signing .app bundle..."
codesign --force --options runtime --timestamp --sign "$CODESIGN_IDENTITY" "$APP_BUNDLE"
codesign --verify --strict --verbose=2 "$APP_BUNDLE"

# notarytool only uploads this; the distributable zip is rebuilt after stapling below.
SUBMIT_ZIP="$OUTPUT_DIR/${BINARY_NAME}-notarize.zip"
rm -f "$SUBMIT_ZIP"
/usr/bin/ditto -c -k --keepParent "$APP_BUNDLE" "$SUBMIT_ZIP"

echo "Submitting for notarization..."
# `submit --wait` exits 0 even when Apple rejects the upload, so check the status ourselves.
SUBMIT_RESULT="$(xcrun notarytool submit "$SUBMIT_ZIP" "${NOTARY_AUTH[@]}" --wait --output-format json)"
SUBMISSION_ID="$(plutil -extract id raw - <<<"$SUBMIT_RESULT")"
SUBMISSION_STATUS="$(plutil -extract status raw - <<<"$SUBMIT_RESULT")"
rm -f "$SUBMIT_ZIP"

if [[ "$SUBMISSION_STATUS" != "Accepted" ]]; then
  echo "Notarization failed with status '$SUBMISSION_STATUS'. Log:" >&2
  xcrun notarytool log "$SUBMISSION_ID" "${NOTARY_AUTH[@]}" >&2 || true
  exit 1
fi

echo "Stapling notarization ticket..."
xcrun stapler staple "$APP_BUNDLE"
xcrun stapler validate "$APP_BUNDLE"

echo "Verifying assessment..."
spctl --assess --type execute --verbose "$APP_BUNDLE"

ZIP_PATH="$OUTPUT_DIR/${BINARY_NAME}-macos-universal.zip"
echo "Creating release zip at $ZIP_PATH..."
rm -f "$ZIP_PATH"
/usr/bin/ditto -c -k --keepParent "$APP_BUNDLE" "$ZIP_PATH"

echo "Done: $ZIP_PATH"
