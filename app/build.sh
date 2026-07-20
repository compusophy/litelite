#!/usr/bin/env bash
# Build the shell wasm and put it next to index.html. Then serve the page
# from THIS directory (wasm needs http, not file://):
#
#   ./build.sh && python -m http.server -d . 8080
#   -> http://localhost:8080
set -eu
cd "$(dirname "$0")"
cargo build --release --target wasm32-unknown-unknown
cp target/wasm32-unknown-unknown/release/appshell.wasm appshell.wasm
ls -la appshell.wasm
