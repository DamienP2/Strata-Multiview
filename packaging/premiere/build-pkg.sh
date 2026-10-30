#!/bin/sh
# Builds the macOS installer of the Premiere panel. The extension goes to the
# current user's CEP folder (where the panel can update itself), so the
# package only stages it and its postinstall copies it for the user.
# Usage: packaging/premiere/build-pkg.sh <version> <output.pkg>
# Optional signing: MACOS_INSTALLER_IDENTITY ("Developer ID Installer: …").
set -eu
VERSION=$1 OUT=$2
HERE=$(cd "$(dirname "$0")" && pwd)
WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT

STAGE="$WORK/root/Library/Application Support/Strata/Premiere"
"$HERE/stage.sh" "$STAGE"
mkdir -p "$WORK/scripts"
cp "$HERE/postinstall" "$WORK/scripts/postinstall"
chmod 0755 "$WORK/scripts/postinstall"

pkgbuild --root "$WORK/root" --scripts "$WORK/scripts" \
  --identifier io.stratagraphics.premiere --version "$VERSION" --install-location / \
  ${MACOS_INSTALLER_IDENTITY:+--sign "$MACOS_INSTALLER_IDENTITY"} "$OUT"
echo "built $OUT"
