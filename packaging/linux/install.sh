#!/bin/sh
# Strata Graphics — Ubuntu installer (systemd).
#
#   sudo ./install.sh                          asks its questions as it goes
#   sudo ./install.sh --yes [options]          no questions (defaults, options)
#   sudo /opt/strata/install.sh ndi            add the NDI runtime later
#   sudo /opt/strata/install.sh decklink       add the DeckLink driver later
#
# Options:
#   --bind ADDR:PORT          web interface and API (first install: 0.0.0.0:8080)
#   --data-dir DIR            data folder (default /var/lib/strata-graphics)
#   --ndi | --no-ndi          install the NDI runtime (downloads the NDI SDK)
#   --ndi-sdk FILE            NDI SDK for Linux already downloaded (.tar.gz)
#   --accept-ndi-license      accept the NDI SDK licence without asking
#   --decklink | --no-decklink
#   --decklink-driver FILE    Blackmagic Desktop Video for Linux (.tar.gz or .deb)
#   --no-admin                do not offer to create the administrator account
set -eu

case "${LC_ALL:-${LC_MESSAGES:-${LANG:-}}}" in fr*) FR=1 ;; *) FR= ;; esac
say() { if [ -n "$FR" ]; then printf '%s\n' "$1"; else printf '%s\n' "$2"; fi; }
step() { printf '\n== %s\n' "$(say "$1" "$2")"; }
warn() { printf '!  %s\n' "$(say "$1" "$2")" >&2; }
die() { printf 'x  %s\n' "$(say "$1" "$2")" >&2; exit 1; }

PREFIX=/opt/strata
UNIT=/etc/systemd/system/strata-graphics.service
NDI_URL=${NDI_SDK_URL:-https://downloads.ndi.tv/SDK/NDI_SDK_Linux/Install_NDI_SDK_v6_Linux.tar.gz}
DECKLINK_PAGE=https://www.blackmagicdesign.com/support/family/capture-and-playback
HERE=$(cd "$(dirname "$0")" && pwd)

YES='' BIND='' DATA_DIR='' NDI='' NDI_SDK='' NDI_LICENSE='' DECKLINK='' DECKLINK_DRIVER='' ADMIN=1 ONLY=''
need() { [ $# -ge 2 ] || die "Il manque la valeur de $1." "$1 needs a value."; }
while [ $# -gt 0 ]; do
  case "$1" in
    -y|--yes) YES=1 ;;
    --bind) need "$@"; BIND=$2; shift ;;
    --bind=*) BIND=${1#*=} ;;
    --data-dir) need "$@"; DATA_DIR=$2; shift ;;
    --data-dir=*) DATA_DIR=${1#*=} ;;
    --ndi) NDI=y ;;
    --no-ndi) NDI=n ;;
    --ndi-sdk) need "$@"; NDI_SDK=$2; NDI=y; shift ;;
    --accept-ndi-license) NDI_LICENSE=1 ;;
    --decklink) DECKLINK=y ;;
    --no-decklink) DECKLINK=n ;;
    --decklink-driver) need "$@"; DECKLINK_DRIVER=$2; DECKLINK=y; shift ;;
    --no-admin) ADMIN= ;;
    ndi|decklink) ONLY="$ONLY $1" ;;
    -h|--help) awk 'NR > 1 && /^#/ { sub(/^# ?/, ""); print; next } NR > 1 { exit }' "$0"; exit 0 ;;
    *) die "Option inconnue : $1 (--help pour l'aide)" "Unknown option: $1 (see --help)" ;;
  esac
  shift
done

[ "$(id -u)" -eq 0 ] || die "Lancez-le en administrateur : sudo $0" "Please run as root: sudo $0"
command -v systemctl >/dev/null || die \
  "systemd est nécessaire (sinon lancez 'strata-server run' avec votre propre système de démarrage)." \
  "systemd is required (run 'strata-server run' under your own init system otherwise)."

# Questions only with someone at the terminal, and without --yes.
INTERACTIVE=
[ -z "$YES" ] && [ -t 0 ] && INTERACTIVE=1

