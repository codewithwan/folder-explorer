#!/usr/bin/env bash
# Build a macOS app bundle and a drag-to-Applications .dmg for Folder Explorer.
#
# Usage: packaging/macos/bundle.sh
# Output: target/macos/Folder Explorer.app  and  target/macos/FolderExplorer.dmg
#
# Needs: cargo, sips, iconutil, hdiutil (macOS built-ins) and ImageMagick
# (`magick`) to read the Windows .ico used as the icon source.
set -euo pipefail

APP_NAME="Folder Explorer"
BUNDLE_ID="com.codewithwan.folder-explorer"
BIN="folder-explorer"
ICO="assets/app.ico"

# Resolve repo root (two levels up from this script).
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

OUT="target/macos"
APP="$OUT/$APP_NAME.app"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

echo "==> Building release binary"
cargo build --release

echo "==> Generating AppIcon.icns from $ICO"
magick "$ICO" "$WORK/frame-%02d.png" 2>/dev/null || magick "$ICO" "$WORK/frame-00.png"
# Pick the largest frame the .ico contains.
BEST=""; BEST_W=0
for f in "$WORK"/frame-*.png; do
    w=$(sips -g pixelWidth "$f" 2>/dev/null | awk '/pixelWidth/{print $2}')
    if [ -n "${w:-}" ] && [ "$w" -gt "$BEST_W" ]; then BEST_W=$w; BEST=$f; fi
done
echo "    largest source icon: ${BEST_W}px"
ICONSET="$WORK/AppIcon.iconset"
mkdir -p "$ICONSET"
for s in 16 32 128 256 512; do
    sips -z "$s" "$s" "$BEST" --out "$ICONSET/icon_${s}x${s}.png" >/dev/null
    d=$((s * 2))
    sips -z "$d" "$d" "$BEST" --out "$ICONSET/icon_${s}x${s}@2x.png" >/dev/null
done
iconutil -c icns "$ICONSET" -o "$WORK/AppIcon.icns"

echo "==> Assembling $APP"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "target/release/$BIN" "$APP/Contents/MacOS/$BIN"
cp "$WORK/AppIcon.icns" "$APP/Contents/Resources/AppIcon.icns"
VERSION="$(sed -n 's/^version *= *"\(.*\)"/\1/p' Cargo.toml | head -1)"
cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key>            <string>$APP_NAME</string>
    <key>CFBundleDisplayName</key>     <string>$APP_NAME</string>
    <key>CFBundleIdentifier</key>      <string>$BUNDLE_ID</string>
    <key>CFBundleVersion</key>         <string>$VERSION</string>
    <key>CFBundleShortVersionString</key> <string>$VERSION</string>
    <key>CFBundleExecutable</key>      <string>$BIN</string>
    <key>CFBundleIconFile</key>        <string>AppIcon</string>
    <key>CFBundlePackageType</key>     <string>APPL</string>
    <key>LSMinimumSystemVersion</key>  <string>11.0</string>
    <key>NSHighResolutionCapable</key> <true/>
</dict>
</plist>
PLIST

echo "==> Creating DMG"
DMG="$OUT/FolderExplorer.dmg"
STAGE="$WORK/dmg"
mkdir -p "$STAGE"
cp -R "$APP" "$STAGE/"
ln -s /Applications "$STAGE/Applications"
rm -f "$DMG"
hdiutil create -volname "$APP_NAME" -srcfolder "$STAGE" -ov -format UDZO "$DMG" >/dev/null

echo "==> Done"
echo "    app: $APP"
echo "    dmg: $DMG"
