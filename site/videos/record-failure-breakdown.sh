#!/usr/bin/env bash
# Record the "failure & error breakdown" web UI demo end to end:
#   1. start the Docker harness (go-httpbin on :8080),
#   2. run examples/26-failure-breakdown.yaml with the management UI on a
#      NON-default port (6471) so it can coexist with other UI instances,
#   3. drive a headless Chromium through the failure breakdown + CSV/report
#      download with Playwright (site/videos/record-failure-breakdown.js),
#   4. transcode the recording to mp4 + a poster jpg under site/videos/out/.
#
# Requires: docker, node + playwright (cached chromium), ffmpeg, a built loadr.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

LOADR="${LOADR:-$ROOT/target/release/loadr}"
PORT="${PORT:-6471}"
NAME="13-failure-breakdown"
WORK="$(mktemp -d)"
OUT_DIR="$ROOT/site/videos/out"
mkdir -p "$OUT_DIR"

cleanup() {
  [ -n "${UI_PID:-}" ] && kill "$UI_PID" 2>/dev/null || true
  rm -rf "$WORK"
}
trap cleanup EXIT

# 1. Harness (idempotent — reuses already-running containers).
docker compose -f examples/harness/docker-compose.yml up -d httpbin >/dev/null 2>&1 || true

# 2. Live run + UI on a non-default port.
"$LOADR" run --ui --ui-bind "127.0.0.1:$PORT" examples/26-failure-breakdown.yaml &
UI_PID=$!
sleep 4

# 3. Record (cached Playwright chromium).
VIDEO_OUT="$WORK/video-out" DOWNLOAD_DIR="$WORK/downloads" \
  LOADR_UI="http://127.0.0.1:$PORT" \
  node site/videos/record-failure-breakdown.js

# 4. Transcode webm -> mp4 + poster.
WEBM="$(ls -t "$WORK"/video-out/*.webm | head -1)"
ffmpeg -y -i "$WEBM" -movflags +faststart -pix_fmt yuv420p \
  -vf "scale=1600:-2" -c:v libx264 -crf 24 -preset veryfast \
  "$OUT_DIR/$NAME.mp4"
ffmpeg -y -i "$WEBM" -vf "select=eq(n\,90),scale=1600:-2" -frames:v 1 \
  "$OUT_DIR/$NAME-poster.jpg"

echo "wrote $OUT_DIR/$NAME.mp4 and $OUT_DIR/$NAME-poster.jpg"
ls -la "$WORK/downloads" || true
