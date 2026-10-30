#!/usr/bin/env bash
# Downloads the NDI SDK and extracts its runtime library, to ship it with
# Strata (the NDI SDK licence allows redistributing the runtime with an
# application, with its licence and the NDI trademark notice).
#
# Usage: packaging/ndi/fetch-runtime.sh <macos|windows> <out-dir>
# Writes <out-dir>/libndi.dylib (macOS, universal) or
# <out-dir>/Processing.NDI.Lib.x64.dll (Windows), plus the licence files.
# A failed download only warns: the package is built without NDI.
set -uo pipefail
PLATFORM=$1 OUT=$2
mkdir -p "$OUT"
WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT

warn() { echo "::warning::NDI runtime not bundled: $*"; exit 0; }

case "$PLATFORM" in
  macos)
    URL=${NDI_SDK_URL:-https://downloads.ndi.tv/SDK/NDI_SDK_Mac/Install_NDI_SDK_v6_Apple.pkg}
    curl -fsSL --retry 3 -o "$WORK/sdk.pkg" "$URL" || warn "download failed ($URL)"
    pkgutil --expand-full "$WORK/sdk.pkg" "$WORK/x" || warn "cannot expand the SDK package"
    LIB=$(find "$WORK/x" -name 'libndi.dylib' -not -path '*/iOS/*' | grep -i macos | head -1)
    [ -n "$LIB" ] || LIB=$(find "$WORK/x" -name 'libndi.dylib' | head -1)
    [ -n "$LIB" ] || warn "libndi.dylib not found in the SDK"
    # Resolve a symlink to the real file.
    cp -L "$LIB" "$OUT/libndi.dylib"
    lipo -info "$OUT/libndi.dylib" || true
    ;;
  windows)
    URL=${NDI_SDK_URL:-https://downloads.ndi.tv/SDK/NDI_SDK/NDI%206%20SDK.exe}
    curl -fsSL --retry 3 -o "$WORK/sdk.exe" "$URL" || warn "download failed ($URL)"
    # Inno Setup installer: silent install on the build machine.
    "$WORK/sdk.exe" //VERYSILENT //SUPPRESSMSGBOXES //NORESTART //SP- || warn "SDK install failed"
    LIB=$(find "/c/Program Files/NDI" -iname 'Processing.NDI.Lib.x64.dll' 2>/dev/null | grep -i 'x64' | head -1)
    [ -n "$LIB" ] || warn "Processing.NDI.Lib.x64.dll not found after installing the SDK"
    cp "$LIB" "$OUT/"
    SDK_ROOT=$(dirname "$(dirname "$(dirname "$LIB")")")
    find "$SDK_ROOT" -maxdepth 2 \( -iname '*licen*' -o -iname '*eula*' \) -type f -exec cp {} "$OUT/" \; 2>/dev/null || true
    ;;
  *) echo "usage: $0 <macos|windows> <out-dir>" >&2; exit 2 ;;
esac

# Licence files shipped with the SDK (names vary between versions).
[ "$PLATFORM" = macos ] && find "$WORK/x" \( -iname '*libndi_licenses*' -o -iname '*NDI*License*' \) -type f -exec cp {} "$OUT/" \; 2>/dev/null
cat > "$OUT/NDI-NOTICE.txt" <<'NOTICE'
This software includes the NDI(R) runtime library, redistributed under the
NDI SDK licence. NDI(R) is a registered trademark of Vizrt NDI AB.
https://ndi.video
NOTICE
echo "NDI runtime bundled in $OUT:"
ls -la "$OUT"
