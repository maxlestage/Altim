#!/bin/sh
# Builds the web front into web/dist, where the server serves it ($ALTIM_WEB_ROOT/dist):
# - the Rust + Yew front (frontend/, WebAssembly) owns every page: index.html, the hashed .js/.wasm/.css;
# - during the migration (phases 1-2), the React web app is also built into web/dist/app and the server serves it for
#   /app/* while the Yew screens are being ported (ALTIM_REACT_APP=0, or no Bun, skips it: Yew then serves /app too).
# Tools: cargo with the wasm32-unknown-unknown target, wasm-bindgen (same version as the crate, fetched by
# scripts/web-tools.sh into $ALTIM_TOOLS), wasm-opt (optional, smaller .wasm).
set -eu
ROOT=$(cd "$(dirname "$0")/.." && pwd)
OUT="$ROOT/web/dist"
TOOLS=${ALTIM_TOOLS:-$ROOT/target/web-tools}
TARGET_DIR=${CARGO_TARGET_DIR:-$ROOT/target}
PATH="$TOOLS:$PATH"

command -v wasm-bindgen >/dev/null 2>&1 || sh "$ROOT/scripts/web-tools.sh"

echo "-----> Altim : front Rust + Yew (WebAssembly)"
cargo build --manifest-path "$ROOT/Cargo.toml" -p altim-web --profile wasm-release --target wasm32-unknown-unknown --locked
WASM_IN="$TARGET_DIR/wasm32-unknown-unknown/wasm-release/altim_web.wasm"

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
wasm-bindgen --target web --no-typescript --out-dir "$tmp" --out-name altim "$WASM_IN"
if command -v wasm-opt >/dev/null 2>&1 && [ "${ALTIM_WASM_OPT:-1}" != 0 ]; then
  wasm-opt -Oz --enable-bulk-memory --enable-nontrapping-float-to-int --enable-sign-ext --enable-mutable-globals \
    --enable-reference-types --enable-multivalue -o "$tmp/altim_bg.opt.wasm" "$tmp/altim_bg.wasm"
  mv "$tmp/altim_bg.opt.wasm" "$tmp/altim_bg.wasm"
else
  echo "       (wasm-opt absent : .wasm non optimisé par Binaryen)"
fi

hash() { sha256sum "$1" | cut -c1-16; }
rm -rf "$OUT"
mkdir -p "$OUT"
wasm="altim-$(hash "$tmp/altim_bg.wasm")_bg.wasm"
glue="altim-$(hash "$tmp/altim.js").js"
cp "$tmp/altim_bg.wasm" "$OUT/$wasm"
cp "$tmp/altim.js" "$OUT/$glue"
# The loader is a file (CSP: script-src 'self', no inline script).
printf 'import init from "/%s";\ninit({ module_or_path: "/%s" });\n' "$glue" "$wasm" > "$tmp/main.js"
main="main-$(hash "$tmp/main.js").js"
cp "$tmp/main.js" "$OUT/$main"
gcss="global-$(hash "$ROOT/frontend/styles/global.css").css"
acss="app-$(hash "$ROOT/frontend/styles/app.css").css"
cp "$ROOT/frontend/styles/global.css" "$OUT/$gcss"
cp "$ROOT/frontend/styles/app.css" "$OUT/$acss"
sed -e "s#{{GLOBAL_CSS}}#$gcss#" -e "s#{{APP_CSS}}#$acss#" -e "s#{{GLUE_JS}}#$glue#" -e "s#{{WASM}}#$wasm#" -e "s#{{MAIN_JS}}#$main#" \
  "$ROOT/frontend/index.html" > "$OUT/index.html"
size=$(wc -c < "$OUT/$wasm")
gz=$(gzip -9c "$OUT/$wasm" | wc -c)
echo "       $wasm : $size octets ($gz gzip)"

# Migration: the React web app for /app/* until its screens are ported (web/dist/app, served first by the server).
if [ "${ALTIM_REACT_APP:-1}" != 0 ]; then
  bun_run() {
    if command -v bun >/dev/null 2>&1; then
      (cd "$ROOT/web" && bun run "$@")
    elif command -v npx >/dev/null 2>&1; then
      (cd "$ROOT/web" && npx --yes "bun@$(cat "$ROOT/.bun-version")" run "$@")
    else
      return 1
    fi
  }
  echo "-----> Altim : app web React (transition, /app/*)"
  if [ ! -d "$ROOT/node_modules" ] && command -v bun >/dev/null 2>&1; then (cd "$ROOT" && bun install --frozen-lockfile); fi
  bun_run build-app || { echo "       Bun indisponible : /app servi par le front Yew"; rm -rf "$OUT/app"; }
fi
