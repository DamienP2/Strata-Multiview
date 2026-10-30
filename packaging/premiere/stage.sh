#!/bin/sh
# Lays out the Premiere panel for the installers, like the zip the server
# hands out: <out>/io.stratagraphics.premiere/ (the extension) and the install scripts.
# Usage: packaging/premiere/stage.sh <out dir>
set -eu
ROOT=$(cd "$(dirname "$0")/../.." && pwd)
SRC="$ROOT/integrations/premiere"
OUT=$1
rm -rf "$OUT"
mkdir -p "$OUT/io.stratagraphics.premiere"
for item in CSXS host ui index.html loader.js .debug; do
  cp -R "$SRC/$item" "$OUT/io.stratagraphics.premiere/"
done
cp "$SRC/install/install-mac.sh" "$SRC/install/install-windows.ps1" "$SRC/README.md" "$OUT/"
chmod 0755 "$OUT/install-mac.sh"
