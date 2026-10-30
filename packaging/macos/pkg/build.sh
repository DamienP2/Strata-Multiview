#!/bin/sh
# Builds the macOS installer package from universal binaries.
# Usage: packaging/macos/pkg/build.sh <strata-server> <strata-tray> <version> <output.pkg>
# Optional signing: MACOS_APP_IDENTITY ("Developer ID Application: …") for the
# apps and binaries, MACOS_INSTALLER_IDENTITY ("Developer ID Installer: …")
# for the package.
set -eu
BIN=$1 TRAY=$2 VERSION=$3 OUT=$4
HERE=$(cd "$(dirname "$0")" && pwd)
ROOT=$(cd "$HERE/../../.." && pwd)
WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT

# 1024 px artwork -> .icns with every size macOS asks for.
make_icns() { # source.png dest.icns
  set_dir="$WORK/icon.iconset"
  rm -rf "$set_dir" && mkdir -p "$set_dir"
  for s in 16 32 128 256 512; do
    sips -z $s $s "$1" --out "$set_dir/icon_${s}x${s}.png" >/dev/null
    sips -z $((s * 2)) $((s * 2)) "$1" --out "$set_dir/icon_${s}x${s}@2x.png" >/dev/null
  done
  iconutil -c icns "$set_dir" -o "$2"
}

# App bundle: name, executable source, executable name, Info.plist dir, icon png
make_app() {
  app="$APPS/$1.app"
  mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
  cp "$2" "$app/Contents/MacOS/$3"
  chmod 0755 "$app/Contents/MacOS/$3"
  sed "s/__VERSION__/$VERSION/g" "$4/Info.plist" > "$app/Contents/Info.plist"
  for lproj in "$4"/*.lproj; do cp -R "$lproj" "$app/Contents/Resources/"; done
  make_icns "$5" "$app/Contents/Resources/AppIcon.icns"
  printf 'APPL????' > "$app/Contents/PkgInfo"
}

# --- payload -----------------------------------------------------------------
PAYLOAD="$WORK/root/usr/local/strata"
mkdir -p "$PAYLOAD"
cp "$BIN" "$PAYLOAD/strata-server"
chmod 0755 "$PAYLOAD/strata-server"
cp "$ROOT/packaging/macos/uninstall.sh" "$ROOT/docs/INSTALL.md" "$ROOT/docs/outputs.example.json" "$ROOT/assets/fonts/OFL.txt" "$PAYLOAD/"
chmod 0755 "$PAYLOAD/uninstall.sh"

# NDI runtime (fetched by packaging/ndi/fetch-runtime.sh), loaded by the
# server from lib/ next to it.
if [ -n "${NDI_DIR:-}" ] && [ -f "$NDI_DIR/libndi.dylib" ]; then
  mkdir -p "$PAYLOAD/lib"
  cp "$NDI_DIR"/* "$PAYLOAD/lib/"
  chmod 0644 "$PAYLOAD/lib/libndi.dylib"
  echo "bundling the NDI runtime"
else
  echo "warning: no NDI runtime to bundle (NDI_DIR)"
fi

# ffmpeg (packaging/ffmpeg/fetch.sh): streams in and out, video conversion.
if [ -n "${FFMPEG_DIR:-}" ] && [ -f "$FFMPEG_DIR/ffmpeg" ]; then
  mkdir -p "$PAYLOAD/bin"
  cp "$FFMPEG_DIR"/* "$PAYLOAD/bin/"
  chmod 0755 "$PAYLOAD/bin/ffmpeg"
  echo "bundling ffmpeg"
else
  echo "warning: no ffmpeg to bundle (FFMPEG_DIR)"
fi

APPS="$WORK/root/Applications/Strata Graphics"
mkdir -p "$APPS"
make_app "Strata Graphics" "$TRAY" strata-tray "$HERE/app" "$ROOT/assets/icons/strata-macos-1024.png"
make_app "Uninstall Strata Graphics" "$HERE/uninstaller/uninstall" uninstall "$HERE/uninstaller" "$ROOT/assets/icons/uninstall-macos-1024.png"

mkdir -p "$WORK/root/Library/LaunchAgents"
cp "$HERE/com.stratagraphics.tray.plist" "$WORK/root/Library/LaunchAgents/"

# --- signing (optional) --------------------------------------------------------
if [ -n "${MACOS_APP_IDENTITY:-}" ]; then
  sign() { codesign --force --options runtime --timestamp --sign "$MACOS_APP_IDENTITY" "$@"; }
  # Same team as the server, so the hardened runtime loads it.
  [ -f "$PAYLOAD/lib/libndi.dylib" ] && sign "$PAYLOAD/lib/libndi.dylib"
  [ -f "$PAYLOAD/bin/ffmpeg" ] && sign "$PAYLOAD/bin/ffmpeg"
  sign "$PAYLOAD/strata-server"
  sign "$APPS/Strata Graphics.app"
  sign "$APPS/Uninstall Strata Graphics.app"
fi

# --- package ---------------------------------------------------------------------
# Always install the apps where we put them, even if a copy exists elsewhere.
pkgbuild --analyze --root "$WORK/root" "$WORK/component.plist"
i=0
while /usr/libexec/PlistBuddy -c "Print :$i" "$WORK/component.plist" >/dev/null 2>&1; do
  /usr/libexec/PlistBuddy -c "Set :$i:BundleIsRelocatable false" "$WORK/component.plist"
  i=$((i + 1))
done

pkgbuild --root "$WORK/root" --component-plist "$WORK/component.plist" --scripts "$HERE/scripts" \
  --identifier com.stratagraphics.server --version "$VERSION" --install-location / \
  "$WORK/component.pkg"
sed "s/__VERSION__/$VERSION/" "$HERE/Distribution.xml" > "$WORK/Distribution.xml"

SIGN=""
if [ -n "${MACOS_INSTALLER_IDENTITY:-}" ]; then SIGN="--sign"; fi
productbuild --distribution "$WORK/Distribution.xml" --resources "$HERE/resources" \
  --package-path "$WORK" ${SIGN:+$SIGN "$MACOS_INSTALLER_IDENTITY"} "$OUT"
echo "built $OUT"
exit 0