# ask FR EN DEFAULT(y|n): yes? Unattended, the default.
ask() {
  if [ -z "$INTERACTIVE" ]; then [ "$3" = y ]; return; fi
  if [ "$3" = y ]; then hint=$(say "[O/n]" "[Y/n]"); else hint=$(say "[o/N]" "[y/N]"); fi
  printf '%s %s ' "$(say "$1" "$2")" "$hint"
  read -r answer || answer=
  case "$answer" in [oOyY]*) return 0 ;; [nN]*) return 1 ;; *) [ "$3" = y ] ;; esac
}
# prompt FR EN DEFAULT: the answer (unattended, the default).
prompt() {
  if [ -z "$INTERACTIVE" ]; then printf '%s' "$3"; return; fi
  if [ -n "$3" ]; then printf '%s [%s] ' "$(say "$1" "$2")" "$3" >&2; else printf '%s ' "$(say "$1" "$2")" >&2; fi
  read -r answer || answer=
  printf '%s' "${answer:-$3}"
}

APT_READY=
apt_install() {
  command -v apt-get >/dev/null || { warn "apt-get introuvable : installez $* à la main." "apt-get not found: install $* by hand."; return 1; }
  if [ -z "$APT_READY" ]; then apt-get update -qq >/dev/null 2>&1 || true; APT_READY=1; fi
  say "Installation : $*" "Installing: $*"
  log=$(mktemp)
  if DEBIAN_FRONTEND=noninteractive apt-get install -y -qq "$@" >"$log" 2>&1; then
    rm -f "$log"
  else
    grep -E '^(E|W):' "$log" >&2 || tail -5 "$log" >&2
    rm -f "$log"
    return 1
  fi
}

# The data folder and the address of the installed service, if any.
unit_data_dir() {
  [ -f "$UNIT" ] || return 0
  sed -n 's/^ExecStart=.*--data-dir "\([^"]*\)".*/\1/p' "$UNIT" | head -1
}
unit_bind() {
  [ -f "$UNIT" ] || return 0
  sed -n 's/^ExecStart=.*--default-bind \([^ ]*\).*/\1/p' "$UNIT" | head -1
}

