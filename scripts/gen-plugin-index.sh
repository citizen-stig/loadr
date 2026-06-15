#!/usr/bin/env bash
# Regenerate plugins/index.json from the per-artifact *.meta.json files emitted
# by scripts/build-plugin.sh, merging into any existing index so previously
# released plugins/versions/targets are preserved.
#
# The output is the format the `loadr plugin install` resolver consumes:
#
#   { "schema": 1,
#     "plugins": {
#       "<name>": {
#         "kind": "...", "description": "...", "latest": "<semver>",
#         "versions": {
#           "<semver>": {
#             "min_loadr_abi": "<abi>",
#             "artifacts": {
#               "<target>": { "url": "...", "sha256": "...", "entry": "..." }
#   } } } } } }
#
# Usage:
#   scripts/gen-plugin-index.sh <meta-dir> [index-path]
#
# Env:
#   RELEASE_BASE_URL   base for artifact download URLs. Default:
#                        https://github.com/levantar-ai/loadr/releases/download/<tag>
#                      where <tag> comes from $RELEASE_TAG.
#   RELEASE_TAG        the git tag of the plugin release (required unless
#                      RELEASE_BASE_URL is given explicitly).
#   MIN_LOADR_ABI      ABI string recorded per version (default: "1.0").
set -euo pipefail

META_DIR="${1:?usage: gen-plugin-index.sh <meta-dir> [index-path]}"
INDEX_PATH="${2:-plugins/index.json}"
MIN_LOADR_ABI="${MIN_LOADR_ABI:-1.0}"

if [ -n "${RELEASE_BASE_URL:-}" ]; then
  BASE_URL="${RELEASE_BASE_URL%/}"
elif [ -n "${RELEASE_TAG:-}" ]; then
  BASE_URL="https://github.com/levantar-ai/loadr/releases/download/${RELEASE_TAG}"
else
  echo "error: set RELEASE_TAG (or RELEASE_BASE_URL) so artifact URLs resolve" >&2
  exit 1
fi

shopt -s nullglob
METAS=( "$META_DIR"/*.meta.json )
[ "${#METAS[@]}" -gt 0 ] || { echo "error: no *.meta.json under $META_DIR" >&2; exit 1; }

# Seed from the existing index (preserve prior releases) or an empty skeleton.
if [ -f "$INDEX_PATH" ]; then
  INDEX="$(cat "$INDEX_PATH")"
else
  INDEX='{"schema":1,"plugins":{}}'
fi

for meta in "${METAS[@]}"; do
  INDEX="$(jq \
    --arg base "$BASE_URL" \
    --arg abi  "$MIN_LOADR_ABI" \
    --slurpfile m "$meta" '
    ($m[0]) as $a
    | ($a.name) as $name | ($a.version) as $ver | ($a.target) as $tgt
    | .plugins[$name].kind = $a.kind
    | .plugins[$name].description = $a.description
    | .plugins[$name].versions[$ver].min_loadr_abi = $abi
    | .plugins[$name].versions[$ver].artifacts[$tgt] = {
        url: ($base + "/" + $a.archive),
        sha256: $a.sha256,
        entry: $a.entry
      }
    ' <<<"$INDEX")"
done

# Recompute each plugin'\''s `latest` as the highest semver across its versions.
INDEX="$(jq '
  def to_parts: split(".") | map(tonumber? // 0);
  .plugins |= with_entries(
    .value.latest = (
      .value.versions | keys
      | sort_by(to_parts) | last // .value.latest
    )
  )
' <<<"$INDEX")"

mkdir -p "$(dirname "$INDEX_PATH")"
printf '%s\n' "$INDEX" | jq -S '.' > "$INDEX_PATH"
echo "==> wrote $INDEX_PATH"
jq -r '.plugins | to_entries[] | "    \(.key) latest=\(.value.latest) versions=\(.value.versions|keys|join(","))"' "$INDEX_PATH"
