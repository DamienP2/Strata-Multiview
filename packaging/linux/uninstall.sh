#!/bin/sh
# Removes the service and binaries. Data in /var/lib/strata-graphics is kept unless --purge.
set -eu
[ "$(id -u)" -eq 0 ] || { echo "Please run as root: sudo $0 $*" >&2; exit 1; }
/opt/strata/strata-server service uninstall || true
rm -f /usr/local/bin/strata-server
rm -rf /opt/strata
if [ "${1:-}" = "--purge" ]; then
  rm -rf /var/lib/strata-graphics
  userdel strata 2>/dev/null || true
fi
echo "Strata Graphics removed."
if dpkg -s desktopvideo >/dev/null 2>&1; then
  echo "The DeckLink driver stays (Blackmagic Desktop Video): sudo apt remove desktopvideo to remove it."
fi
