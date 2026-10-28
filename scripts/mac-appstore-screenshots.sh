#!/usr/bin/env bash
# Mac App Store screenshots from window captures: each is scaled to fit
# 2880x1800 (16:10, the largest size App Store Connect takes for macOS) and
# padded with the window background colour, so any window size works. A
# window sized to 1440x900 points on a Retina screen needs no padding.
# Usage: scripts/mac-appstore-screenshots.sh OUT_DIR capture.png...
# Capture a window without its shadow: ⌘⇧4, Space, then ⌥-click the window.
set -euo pipefail

W=2880 H=1800 PAD=1E1E1E
[[ $# -ge 2 ]] || { echo "usage: $0 OUT_DIR capture.png..." >&2; exit 2; }
OUT=$1; shift
mkdir -p "$OUT"

n=0
for src in "$@"; do
  n=$((n + 1))
  dst="$OUT/$(printf '%02d' "$n")-$(basename "${src%.*}").png"
  w=$(sips -g pixelWidth "$src" | awk '/pixelWidth/ {print $2}')
  h=$(sips -g pixelHeight "$src" | awk '/pixelHeight/ {print $2}')
  # Fit inside WxH, keeping the aspect ratio.
  if (( w * H > h * W )); then nw=$W nh=$(( h * W / w )); else nh=$H nw=$(( w * H / h )); fi
  sips -s format png -z "$nh" "$nw" "$src" --out "$dst" >/dev/null
  sips -p "$H" "$W" --padColor "$PAD" "$dst" >/dev/null 2>&1
  # App Store Connect refuses images with an alpha channel.
  sips -s format jpeg "$dst" --out "${dst%.png}.jpg" >/dev/null && rm "$dst"
  echo "${dst%.png}.jpg (${w}x${h} -> ${W}x${H})"
done