# ---------------------------------------------------------------- server ----
install_server() {
  step "Serveur Strata Graphics" "Strata Graphics server"
  [ "$HERE" != "$PREFIX" ] || die \
    "Lancez l'installateur du paquet (ou mettez à jour depuis Réglages › Mises à jour)." \
    "Run the package's installer (or update from Settings › Updates)."
  [ -f "$HERE/strata-server" ] || die "strata-server est introuvable à côté de ce script." "strata-server not found next to this script."
  if [ -f "$UNIT" ]; then
    # Re-install / upgrade: keep the data folder, stop the service first.
    [ -n "$DATA_DIR" ] || DATA_DIR=$(unit_data_dir)
    [ -n "$BIND" ] || BIND=$(unit_bind)
    "$PREFIX/strata-server" service uninstall >/dev/null 2>&1 || true
  elif [ -z "$BIND" ]; then
    port=$(prompt "Port de l'interface web" "Web interface port" 8080)
    case "$port" in ''|*[!0-9]*) die "Port invalide : $port" "Invalid port: $port" ;; esac
    BIND=0.0.0.0:$port
  fi
  # An install from before the rename (Pulse CG): stop it; the service
  # install below moves its data and removes its service account.
  if [ -f /etc/systemd/system/pulse-cg.service ]; then
    systemctl stop pulse-cg 2>/dev/null || true
  fi
  install -d "$PREFIX"
  install -m 0755 "$HERE/strata-server" "$PREFIX/strata-server"
  # This installer stays, to add NDI or DeckLink later.
  install -m 0755 "$HERE/install.sh" "$PREFIX/install.sh"
  [ -f "$HERE/uninstall.sh" ] && install -m 0755 "$HERE/uninstall.sh" "$PREFIX/uninstall.sh"
  cp "$HERE"/*.md "$HERE"/OFL.txt "$PREFIX/" 2>/dev/null || true
  # ffmpeg shipped with the package (streams, inputs, video conversion).
  if [ -d "$HERE/bin" ]; then install -d "$PREFIX/bin" && cp "$HERE"/bin/* "$PREFIX/bin/" && chmod 0755 "$PREFIX/bin/ffmpeg"; fi
  ln -sf "$PREFIX/strata-server" /usr/local/bin/strata-server

  set --
  [ -n "$BIND" ] && set -- "$@" --bind "$BIND"
  [ -n "$DATA_DIR" ] && set -- "$@" --data-dir "$DATA_DIR"
  "$PREFIX/strata-server" service install "$@"
  rm -rf /opt/pulse-cg
  rm -f /usr/local/bin/cg-server
}

# ------------------------------------------------------------------- NDI ----
ndi_installed() { [ -e "$PREFIX/lib/libndi.so.6" ]; }

install_ndi() {
  step "Runtime NDI" "NDI runtime"
  if [ -z "$NDI_LICENSE" ]; then
    say "Le runtime NDI vient du SDK NDI de Vizrt NDI AB, sous sa licence : https://ndi.video/sdk/license/" \
        "The NDI runtime comes from Vizrt NDI AB's NDI SDK, under its licence: https://ndi.video/sdk/license/"
    if [ -z "$INTERACTIVE" ]; then
      warn "NDI non installé : ajoutez --accept-ndi-license pour accepter la licence du SDK NDI." \
           "NDI not installed: add --accept-ndi-license to accept the NDI SDK licence."
      return 0
    fi
    ask "Accepter la licence du SDK NDI ?" "Accept the NDI SDK licence?" n || {
      warn "NDI non installé." "NDI not installed."
      return 0
    }
  fi
  # The runtime finds the NDI sources on the network with Avahi (mDNS).
  apt_install curl ca-certificates libavahi-client3 libavahi-common3 avahi-daemon ||
    warn "Dépendances NDI non installées (libavahi-client3, avahi-daemon)." "NDI dependencies not installed (libavahi-client3, avahi-daemon)."
  systemctl enable --now avahi-daemon >/dev/null 2>&1 || true

  work=$(mktemp -d)
  if [ -n "$NDI_SDK" ]; then
    cp "$NDI_SDK" "$work/sdk.tar.gz" || die "Fichier introuvable : $NDI_SDK" "File not found: $NDI_SDK"
  else
    say "Téléchargement du SDK NDI…" "Downloading the NDI SDK…"
    if ! curl -fsSL --retry 3 -o "$work/sdk.tar.gz" "$NDI_URL"; then
      warn "Téléchargement impossible ($NDI_URL). Téléchargez le SDK NDI pour Linux puis : sudo $PREFIX/install.sh ndi --ndi-sdk <fichier>" \
           "Download failed ($NDI_URL). Download the NDI SDK for Linux, then: sudo $PREFIX/install.sh ndi --ndi-sdk <file>"
      rm -rf "$work"
      return 0
    fi
  fi
  (cd "$work" && tar xzf sdk.tar.gz) || die "Archive du SDK NDI illisible." "Unreadable NDI SDK archive."
  setup=$(find "$work" -maxdepth 2 -name 'Install_NDI_SDK*Linux*.sh' | head -1)
  if [ -n "$setup" ]; then
    # It shows the licence (accepted above) and asks for « y ».
    (cd "$work" && yes y | bash "$setup" >/dev/null 2>&1) || true
    if ! find "$work" -name 'libndi.so*' | grep -q .; then
      # Its archive follows a __MARKER__ line: unpack it directly.
      line=$(awk '/^__[A-Z_]+__$/ { print NR + 1; exit }' "$setup")
      [ -n "$line" ] && (cd "$work" && tail -n +"$line" "$setup" | tar xz 2>/dev/null) || true
    fi
  fi
  arch=$(uname -m)-linux-gnu
  lib=$(find "$work" -path "*$arch*" -name 'libndi.so.*.*' -type f | head -1)
  [ -n "$lib" ] || lib=$(find "$work" -name 'libndi.so.6*' -type f | head -1)
  if [ -z "$lib" ]; then
    rm -rf "$work"
    die "libndi.so introuvable dans le SDK NDI." "libndi.so not found in the NDI SDK."
  fi
  install -d "$PREFIX/lib"
  rm -f "$PREFIX"/lib/libndi.so*
  install -m 0755 "$lib" "$PREFIX/lib/"
  ln -sf "$(basename "$lib")" "$PREFIX/lib/libndi.so.6"
  find "$work" -maxdepth 3 -type f \( -iname '*licen*' -o -iname '*eula*' \) -exec cp {} "$PREFIX/lib/" \; 2>/dev/null || true
  rm -rf "$work"
  missing=$(ldd "$PREFIX/lib/libndi.so.6" 2>/dev/null | grep 'not found' || true)
  if [ -n "$missing" ]; then
    warn "Il manque des bibliothèques au runtime NDI :" "The NDI runtime misses libraries:"
    printf '%s\n' "$missing" >&2
  else
    say "Runtime NDI installé : $PREFIX/lib/$(basename "$lib")" "NDI runtime installed: $PREFIX/lib/$(basename "$lib")"
  fi
}

# -------------------------------------------------------------- DeckLink ----
decklink_cards() { grep -l 0xbdbd /sys/bus/pci/devices/*/vendor 2>/dev/null | wc -l; }
decklink_driver() { dpkg-query -W -f='${Version}' desktopvideo 2>/dev/null || true; }

