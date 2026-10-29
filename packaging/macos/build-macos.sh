#!/usr/bin/env bash
# Builds the macOS app bundle and disk image for Techvision API Checker.
# Run on macOS: the Apple toolchain (iconutil, hdiutil, lipo) is required.
set -euo pipefail

APP="Techvision API Checker"
BIN=techvision-api-checker
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

VERSION="$(sed -n 's/^version *= *"\([^"]*\)"/\1/p' Cargo.toml | head -1)"
IDENTIFIER="com.techvision.apichecker"
OUT="$ROOT/dist/macos"
BUILD="${OUT}/build"
APPDIR="${BUILD}/${APP}.app"

# aarch64 by default; pass "universal" to build both and lipo them together.
MODE="${1:-host}"

log() { printf '\n=== %s ===\n' "$*"; }

need() { command -v "$1" >/dev/null 2>&1 || { echo "missing required command: $1 (Xcode command line tools?)" >&2; exit 1; }; }
need cargo
need iconutil
need hdiutil
need sips

mkdir -p "$OUT" "$BUILD"

case "$MODE" in
  host)
    log "building release binary for the host architecture"
    cargo build --release --locked
    BINS=("target/release/$BIN")
    ;;
  universal)
    log "building arm64 + x86_64 and combining with lipo"
    need rustup
    rustup target add aarch64-apple-darwin x86_64-apple-darwin >/dev/null
    cargo build --release --locked --target aarch64-apple-darwin
    cargo build --release --locked --target x86_64-apple-darwin
    BINS=("target/aarch64-apple-darwin/release/$BIN" "target/x86_64-apple-darwin/release/$BIN")
    ;;
  *)
    echo "usage: $0 [host|universal]" >&2
    exit 2
    ;;
esac

log "assembling ${APP}.app"
rm -rf "$APPDIR"
mkdir -p "$APPDIR/Contents/MacOS" "$APPDIR/Contents/Resources"

if [[ ${#BINS[@]} -eq 2 ]]; then
  lipo -create "${BINS[@]}" -output "$APPDIR/Contents/MacOS/$BIN"
else
  install -m 755 "${BINS[0]}" "$APPDIR/Contents/MacOS/$BIN"
fi

cat > "$APPDIR/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key><string>${APP}</string>
    <key>CFBundleDisplayName</key><string>${APP}</string>
    <key>CFBundleExecutable</key><string>${BIN}</string>
    <key>CFBundleIdentifier</key><string>${IDENTIFIER}</string>
    <key>CFBundleInfoDictionaryVersion</key><string>6.0</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleShortVersionString</key><string>${VERSION}</string>
    <key>CFBundleVersion</key><string>${VERSION}</string>
    <key>CFBundleIconFile</key><string>AppIcon</string>
    <key>LSMinimumSystemVersion</key><string>11.0</string>
    <key>LSApplicationCategoryType</key><string>public.app-category.developer-tools</string>
    <key>NSHighResolutionCapable</key><true/>
    <key>NSHumanReadableCopyright</key><string>Copyright 2026 Jayasimma D. All rights reserved.</string>
</dict>
</plist>
PLIST

log "generating AppIcon.icns from assets/icon-master.png"
ICONSET="${BUILD}/AppIcon.iconset"
rm -rf "$ICONSET"; mkdir -p "$ICONSET"
# iconutil requires this exact set of names; each is rendered from the master PNG.
for spec in \
  icon_16x16.png:16 icon_16x16@2x.png:32 \
  icon_32x32.png:32 icon_32x32@2x.png:64 \
  icon_128x128.png:128 icon_128x128@2x.png:256 \
  icon_256x256.png:256 icon_256x256@2x.png:512 \
  icon_512x512.png:512 icon_512x512@2x.png:1024
do
  name="${spec%%:*}"
  size="${spec##*:}"
  sips -z "$size" "$size" assets/icon-master.png --out "${ICONSET}/${name}" >/dev/null
done
iconutil -c icns "$ICONSET" -o "$APPDIR/Contents/Resources/AppIcon.icns"

install -m 644 LICENSE.md "$APPDIR/Contents/Resources/LICENSE.md"
printf 'APPL????' > "$APPDIR/Contents/PkgInfo"

log "ad-hoc code signature (replace with a Developer ID for distribution)"
codesign --force --deep --sign - "$APPDIR" || echo "codesign skipped"

log "packaging .dmg and .tar.gz"
DMG="${OUT}/${APP}-${VERSION}-macos.dmg"
rm -f "$DMG"
STAGING="${BUILD}/dmg"
rm -rf "$STAGING"; mkdir -p "$STAGING"
cp -R "$APPDIR" "$STAGING/"
ln -s /Applications "$STAGING/Applications"
hdiutil create -volname "$APP" -srcfolder "$STAGING" -ov -format UDZO "$DMG" >/dev/null

tar -C "$BUILD" -czf "${OUT}/${APP}-${VERSION}-macos.tar.gz" "${APP}.app"

log "artifacts"
ls -lh "$OUT"
(cd "$OUT" && shasum -a 256 ./*.dmg ./*.tar.gz > SHA256SUMS-macos.txt && cat SHA256SUMS-macos.txt)

cat <<'NOTE'

Unsigned build: macOS Gatekeeper will block first launch. Right-click the app and choose
Open, or run:  xattr -dr com.apple.quarantine "/Applications/Techvision API Checker.app"
To distribute normally, sign with a Developer ID and notarise with `notarytool`.
NOTE
