#!/bin/sh
# Builds the web front (Rust + Yew, frontend/) into web/dist, where the server serves it ($ALTIM_WEB_ROOT/dist). It is
# compiled to WebAssembly twice, so that each page downloads only its half:
# - the presentation site (/, legal pages): web/dist/index.html, altim-site-<hash>.js and altim-site-<hash>_bg.wasm;
# - the web app (/app/*): web/dist/app/index.html, altim-app-<hash>.js and altim-app-<hash>_bg.wasm;
# plus the shared styles (global-<hash>.css, app-<hash>.css). Icons and other static files stay in web/public.
# Tools: cargo with the wasm32-unknown-unknown target, wasm-bindgen (same version as the crate, fetched by
# scripts/web-tools.sh into $ALTIM_TOOLS), wasm-opt (optional, smaller .wasm).
set -eu
ROOT=$(cd "$(dirname "$0")/.." && pwd)
OUT="$ROOT/web/dist"
TOOLS=${ALTIM_TOOLS:-$ROOT/target/web-tools}
TARGET_DIR=${CARGO_TARGET_DIR:-$ROOT/target}
PATH="$TOOLS:$PATH"

command -v wasm-bindgen >/dev/null 2>&1 || sh "$ROOT/scripts/web-tools.sh"
# The only time zones the browser code uses (Paris, New York, UTC): the rest of the IANA database stays out of the .wasm.
export CHRONO_TZ_TIMEZONE_FILTER="(Europe/Paris|America/New_York|UTC)"

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
hash() { sha256sum "$1" | cut -c1-16; }
rm -rf "$OUT"
mkdir -p "$OUT/app"
gcss="global-$(hash "$ROOT/frontend/styles/global.css").css"
acss="app-$(hash "$ROOT/frontend/styles/app.css").css"
cp "$ROOT/frontend/styles/global.css" "$OUT/$gcss"
cp "$ROOT/frontend/styles/app.css" "$OUT/$acss"

# One half of the front ($1 = site | app, the cargo feature) and its HTML page ($2).
bundle() {
  echo "-----> Altim : $1 (Rust + Yew → WebAssembly)"
  cargo build --manifest-path "$ROOT/Cargo.toml" -p altim-web --profile wasm-release --target wasm32-unknown-unknown --locked \
    --no-default-features --features "$1"
  dir="$tmp/$1"
  wasm-bindgen --target web --no-typescript --out-dir "$dir" --out-name altim "$TARGET_DIR/wasm32-unknown-unknown/wasm-release/altim_web.wasm"
  if command -v wasm-opt >/dev/null 2>&1 && [ "${ALTIM_WASM_OPT:-1}" != 0 ]; then
    # -O1, not -Oz: the stronger levels shrink the raw file by ~10 % but it then compresses worse (+13 % gzip, +12 %
    # brotli), and the network carries the compressed file (measured on this front, Binaryen 133).
    wasm-opt -O1 --enable-bulk-memory --enable-nontrapping-float-to-int --enable-sign-ext --enable-mutable-globals \
      --enable-reference-types --enable-multivalue -o "$dir/altim_bg.opt.wasm" "$dir/altim_bg.wasm"
    mv "$dir/altim_bg.opt.wasm" "$dir/altim_bg.wasm"
  else
    echo "       (wasm-opt absent : .wasm non optimisé par Binaryen)"
  fi
  wasm="altim-$1-$(hash "$dir/altim_bg.wasm")_bg.wasm"
  glue="altim-$1-$(hash "$dir/altim.js").js"
  cp "$dir/altim_bg.wasm" "$OUT/$wasm"
  cp "$dir/altim.js" "$OUT/$glue"
  # The loader is a file (CSP: script-src 'self', no inline script).
  printf 'import init from "/%s";\ninit({ module_or_path: "/%s" });\n' "$glue" "$wasm" > "$dir/main.js"
  main="main-$1-$(hash "$dir/main.js").js"
  cp "$dir/main.js" "$OUT/$main"
  sed -e "s#{{GLOBAL_CSS}}#$gcss#" -e "s#{{APP_CSS}}#$acss#" -e "s#{{GLUE_JS}}#$glue#" -e "s#{{WASM}}#$wasm#" -e "s#{{MAIN_JS}}#$main#" \
    "$ROOT/frontend/index.html" > "$2"
  echo "       $wasm : $(wc -c < "$OUT/$wasm") octets ($(gzip -9c "$OUT/$wasm" | wc -c) gzip)"
}

bundle site "$OUT/index.html"
bundle app "$OUT/app/index.html"
