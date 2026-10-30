#!/usr/bin/env bash
# Downloads a static ffmpeg to ship with Strata: stream outputs (SRT, UDP,
# RTMP, HLS), live inputs and video conversion go through it. The server
# looks for it in bin/ next to itself (crates/cg-output/src/ffmpeg.rs).
#
# Usage: packaging/ffmpeg/fetch.sh <platform> <out-dir>
#   platform: linux-x86_64 | linux-arm64 | windows-x86_64 |
#             macos-arm64 | macos-x86_64 | macos (universal)
# Writes <out-dir>/ffmpeg (ffmpeg.exe on Windows) and its licence notice.
# FFMPEG_URL overrides the download (one archive, one architecture).
# A failed download only warns: the package is built without ffmpeg (the
# server then uses the one on the PATH). With FFMPEG_REQUIRED=1 (release
# builds) it fails instead, as does an ffmpeg without SRT or RTMP: an
# installer without them has no stream inputs or outputs.
#
# Builds used: BtbN/FFmpeg-Builds (Linux, Windows; GPL, with libx264,
# libx265, libsrt) and Martin Riedl's macOS builds (arm64 and x86_64, joined
# into one universal binary). When Martin Riedl's site does not answer, the
# macOS ffmpeg is the one an earlier Strata release shipped (gh CLI, with
# GH_TOKEN in CI).
set -uo pipefail
PLATFORM=$1 OUT=$2
mkdir -p "$OUT"
WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT

warn() {
  if [ -n "${FFMPEG_REQUIRED:-}" ]; then
    echo "::error::ffmpeg not bundled: $*"
    exit 1
  fi
  echo "::warning::ffmpeg not bundled: $*"
  exit 0
}

# The ffmpeg found lacks what streams need.
lacks() {
  if [ -n "${FFMPEG_REQUIRED:-}" ]; then
    echo "::error::ffmpeg: $*"
    exit 1
  fi
  echo "::warning::ffmpeg: $*"
}

BTBN=https://github.com/BtbN/FFmpeg-Builds/releases/download/latest
RIEDL=https://ffmpeg.martin-riedl.de/redirect/latest/macos

# Downloads and unpacks an archive, prints where the ffmpeg binary is.
fetch() { # url name
  local url=$1 dir="$WORK/$2"
  mkdir -p "$dir"
  curl -fsSL --retry 3 -o "$dir/archive" "$url" || return 1
  case "$url" in
    *.zip) (cd "$dir" && unzip -q archive) || return 1 ;;
    *) tar xf "$dir/archive" -C "$dir" || return 1 ;;
  esac
  find "$dir" -type f \( -name ffmpeg -o -name ffmpeg.exe \) | head -1
}

# True when this ffmpeg runs here and streams over SRT.
has_srt() { # ffmpeg
  "$1" -hide_banner -protocols 2>/dev/null | grep -qw srt
}

# Martin Riedl's arm64 and x86_64 builds joined into one universal binary.
riedl_universal() { # dest
  local arm x86
  arm=$(fetch "$RIEDL/arm64/release/ffmpeg.zip" arm64) && [ -n "$arm" ] || return 1
  x86=$(fetch "$RIEDL/amd64/release/ffmpeg.zip" x86_64) && [ -n "$x86" ] || return 1
  lipo -create -output "$1" "$arm" "$x86"
}

# The ffmpeg (bin/ffmpeg) of the newest earlier Strata release whose macOS
# package has one that streams over SRT. Sets FROM_RELEASE.
from_release() { # dest
  local repo=${GITHUB_REPOSITORY:-DamienP2/CG} tag dir ff
  command -v gh >/dev/null || return 1
  for tag in $(gh release list --repo "$repo" --limit 30 --exclude-drafts --json tagName --jq '.[].tagName'); do
    dir="$WORK/release-$tag"
    mkdir -p "$dir"
    gh release download "$tag" --repo "$repo" --pattern '*-macos-universal.pkg' --dir "$dir" >/dev/null 2>&1 || continue
    for pkg in "$dir"/*.pkg; do
      pkgutil --payload-files "$pkg" 2>/dev/null | grep -q 'bin/ffmpeg$' || continue
      pkgutil --expand-full "$pkg" "$dir/x" >/dev/null || continue
      ff=$(find "$dir/x" -type f -path '*/bin/ffmpeg' | head -1)
      if [ -n "$ff" ] && has_srt "$ff"; then
        cp "$ff" "$1" && FROM_RELEASE=$tag && return 0
      fi
    done
    rm -rf "$dir"
  done
  return 1
}

