#!/usr/bin/env bash
# Set the workspace package version (members inherit via version.workspace = true).
set -euo pipefail
VERSION="${1:?usage: set-version.sh <x.y.z>}"
sed -i -E "0,/^version = \"[0-9]+\.[0-9]+\.[0-9]+\"/s//version = \"${VERSION}\"/" Cargo.toml
# Refresh lockfile entries for the workspace crates (only when a toolchain is present).
if command -v cargo >/dev/null 2>&1; then
  cargo update --workspace >/dev/null 2>&1 || true
fi
echo "set workspace version -> ${VERSION}"
