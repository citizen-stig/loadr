#!/usr/bin/env bash
# Render VHS terminal demos to mp4 + a poster jpg under site/videos/out/.
#
#   ./render-tapes.sh                 # every *.tape
#   ./render-tapes.sh 51-gen 54-record  # just these
#   ./render-tapes.sh --posters-only    # re-derive posters from existing mp4s
#
# A tape only declares `Output out/<name>.mp4`; VHS has no poster step. The
# <video poster="…"> attribute in site/build-demos.py points at
# /videos/<name>-poster.jpg, and the players use preload="none" — so a missing
# poster renders as a black box until the visitor clicks play.
#
# Frame choice: a fixed timestamp lands on whatever the terminal happened to be
# doing — often a half-typed command or a cleared screen. Instead, sample across
# the back half and keep the largest JPEG. More bytes = more entropy = more text
# on screen, which also rules out blank and mid-`clear` frames.
#
# Requires: vhs, ffmpeg.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$HERE"

OUT_DIR="$HERE/out"
BG="0x0d1117"          # terminal background, for letterbox padding
POSTER_W=800
POSTER_H=450
SAMPLE_PCTS=(40 50 60 70 78 85 90 95)

POSTERS_ONLY=0
if [ "${1:-}" = "--posters-only" ]; then POSTERS_ONLY=1; shift; fi

mkdir -p "$OUT_DIR"

# Extract the highest-content frame of $1.mp4 as $1-poster.jpg.
make_poster() {
  local name="$1" mp4="$OUT_DIR/$1.mp4"
  local info dur secs t sz best="" best_sz=0 tmp
  tmp="$(mktemp -d)"

  # `ffmpeg -i` with no output exits 1 — tolerate it, we only want the banner.
  info=$(ffmpeg -hide_banner -i "$mp4" 2>&1 || true)
  dur=$(printf '%s\n' "$info" | grep -oE 'Duration: [0-9:.]+' | head -1 | cut -d' ' -f2 || true)
  if [ -z "$dur" ]; then
    echo "  !! $name: could not read duration" >&2
    rm -rf "$tmp"; return 1
  fi
  secs=$(echo "$dur" | awk -F: '{print ($1*3600)+($2*60)+$3}')

  for pct in "${SAMPLE_PCTS[@]}"; do
    t=$(awk -v s="$secs" -v p="$pct" 'BEGIN{printf "%.1f", s*p/100}')
    ffmpeg -hide_banner -loglevel error -ss "$t" -i "$mp4" -frames:v 1 \
      -vf "scale=${POSTER_W}:${POSTER_H}:force_original_aspect_ratio=decrease,pad=${POSTER_W}:${POSTER_H}:(ow-iw)/2:(oh-ih)/2:color=${BG}" \
      -q:v 3 -y "$tmp/$pct.jpg" 2>/dev/null || continue
    sz=$(stat -c%s "$tmp/$pct.jpg" 2>/dev/null || echo 0)
    if [ "$sz" -gt "$best_sz" ]; then best_sz=$sz; best=$pct; fi
  done

  if [ -z "$best" ]; then
    echo "  !! $name: no frame extracted" >&2
    rm -rf "$tmp"; return 1
  fi
  cp "$tmp/$best.jpg" "$OUT_DIR/$name-poster.jpg"
  rm -rf "$tmp"
  echo "  $name-poster.jpg (frame @ ${best}% of ${secs}s, ${best_sz}B)"
}

if [ "$#" -gt 0 ]; then
  tapes=()
  for a in "$@"; do tapes+=("${a%.tape}.tape"); done
else
  tapes=(*.tape)
fi

fails=0
for tape in "${tapes[@]}"; do
  name="$(basename "$tape" .tape)"
  if [ "$POSTERS_ONLY" -eq 1 ]; then
    [ -f "$OUT_DIR/$name.mp4" ] || { echo "skip $name (no mp4)"; continue; }
  else
    [ -f "$tape" ] || { echo "skip $name (no tape)" >&2; continue; }
    echo "==> vhs $tape"
    vhs "$tape"
  fi
  [ -f "$OUT_DIR/$name.mp4" ] || { echo "  !! $name: tape produced no mp4" >&2; fails=$((fails+1)); continue; }
  # One bad recording shouldn't abandon the rest of the batch — but still fail.
  make_poster "$name" || fails=$((fails+1))
done

if [ "$fails" -gt 0 ]; then
  echo "$fails demo(s) failed to render" >&2
  exit 1
fi
