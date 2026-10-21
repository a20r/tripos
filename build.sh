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

rm -rf dist
mkdir -p dist/pkg
cp -r web/. dist/
wasm-bindgen --target web --no-typescript --out-dir dist/pkg "$WASM"
ls -la dist/pkg
