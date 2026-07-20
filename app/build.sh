#!/usr/bin/env bash
# Build the shell. Two outputs:
#   appshell.wasm      — next to index.html, for local dev (needs http)
#   dist/index.html    — the SINGLE-FILE build: wasm inlined as base64;
#                        self-contained, serves from anywhere, even file://
#
# Local dev:   ./build.sh && python -m http.server -d . 8080
# Deploy:      ./build.sh && vercel deploy dist --prod   (any static host works)
set -eu
cd "$(dirname "$0")"
cargo build --release --target wasm32-unknown-unknown
cp target/wasm32-unknown-unknown/release/appshell.wasm appshell.wasm
mkdir -p dist
python - <<'EOF'
import base64
b64 = base64.b64encode(open("appshell.wasm", "rb").read()).decode()
html = open("index.html", encoding="utf-8").read()
assert "__WASM_B64__" in html, "marker missing"
open("dist/index.html", "w", encoding="utf-8", newline="\n").write(
    html.replace("__WASM_B64__", b64))
print(f"dist/index.html: single file, {len(html) + len(b64)} bytes")
EOF
