#!/bin/sh
# Removes Strata Multiview (service, menu bar app, uninstaller, command line tool).
# Data in /Library/Application Support/Strata Multiview is kept unless --purge.
set -u
[ "$(id -u)" -eq 0 ] || { echo "Please run as root: sudo $0 $*" >&2; exit 1; }

/usr/local/strata-multiview/strata-mv-server service uninstall 2>/dev/null || launchctl bootout system/com.stratamultiview.server 2>/dev/null || true
rm -f /Library/LaunchDaemons/com.stratamultiview.server.plist

USER_ID=$(stat -f %u /dev/console 2>/dev/null || echo 0)
[ "$USER_ID" != "0" ] && launchctl bootout "gui/$USER_ID/com.stratamultiview.tray" 2>/dev/null
pkill -x strata-mv-tray 2>/dev/null
rm -f /Library/LaunchAgents/com.stratamultiview.tray.plist

/usr/libexec/ApplicationFirewall/socketfilterfw --remove /usr/local/strata-multiview/strata-mv-server >/dev/null 2>&1
rm -rf "/Applications/Strata Multiview"
rm -f /usr/local/bin/strata-mv-server
rm -f /etc/sudoers.d/strata-multiview
rm -rf /usr/local/strata-multiview
pkgutil --forget com.stratamultiview.server >/dev/null 2>&1

if [ "${1:-}" = "--purge" ]; then
  rm -rf "/Library/Application Support/Strata Multiview" "/Library/Logs/Strata Multiview"
  echo "Strata Multiview and its data have been removed."
else
  echo "Strata Multiview removed. Data kept in /Library/Application Support/Strata Multiview."
fi
exit 0
