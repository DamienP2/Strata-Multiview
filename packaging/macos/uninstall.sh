#!/bin/sh
# Removes Strata Graphics (service, menu bar app, uninstaller, command line tool).
# Data in /Library/Application Support/Strata Graphics is kept unless --purge.
set -u
[ "$(id -u)" -eq 0 ] || { echo "Please run as root: sudo $0 $*" >&2; exit 1; }

/usr/local/strata/strata-server service uninstall 2>/dev/null || launchctl bootout system/com.stratagraphics.server 2>/dev/null || true
rm -f /Library/LaunchDaemons/com.stratagraphics.server.plist

USER_ID=$(stat -f %u /dev/console 2>/dev/null || echo 0)
[ "$USER_ID" != "0" ] && launchctl bootout "gui/$USER_ID/com.stratagraphics.tray" 2>/dev/null
pkill -x strata-tray 2>/dev/null
rm -f /Library/LaunchAgents/com.stratagraphics.tray.plist

/usr/libexec/ApplicationFirewall/socketfilterfw --remove /usr/local/strata/strata-server >/dev/null 2>&1
rm -rf "/Applications/Strata Graphics"
rm -f /usr/local/bin/strata-server
rm -f /etc/sudoers.d/strata-graphics
rm -rf /usr/local/strata
pkgutil --forget com.stratagraphics.server >/dev/null 2>&1

if [ "${1:-}" = "--purge" ]; then
  rm -rf "/Library/Application Support/Strata Graphics" "/Library/Logs/Strata Graphics"
  echo "Strata Graphics and its data have been removed."
else
  echo "Strata Graphics removed. Data kept in /Library/Application Support/Strata Graphics."
fi
exit 0
