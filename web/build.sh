#!/usr/bin/env bash
# Rebuild the WebAssembly module into web/pkg/.
#
# Requires: rustup target add wasm32-unknown-unknown, and wasm-pack
# (cargo install wasm-pack).
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$here/.." && pwd)"

wasm-pack build "$repo_root/crates/wasm" \
    --release \
    --target web \
    --out-dir "$here/pkg" \
    --out-name arduboy

# wasm-pack drops a .gitignore that would hide the build output; we commit the
# prebuilt pkg/ so Cloudflare Pages can serve it with no build step.
rm -f "$here/pkg/.gitignore"

echo "Built web/pkg. Serve locally with:  python -m http.server -d \"$here\" 8080"
