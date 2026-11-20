#!/usr/bin/env bash
# Build the Rust decoder to wasm and lay it out in extension/pkg/.
# Requires: rustup target wasm32-unknown-unknown, wasm-bindgen-cli matching Cargo.toml.
set -euo pipefail
cd "$(dirname "$0")/.."

extra=()
if [[ "${BUILD_STD:-}" == 1 ]]; then
  # For toolchains without a prebuilt wasm32 std (needs rust-src).
  export RUSTC_BOOTSTRAP=1
  extra=(-Zbuild-std=std,panic_abort)
fi

cargo build --release --lib --target wasm32-unknown-unknown "${extra[@]}"
rm -rf extension/pkg
wasm-bindgen --target web --no-typescript --out-dir extension/pkg \
  target/wasm32-unknown-unknown/release/qrqrqr.wasm

if command -v wasm-opt >/dev/null; then
  wasm-opt -Os --enable-bulk-memory --enable-nontrapping-float-to-int --enable-sign-ext \
    extension/pkg/qrqrqr_bg.wasm -o extension/pkg/qrqrqr_bg.wasm
fi
ls -l extension/pkg
