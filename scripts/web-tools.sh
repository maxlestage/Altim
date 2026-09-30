#!/bin/sh
# Tools of the web build, as prebuilt binaries (fast, no compilation) into $ALTIM_TOOLS (default target/web-tools):
# - the wasm32 target of the current Rust toolchain;
# - wasm-bindgen, at the exact version of the `wasm-bindgen` crate in Cargo.lock (they must match);
# - wasm-opt (Binaryen), optional: ALTIM_WASM_OPT=0 skips it (a slightly larger .wasm).
set -eu
ROOT=$(cd "$(dirname "$0")/.." && pwd)
TOOLS=${ALTIM_TOOLS:-$ROOT/target/web-tools}
BINARYEN=${ALTIM_BINARYEN:-133}
mkdir -p "$TOOLS"

rustup target add wasm32-unknown-unknown >/dev/null 2>&1 || rustup target add wasm32-unknown-unknown

wb=$(awk '/^name = "wasm-bindgen"$/{getline; gsub(/version = |"/, ""); print; exit}' "$ROOT/Cargo.lock")
if [ "$("$TOOLS/wasm-bindgen" --version 2>/dev/null | cut -d' ' -f2)" != "$wb" ]; then
  echo "-----> wasm-bindgen $wb"
  curl -fsSL "https://github.com/wasm-bindgen/wasm-bindgen/releases/download/$wb/wasm-bindgen-$wb-x86_64-unknown-linux-musl.tar.gz" \
    | tar -xz -C "$TOOLS" --strip-components=1 "wasm-bindgen-$wb-x86_64-unknown-linux-musl/wasm-bindgen"
fi

if [ "${ALTIM_WASM_OPT:-1}" != 0 ] && ! "$TOOLS/wasm-opt" --version >/dev/null 2>&1; then
  echo "-----> wasm-opt (Binaryen $BINARYEN)"
  curl -fsSL "https://github.com/WebAssembly/binaryen/releases/download/version_$BINARYEN/binaryen-version_$BINARYEN-x86_64-linux.tar.gz" \
    | tar -xz -C "$TOOLS" --strip-components=2 "binaryen-version_$BINARYEN/bin/wasm-opt" || echo "       wasm-opt indisponible (facultatif)"
fi
