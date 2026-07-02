#!/usr/bin/env bash
# Build the extractor as a WASM component (wasm32-wasip2 emits components
# directly). The artifact lands in target/wasm32-wasip2/release/.
set -euo pipefail
cd "$(dirname "$0")"
cargo build --release --target wasm32-wasip2
echo "built: target/wasm32-wasip2/release/loadr_plugin_css_select.wasm"
