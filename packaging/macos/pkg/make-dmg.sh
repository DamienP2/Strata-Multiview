#!/bin/sh
# Wraps the installer in a disk image with the Strata icon on the volume
# and on the package (icons survive the download inside a .dmg, not a .zip).
# Usage: packaging/macos/pkg/make-dmg.sh <installer.pkg> <version> <output.dmg>
set -eu
PKG=$1 VERSION=$2 OUT=$3
HERE=$(cd "$(dirname "$0")" && pwd)
ROOT=$(cd "$HERE/../../.." && pwd)
WORK=$(mktemp -d)
cleanup() {
  [ -d "$WORK/mnt/.fseventsd" ] && hdiutil detach -force "$WORK/mnt" -quiet 2>/dev/null
  rm -rf "$WORK"
  return 0
}
trap cleanup EXIT

# hdiutil fails now and then with "Resource busy" (code 16) while macOS
# still scans or indexes the image: try again a few times.
retry() {
  n=0
  until "$@"; do
    n=$((n + 1))
    [ "$n" -ge 5 ] && return 1
    echo "retrying ($n): $*" >&2
    sleep $((n * 3))
  done
}

# Icon from the artwork.
mkdir -p "$WORK/icon.iconset"
for s in 16 32 128 256 512; do
  sips -z $s $s "$ROOT/assets/icons/strata-macos-1024.png" --out "$WORK/icon.iconset/icon_${s}x${s}.png" >/dev/null
  sips -z $((s * 2)) $((s * 2)) "$ROOT/assets/icons/strata-macos-1024.png" --out "$WORK/icon.iconset/icon_${s}x${s}@2x.png" >/dev/null
done
iconutil -c icns "$WORK/icon.iconset" -o "$WORK/icon.icns"

STAGE="$WORK/stage"
mkdir -p "$STAGE"
NAME="Install Strata Graphics $VERSION.pkg"
cp "$PKG" "$STAGE/$NAME"
# Custom Finder icon on the package file.
osascript -l JavaScript -e "ObjC.import('AppKit'); \$.NSWorkspace.sharedWorkspace.setIconForFileOptions(\$.NSImage.alloc.initWithContentsOfFile('$WORK/icon.icns'), '$STAGE/$NAME', 0)" >/dev/null

# The volume's own icon needs the image mounted (the custom-icon flag sits on
# the volume root). The icon file itself travels in the source folder.
cp "$WORK/icon.icns" "$STAGE/.VolumeIcon.icns"
retry hdiutil create -ov -quiet -fs HFS+ -volname "Strata Graphics $VERSION" -srcfolder "$STAGE" -format UDRW "$WORK/rw.dmg"
mkdir -p "$WORK/mnt"
rm -f "$OUT"
if retry hdiutil attach -quiet -nobrowse -noautoopen -mountpoint "$WORK/mnt" "$WORK/rw.dmg"; then
  # Spotlight indexing the fresh volume is what keeps it busy on the build machines.
  mdutil -i off "$WORK/mnt" >/dev/null 2>&1 || true
  SetFile -a C "$WORK/mnt" 2>/dev/null || xattr -wx com.apple.FinderInfo "0000000000000000040000000000000000000000000000000000000000000000" "$WORK/mnt"
  sync
  DEV=$(hdiutil info | awk -v m="$WORK/mnt" '$NF == m { print $1; exit }')
  if retry hdiutil detach -quiet "$WORK/mnt" || retry hdiutil detach -force -quiet "$WORK/mnt" || { [ -n "$DEV" ] && retry diskutil unmountDisk force "$DEV"; }; then
    retry hdiutil convert -ov -quiet "$WORK/rw.dmg" -format UDZO -o "$OUT"
  else
    # The volume stays busy: an image straight from the folder (no custom
    # icon on the volume, the package keeps its own) rather than no image.
    echo "the image cannot be unmounted: building it without the volume icon" >&2
    retry hdiutil create -ov -quiet -fs HFS+ -volname "Strata Graphics $VERSION" -srcfolder "$STAGE" -format UDZO "$OUT"
  fi
else
  echo "the image cannot be mounted: building it without the volume icon" >&2
  retry hdiutil create -ov -quiet -fs HFS+ -volname "Strata Graphics $VERSION" -srcfolder "$STAGE" -format UDZO "$OUT"
fi
echo "built $OUT"
exit 0
