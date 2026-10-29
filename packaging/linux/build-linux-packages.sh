#!/usr/bin/env bash
# Builds the Linux packages for Techvision API Checker: .deb, .rpm and AppImage.
# Run this on Linux (CI uses ubuntu-24.04). Requires cargo, and network access the
# first time so the cargo packaging tools can be installed.
set -euo pipefail

APP=techvision-api-checker
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

VERSION="$(sed -n 's/^version *= *"\([^"]*\)"/\1/p' Cargo.toml | head -1)"
OUT="$ROOT/dist/linux"
mkdir -p "$OUT"

log() { printf '\n=== %s ===\n' "$*"; }

need() {
  command -v "$1" >/dev/null 2>&1 || { echo "missing required command: $1" >&2; exit 1; }
}

need cargo

log "building release binary ($VERSION)"
cargo build --release --locked

if [[ ! -x "target/release/$APP" ]]; then
  echo "expected target/release/$APP after cargo build" >&2
  exit 1
fi

log "deb (cargo-deb)"
command -v cargo-deb >/dev/null 2>&1 || cargo install cargo-deb --locked
cargo deb --no-build --output "$OUT/${APP}_${VERSION}_amd64.deb"

log "rpm (cargo-generate-rpm)"
command -v cargo-generate-rpm >/dev/null 2>&1 || cargo install cargo-generate-rpm --locked
# No --target: the binary was built into target/release, and passing a triple makes
# cargo-generate-rpm look under target/<triple>/release instead.
cargo generate-rpm -o "$OUT"
# cargo-generate-rpm names the file itself; move anything left in target into dist/linux.
find target -maxdepth 2 -name "${APP}-*.rpm" -exec mv -f {} "$OUT/" \; 2>/dev/null || true

log "AppImage"
APPDIR="$ROOT/dist/AppDir"
rm -rf "$APPDIR"
mkdir -p "$APPDIR/usr/bin" \
         "$APPDIR/usr/share/applications" \
         "$APPDIR/usr/share/doc/${APP}" \
         "$APPDIR/usr/share/icons/hicolor/256x256/apps"

install -m 755 "target/release/$APP" "$APPDIR/usr/bin/$APP"
install -m 644 "packaging/linux/${APP}.desktop" "$APPDIR/usr/share/applications/${APP}.desktop"
install -m 644 "packaging/linux/icons/256x256/${APP}.png" \
               "$APPDIR/usr/share/icons/hicolor/256x256/apps/${APP}.png"
install -m 644 LICENSE.md "$APPDIR/usr/share/doc/${APP}/LICENSE.md"

# AppImage looks for the desktop file and icon at the AppDir root too.
cp "$APPDIR/usr/share/applications/${APP}.desktop" "$APPDIR/${APP}.desktop"
cp "$APPDIR/usr/share/icons/hicolor/256x256/apps/${APP}.png" "$APPDIR/${APP}.png"
cp "$APPDIR/${APP}.png" "$APPDIR/.DirIcon"

cat > "$APPDIR/AppRun" <<'APPRUN'
#!/usr/bin/env bash
# Resolve the AppImage mount point so the bundled binary is used, not a system copy.
HERE="$(dirname "$(readlink -f "${BASH_SOURCE[0]}")")"
export LD_LIBRARY_PATH="$HERE/usr/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
exec "$HERE/usr/bin/techvision-api-checker" "$@"
APPRUN
chmod +x "$APPDIR/AppRun"

if command -v appimagetool >/dev/null 2>&1; then
  ARCH=x86_64 appimagetool "$APPDIR" "$OUT/${APP}-${VERSION}-x86_64.AppImage"
else
  echo "appimagetool not found: leaving $APPDIR ready, and writing a tarball instead" >&2
  tar -C "$(dirname "$APPDIR")" -czf "$OUT/${APP}-${VERSION}-x86_64.AppDir.tar.gz" "$(basename "$APPDIR")"
fi

log "artifacts"
ls -lh "$OUT"
