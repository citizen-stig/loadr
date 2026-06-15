#!/usr/bin/env bash
# Build and package a single loadr plugin cdylib for a single target triple.
#
# Produces, under <out-dir> (default: dist/):
#   <plugin-name>-<target>.tar.gz          archive containing the cdylib
#                                           (renamed to the platform `entry`)
#                                           plus the plugin's plugin.toml
#   <plugin-name>-<target>.tar.gz.sha256   the archive's SHA-256 (hex, no name)
#   <plugin-name>-<target>.meta.json       machine-readable facts the index
#                                           generator consumes (name, kind,
#                                           version, target, entry, sha256, …)
#
# `<plugin-name>` is the manifest `[plugin].name` (e.g. `mongo`), NOT the crate
# directory (`loadr-plugin-mongo`). The platform `entry` is derived from the
# crate's `[lib].name` (e.g. `loadr_plugin_mongo`):
#   linux   -> lib<lib>.so
#   darwin  -> lib<lib>.dylib
#   windows -> <lib>.dll
#
# Usage:
#   scripts/build-plugin.sh <crate-dir> <target-triple> [out-dir]
#
# Examples:
#   scripts/build-plugin.sh plugins/loadr-plugin-mongo x86_64-unknown-linux-gnu
#   scripts/build-plugin.sh plugins/loadr-plugin-mongo aarch64-apple-darwin out
#
# Env:
#   CARGO            cargo binary (default: cargo)
#   SKIP_BUILD=1     reuse an already-built cdylib in target/<triple>/release
set -euo pipefail

CRATE_DIR="${1:?usage: build-plugin.sh <crate-dir> <target-triple> [out-dir]}"
TARGET="${2:?usage: build-plugin.sh <crate-dir> <target-triple> [out-dir]}"
OUT_DIR="${3:-dist}"
CARGO="${CARGO:-cargo}"

CRATE_DIR="${CRATE_DIR%/}"
MANIFEST="${CRATE_DIR}/plugin.toml"
CARGO_TOML="${CRATE_DIR}/Cargo.toml"

[ -f "$MANIFEST" ]   || { echo "error: no plugin.toml in $CRATE_DIR" >&2; exit 1; }
[ -f "$CARGO_TOML" ] || { echo "error: no Cargo.toml in $CRATE_DIR" >&2; exit 1; }

# --- read manifest / Cargo.toml ------------------------------------------------
# Minimal TOML field extraction (these files are flat and machine-written).
toml_get() { # <file> <key>  -> first `key = "value"` value
  grep -E "^[[:space:]]*$2[[:space:]]*=" "$1" | head -1 \
    | sed -E 's/^[^=]*=[[:space:]]*"?([^"]*)"?[[:space:]]*$/\1/'
}

PLUGIN_NAME="$(toml_get "$MANIFEST" name)"
PLUGIN_VERSION="$(toml_get "$MANIFEST" version)"
PLUGIN_KIND="$(toml_get "$MANIFEST" kind)"
PLUGIN_DESC="$(grep -E '^[[:space:]]*description[[:space:]]*=' "$MANIFEST" | head -1 \
  | sed -E 's/^[^=]*=[[:space:]]*"(.*)"[[:space:]]*$/\1/')"
CRATE_NAME="$(toml_get "$CARGO_TOML" name)"
# `[lib] name`, falling back to the crate name with dashes -> underscores.
LIB_NAME="$(awk '/^\[lib\]/{f=1;next} /^\[/{f=0} f && /^[[:space:]]*name[[:space:]]*=/{print; exit}' "$CARGO_TOML" \
  | sed -E 's/^[^=]*=[[:space:]]*"?([^"]*)"?[[:space:]]*$/\1/')"
[ -n "$LIB_NAME" ] || LIB_NAME="${CRATE_NAME//-/_}"

[ -n "$PLUGIN_NAME" ]    || { echo "error: plugin.toml missing name" >&2; exit 1; }
[ -n "$PLUGIN_VERSION" ] || { echo "error: plugin.toml missing version" >&2; exit 1; }

# --- platform-specific cdylib + entry names -----------------------------------
case "$TARGET" in
  *windows*) BUILT="${LIB_NAME}.dll";      ENTRY="${LIB_NAME}.dll" ;;
  *apple*|*darwin*) BUILT="lib${LIB_NAME}.dylib"; ENTRY="lib${LIB_NAME}.dylib" ;;
  *)         BUILT="lib${LIB_NAME}.so";    ENTRY="lib${LIB_NAME}.so" ;;
esac

echo "==> plugin '${PLUGIN_NAME}' v${PLUGIN_VERSION} (${PLUGIN_KIND}) crate ${CRATE_NAME} -> ${TARGET}"
echo "    cdylib: ${BUILT}  entry: ${ENTRY}"

# --- build --------------------------------------------------------------------
if [ "${SKIP_BUILD:-0}" != "1" ]; then
  "$CARGO" build --release --locked --target "$TARGET" -p "$CRATE_NAME"
fi

BUILT_PATH="target/${TARGET}/release/${BUILT}"
[ -f "$BUILT_PATH" ] || { echo "error: expected cdylib not found: $BUILT_PATH" >&2; exit 1; }

# --- package ------------------------------------------------------------------
mkdir -p "$OUT_DIR"
STAGE="$(mktemp -d)"
trap 'rm -rf "$STAGE"' EXIT

# The archive is flat: the `entry` library + a plugin.toml whose `entry` field
# matches this platform's library name, so it installs cleanly on any OS.
cp "$BUILT_PATH" "${STAGE}/${ENTRY}"
sed -E "s|^([[:space:]]*entry[[:space:]]*=).*|\\1 \"${ENTRY}\"|" "$MANIFEST" > "${STAGE}/plugin.toml"

ARCHIVE="${PLUGIN_NAME}-${TARGET}.tar.gz"
ARCHIVE_PATH="${OUT_DIR}/${ARCHIVE}"
tar -C "$STAGE" -czf "$ARCHIVE_PATH" "${ENTRY}" plugin.toml

# --- checksum -----------------------------------------------------------------
if command -v sha256sum >/dev/null 2>&1; then
  SHA="$(sha256sum "$ARCHIVE_PATH" | awk '{print $1}')"
else
  SHA="$(shasum -a 256 "$ARCHIVE_PATH" | awk '{print $1}')"
fi
printf '%s\n' "$SHA" > "${ARCHIVE_PATH}.sha256"

# --- metadata for the index generator -----------------------------------------
META_PATH="${OUT_DIR}/${PLUGIN_NAME}-${TARGET}.meta.json"
jq -n \
  --arg name "$PLUGIN_NAME" \
  --arg version "$PLUGIN_VERSION" \
  --arg kind "$PLUGIN_KIND" \
  --arg description "$PLUGIN_DESC" \
  --arg target "$TARGET" \
  --arg entry "$ENTRY" \
  --arg archive "$ARCHIVE" \
  --arg sha256 "$SHA" \
  '{name:$name, version:$version, kind:$kind, description:$description,
    target:$target, entry:$entry, archive:$archive, sha256:$sha256}' \
  > "$META_PATH"

echo "==> wrote ${ARCHIVE_PATH}"
echo "    sha256 ${SHA}"
echo "    meta   ${META_PATH}"
