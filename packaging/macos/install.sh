#!/bin/sh
# Strata Graphics — macOS installer (launchd). Usage: sudo ./install.sh [--bind 0.0.0.0:8080] [--data-dir DIR]
set -eu
[ "$(id -u)" -eq 0 ] || { echo "Please run as root: sudo $0 $*" >&2; exit 1; }
HERE=$(cd "$(dirname "$0")" && pwd)
PREFIX=/usr/local/strata

if [ -f /Library/LaunchDaemons/com.stratagraphics.server.plist ]; then
  "$PREFIX/strata-server" service uninstall >/dev/null 2>&1 || true
fi
mkdir -p "$PREFIX" /usr/local/bin
cp "$HERE/strata-server" "$PREFIX/strata-server"
chmod 0755 "$PREFIX/strata-server"
cp "$HERE"/*.md "$HERE"/OFL.txt "$PREFIX/" 2>/dev/null || true
# NDI runtime shipped with the package.
if [ -d "$HERE/lib" ]; then mkdir -p "$PREFIX/lib" && cp "$HERE"/lib/* "$PREFIX/lib/"; fi
# ffmpeg shipped with the package (streams, inputs, video conversion).
if [ -d "$HERE/bin" ]; then mkdir -p "$PREFIX/bin" && cp "$HERE"/bin/* "$PREFIX/bin/" && chmod 0755 "$PREFIX/bin/ffmpeg"; fi
# Downloaded binaries are quarantined by Gatekeeper; the archive is not notarized yet.
xattr -dr com.apple.quarantine "$PREFIX" 2>/dev/null || true
ln -sf "$PREFIX/strata-server" /usr/local/bin/strata-server

# The service install also retires an install from before the rename and
# moves its data.
"$PREFIX/strata-server" service install "$@"
rm -rf /usr/local/pulse-cg
rm -f /usr/local/bin/cg-server
echo "Logs: tail -f \"/Library/Logs/Strata Graphics/strata-server.log\""