EXE=ffmpeg
case "$PLATFORM" in
  linux-x86_64) URL=${FFMPEG_URL:-$BTBN/ffmpeg-master-latest-linux64-gpl.tar.xz} ;;
  linux-arm64) URL=${FFMPEG_URL:-$BTBN/ffmpeg-master-latest-linuxarm64-gpl.tar.xz} ;;
  windows-x86_64) URL=${FFMPEG_URL:-$BTBN/ffmpeg-master-latest-win64-gpl.zip}; EXE=ffmpeg.exe ;;
  macos-arm64) URL=${FFMPEG_URL:-$RIEDL/arm64/release/ffmpeg.zip} ;;
  macos-x86_64) URL=${FFMPEG_URL:-$RIEDL/amd64/release/ffmpeg.zip} ;;
  macos)
    URL="$RIEDL/{arm64,amd64}/release/ffmpeg.zip"
    if ! riedl_universal "$OUT/ffmpeg"; then
      echo "::warning::no ffmpeg from $RIEDL: taking the one of an earlier Strata release"
      from_release "$OUT/ffmpeg" || warn "neither $RIEDL nor an earlier Strata release gave one"
      URL="the macOS package of Strata $FROM_RELEASE (built by https://ffmpeg.martin-riedl.de)"
    fi
    ;;
  *) echo "usage: $0 <linux-x86_64|linux-arm64|windows-x86_64|macos-arm64|macos-x86_64|macos> <out-dir>" >&2; exit 2 ;;
esac

if [ "$PLATFORM" != macos ]; then
  BIN=$(fetch "$URL" one) || warn "download failed ($URL)"
  [ -n "$BIN" ] || warn "ffmpeg not found in $URL"
  cp "$BIN" "$OUT/$EXE"
  # The licence that comes with the build.
  find "$(dirname "$(dirname "$BIN")")" -maxdepth 1 -iname 'LICENSE*' -type f -exec cp {} "$OUT/FFMPEG-LICENSE.txt" \; 2>/dev/null
fi
chmod 0755 "$OUT/$EXE"

cat > "$OUT/FFMPEG-NOTICE.txt" <<NOTICE
This package includes FFmpeg (https://ffmpeg.org), a separate program that
Strata runs for video streams and conversions. This build is licensed under
the GNU General Public License version 3 (it includes libx264 and libx265).
Source code: https://ffmpeg.org/download.html and the build scripts of
https://github.com/BtbN/FFmpeg-Builds (Linux, Windows) or
https://ffmpeg.martin-riedl.de (macOS).
Downloaded from: $URL
NOTICE

# What it can do (on a build machine of the same kind).
if "$OUT/$EXE" -hide_banner -version >/dev/null 2>&1; then
  "$OUT/$EXE" -hide_banner -version | head -1
  PROTOCOLS=$("$OUT/$EXE" -hide_banner -protocols 2>/dev/null)
  ENCODERS=$("$OUT/$EXE" -hide_banner -encoders 2>/dev/null)
  for p in srt rtmp; do
    grep -qw "$p" <<<"$PROTOCOLS" || lacks "no $p protocol"
  done
  grep -qw libx264 <<<"$ENCODERS" || lacks "no libx264 encoder"
else
  lacks "$OUT/$EXE does not run on this machine: its protocols are not checked"
fi
echo "ffmpeg bundled in $OUT:"
ls -la "$OUT"
