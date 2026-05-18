#!/usr/bin/env bash
# Build tanaterm and package it as a macOS .app bundle so Finder/Dock show the
# custom icon (a bare Mach-O binary cannot carry a Finder icon).
#
#   Output: target/release/tanaterm.app
#
# Uses only macOS built-in tools (sips, iconutil) — no extra cargo plugins.
set -euo pipefail

cd "$(dirname "$0")/.."

APP_NAME="tanaterm"
ICON_SRC="assets/icon.png"
TARGET_DIR="target/release"
APP_DIR="$TARGET_DIR/$APP_NAME.app"
BIN="$TARGET_DIR/$APP_NAME"

if [ ! -f "$ICON_SRC" ]; then
  echo "error: $ICON_SRC not found" >&2
  exit 1
fi

echo "==> cargo build --release"
cargo build --release

echo "==> generating $APP_NAME.icns from $ICON_SRC"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
ICONSET="$TMP/$APP_NAME.iconset"
ICNS="$TMP/$APP_NAME.icns"
mkdir -p "$ICONSET"
for size in 16 32 128 256 512; do
  sips -z "$size" "$size" "$ICON_SRC" \
    --out "$ICONSET/icon_${size}x${size}.png" >/dev/null
  retina=$((size * 2))
  sips -z "$retina" "$retina" "$ICON_SRC" \
    --out "$ICONSET/icon_${size}x${size}@2x.png" >/dev/null
done
iconutil -c icns "$ICONSET" -o "$ICNS"

echo "==> assembling $APP_DIR"
rm -rf "$APP_DIR"
mkdir -p "$APP_DIR/Contents/MacOS" "$APP_DIR/Contents/Resources"
cp "$BIN" "$APP_DIR/Contents/MacOS/$APP_NAME"
cp "$ICNS" "$APP_DIR/Contents/Resources/$APP_NAME.icns"

VERSION="$(grep -m1 '^version' Cargo.toml | sed -E 's/.*"(.*)".*/\1/')"

cat > "$APP_DIR/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>$APP_NAME</string>
  <key>CFBundleDisplayName</key><string>$APP_NAME</string>
  <key>CFBundleExecutable</key><string>$APP_NAME</string>
  <key>CFBundleIdentifier</key><string>com.sny-tanaka.$APP_NAME</string>
  <key>CFBundleIconFile</key><string>$APP_NAME.icns</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleVersion</key><string>$VERSION</string>
  <key>CFBundleShortVersionString</key><string>$VERSION</string>
  <key>CFBundleInfoDictionaryVersion</key><string>6.0</string>
  <key>LSMinimumSystemVersion</key><string>10.15</string>
  <key>NSHighResolutionCapable</key><true/>
</dict>
</plist>
PLIST

printf 'APPL????' > "$APP_DIR/Contents/PkgInfo"

# Nudge Finder/LaunchServices to pick up the freshly written bundle.
touch "$APP_DIR"

echo "==> done: $APP_DIR"
