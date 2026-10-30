#!/bin/sh
# Removes the service and binaries. Data in /var/lib/strata-multiview is kept unless --purge.
set -eu
[ "$(id -u)" -eq 0 ] || { echo "Please run as root: sudo $0 $*" >&2; exit 1; }
/opt/strata-multiview/strata-mv-server service uninstall || true
rm -f /usr/local/bin/strata-mv-server
rm -rf /opt/strata-multiview
if [ "${1:-}" = "--purge" ]; then
  rm -rf /var/lib/strata-multiview
  userdel strata-mv 2>/dev/null || true
fi
echo "Strata Multiview removed."
if dpkg -s desktopvideo >/dev/null 2>&1; then
  echo "The DeckLink driver stays (Blackmagic Desktop Video): sudo apt remove desktopvideo to remove it."
fi
