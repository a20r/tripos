#!/usr/bin/env bash
# Builds the wasm module and assembles the static site in dist/.
# Needs: rustup target wasm32-unknown-unknown, wasm-bindgen-cli matching Cargo.lock.
set -euo pipefail
cd "$(dirname "$0")"

PROFILE="${1:-release}"
if [ "$PROFILE" = "release" ]; then
  cargo build --release --target wasm32-unknown-unknown -p tripos
  WASM=target/wasm32-unknown-unknown/release/tripos.wasm
else
  cargo build --target wasm32-unknown-unknown -p tripos
  WASM=target/wasm32-unknown-unknown/debug/tripos.wasm
fi

# Cleared in place rather than removed, so a dev server rooted in dist/ keeps working.
mkdir -p dist
find dist -mindepth 1 -delete
mkdir -p dist/pkg
cp -r web/. dist/
wasm-bindgen --target web --no-typescript --out-dir dist/pkg "$WASM"

# Stamp the service worker so every build installs into its own cache.
BUILD="$(git rev-parse --short HEAD 2>/dev/null || echo nogit)-$(date -u +%s)"
sed -i.bak "s/__BUILD__/$BUILD/" dist/sw.js && rm -f dist/sw.js.bak
if grep -q "__BUILD__" dist/sw.js || ! grep -q "$BUILD" dist/sw.js; then
  echo "build.sh: dist/sw.js was not stamped with the build id" >&2
  exit 1
fi
echo "service worker build id: $BUILD"
ls -la dist/pkg