install_decklink() {
  step "Cartes DeckLink (Blackmagic Desktop Video)" "DeckLink cards (Blackmagic Desktop Video)"
  cards=$(decklink_cards)
  say "Cartes Blackmagic détectées : $cards" "Blackmagic cards found: $cards"
  version=$(decklink_driver)
  if [ -n "$version" ] && [ -z "$DECKLINK_DRIVER" ]; then
    say "Pilote déjà installé : Desktop Video $version" "Driver already installed: Desktop Video $version"
  else
    driver=$DECKLINK_DRIVER
    if [ -z "$driver" ] && [ -n "$INTERACTIVE" ]; then
      say "Blackmagic demande un enregistrement pour télécharger le pilote : sur $DECKLINK_PAGE, « Desktop Video » pour Linux." \
          "Blackmagic asks for a registration to download the driver: on $DECKLINK_PAGE, « Desktop Video » for Linux."
      driver=$(prompt "Chemin de l'archive téléchargée (vide = plus tard) :" "Path of the downloaded archive (empty = later):" "")
      # shellcheck disable=SC2088 # a typed « ~/ », expanded here
      case "$driver" in "~/"*) driver="$(getent passwd "${SUDO_USER:-root}" | cut -d: -f6)/${driver#\~/}" ;; esac
    fi
    if [ -z "$driver" ]; then
      warn "Pilote DeckLink non installé. Plus tard : sudo $PREFIX/install.sh decklink --decklink-driver <archive>" \
           "DeckLink driver not installed. Later: sudo $PREFIX/install.sh decklink --decklink-driver <archive>"
      return 0
    fi
    [ -f "$driver" ] || die "Fichier introuvable : $driver" "File not found: $driver"
    work=$(mktemp -d)
    case "$driver" in
      *.deb) deb=$driver ;;
      *) tar xzf "$driver" -C "$work" || die "Archive Desktop Video illisible." "Unreadable Desktop Video archive."
         deb=$(find "$work" -name 'desktopvideo_*_amd64.deb' | head -1) ;;
    esac
    [ -n "$deb" ] || die "desktopvideo_*_amd64.deb introuvable dans l'archive." "desktopvideo_*_amd64.deb not found in the archive."
    case "$deb" in /*) ;; *) deb=$(pwd)/$deb ;; esac
    # The driver is a kernel module built for this kernel (DKMS).
    apt_install dkms "linux-headers-$(uname -r)" ||
      warn "En-têtes du noyau non installés : le module Blackmagic risque de ne pas se compiler." \
           "Kernel headers not installed: the Blackmagic module may not build."
    if command -v mokutil >/dev/null && mokutil --sb-state 2>/dev/null | grep -qi enabled; then
      warn "Secure Boot est actif : le module Blackmagic doit être signé (clé MOK à valider au redémarrage), ou désactivez Secure Boot." \
           "Secure Boot is on: the Blackmagic module must be signed (MOK key to enrol at reboot), or turn Secure Boot off."
    fi
    apt_install "$deb" || die "Installation de Desktop Video impossible." "Desktop Video could not be installed."
    rm -rf "$work"
    version=$(decklink_driver)
    say "Desktop Video $version installé." "Desktop Video $version installed."
    modprobe blackmagic 2>/dev/null ||
      warn "Module Blackmagic non chargé : redémarrez la machine." "Blackmagic module not loaded: restart the machine."
  fi
  if command -v BlackmagicFirmwareUpdater >/dev/null && [ "$cards" -gt 0 ]; then
    BlackmagicFirmwareUpdater status 2>/dev/null || true
    say "Si un firmware est à mettre à jour : sudo BlackmagicFirmwareUpdater update 0, puis redémarrez." \
        "If a firmware needs updating: sudo BlackmagicFirmwareUpdater update 0, then restart."
  fi
  # The service account must open the cards.
  for dev in /dev/blackmagic/*; do
    [ -e "$dev" ] || continue
    if ! runuser -u strata -- test -r "$dev" -a -w "$dev" 2>/dev/null; then
      usermod -aG "$(stat -c %G "$dev")" strata 2>/dev/null || true
    fi
  done
}

# ----------------------------------------------------------------- admin ----
create_admin() {
  data=${DATA_DIR:-$(unit_data_dir)}
  data=${data:-/var/lib/strata-graphics}
  grep -q '"username"' "$data/users.json" 2>/dev/null && return 0
  [ -n "$ADMIN" ] && [ -n "$INTERACTIVE" ] || return 0
  step "Compte administrateur" "Administrator account"
  ask "Créer le compte administrateur maintenant ?" "Create the administrator account now?" y || return 0
  name=$(prompt "Nom du compte" "Account name" admin)
  "$PREFIX/strata-server" user add "$name" --data-dir "$data" ||
    warn "Compte non créé : sudo strata-server user add $name" "Account not created: sudo strata-server user add $name"
  # Readable by the service, whatever the server's version.
  chown -R strata:strata "$data"
}

# ------------------------------------------------------------------ main ----
if [ -n "$ONLY" ]; then
  [ -x "$PREFIX/strata-server" ] || die "Installez d'abord Strata : sudo ./install.sh" "Install Strata first: sudo ./install.sh"
  for c in $ONLY; do
    case "$c" in
      ndi) install_ndi ;;
      decklink) install_decklink ;;
    esac
  done
else
  install_server
  if [ -z "$NDI" ]; then
    if ndi_installed; then
      NDI=n
    elif ask "Installer le runtime NDI (sorties et entrées NDI) ?" "Install the NDI runtime (NDI outputs and inputs)?" y; then
      NDI=y
    fi
  fi
  [ "$NDI" = y ] && install_ndi
  if [ -z "$DECKLINK" ]; then
    default=n
    [ "$(decklink_cards)" -gt 0 ] && [ -z "$(decklink_driver)" ] && default=y
    ask "Installer le pilote des cartes DeckLink (SDI) ?" "Install the DeckLink cards driver (SDI)?" "$default" && DECKLINK=y
  fi
  [ "$DECKLINK" = y ] && install_decklink
  create_admin
fi

# Web updates (Settings > Updates) replace the program in place: let the
# service account write its own folder (and nothing else on the system).
chown -R strata:strata "$PREFIX"
systemctl restart strata-graphics 2>/dev/null || true

step "Terminé" "Done"
if [ -z "$ONLY" ]; then
  ip=$(hostname -I 2>/dev/null | awk '{print $1}')
  port=${BIND##*:}
  say "Interface : http://${ip:-<serveur>}:${port:-8080}" "Web interface: http://${ip:-<server>}:${port:-8080}"
fi
say "Journal : journalctl -u strata-graphics -f" "Logs: journalctl -u strata-graphics -f"
say "Ajouter NDI ou DeckLink plus tard : sudo $PREFIX/install.sh ndi | decklink" \
    "Add NDI or DeckLink later: sudo $PREFIX/install.sh ndi | decklink"
