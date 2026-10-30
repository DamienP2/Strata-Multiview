#!/bin/sh
# Installs the Strata panel for Premiere Pro (current user).
# Usage: ./install-mac.sh [server URL]   (from the unzipped folder)
# The panel then updates itself from the Strata server.
set -eu
HERE=$(cd "$(dirname "$0")" && pwd)
if [ -d "$HERE/io.stratagraphics.premiere" ]; then SRC="$HERE/io.stratagraphics.premiere"; else SRC=$(cd "$HERE/.." && pwd); fi
DEST="$HOME/Library/Application Support/Adobe/CEP/extensions/io.stratagraphics.premiere"
rm -rf "$DEST" "$HOME/Library/Application Support/Adobe/CEP/extensions/io.pulsecg.premiere"
mkdir -p "$DEST"
for item in CSXS host ui index.html loader.js .debug strata.ini; do
  if [ -e "$SRC/$item" ]; then cp -R "$SRC/$item" "$DEST/"; fi
done
if [ $# -gt 0 ]; then printf '[strata]\nserver=%s\n' "$1" > "$DEST/strata.ini"; fi
# The panel is not signed by Adobe Exchange: allow unsigned extensions.
for v in 9 10 11 12 13 14 15 16; do
  defaults write "com.adobe.CSXS.$v" PlayerDebugMode 1
done
echo "Installed in $DEST"
echo "Restart Premiere Pro, then open Window > Extensions > Strata."
