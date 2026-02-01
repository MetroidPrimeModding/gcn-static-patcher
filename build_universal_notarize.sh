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
BUNDLE_ID="${BUNDLE_ID:-com.example.${BINARY_NAME}}"

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
    <string>1.0.0</string>
    <key>CFBundleShortVersionString</key>
    <string>1.0.0</string>
    <key>CFBundleExecutable</key>
    <string>${BINARY_NAME}</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>LSMinimumSystemVersion</key>
    <string>18.0</string>
  </dict>
</plist>
EOF

codesign -d -r- "$APP_BUNDLE" || true

CODESIGN_IDENTITY="${CODESIGN_IDENTITY:-}"
if [[ -z "$CODESIGN_IDENTITY" ]]; then
  echo "Set CODESIGN_IDENTITY to your Developer ID Application certificate name." >&2
  exit 1
fi

echo "Signing .app bundle..."
codesign --force --options runtime --timestamp --sign "$CODESIGN_IDENTITY" "$APP_BUNDLE"

codesign -d -r- "$APP_BUNDLE" || true

ZIP_PATH="$OUTPUT_DIR/${BINARY_NAME}.zip"
echo "Creating notarization zip at $ZIP_PATH..."
/usr/bin/ditto -c -k --keepParent "$APP_BUNDLE" "$ZIP_PATH"

echo "Submitting for notarization..."
if [[ -n "${NOTARY_PROFILE:-}" ]]; then
  xcrun notarytool submit "$ZIP_PATH" --keychain-profile "$NOTARY_PROFILE" --wait
else
  APPLE_ID="${APPLE_ID:-}"
  TEAM_ID="${TEAM_ID:-}"
  APP_SPECIFIC_PASSWORD="${APP_SPECIFIC_PASSWORD:-}"
  if [[ -z "$APPLE_ID" || -z "$TEAM_ID" || -z "$APP_SPECIFIC_PASSWORD" ]]; then
    echo "Set NOTARY_PROFILE (recommended) or APPLE_ID, TEAM_ID, and APP_SPECIFIC_PASSWORD." >&2
    exit 1
  fi
  xcrun notarytool submit "$ZIP_PATH" \
    --apple-id "$APPLE_ID" \
    --team-id "$TEAM_ID" \
    --password "$APP_SPECIFIC_PASSWORD" \
    --wait
fi

echo "Stapling notarization ticket..."
xcrun stapler staple "$APP_BUNDLE"

echo "Verifying assessment..."
spctl --assess --type execute --verbose "$APP_BUNDLE"

echo "Done: $APP_BUNDLE"
