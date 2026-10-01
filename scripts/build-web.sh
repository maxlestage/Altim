#!/bin/sh
# Builds the web front (Rust + Yew, frontend/) into web/dist, where the server serves it ($ALTIM_WEB_ROOT/dist). It is
# compiled to WebAssembly once for the presentation site and once per group of app screens (altim_core::web::bundle),
# so that a first visit downloads only the screens of its page:
# - the presentation site (/, legal pages): web/dist/index.html, altim-site-<hash>.js and altim-site-<hash>_bg.wasm;
# - each group of the web app (/app/*): web/dist/app/<group>.html (index.html for the Radar),
#   altim-app-<group>-<hash>.js and altim-app-<group>-<hash>_bg.wasm;
# plus the shared styles (global-<hash>.css, app-<hash>.css). Icons and other static files stay in web/public.
# The .wasm, .js and .css also get a brotli (.br) and a gzip (.gz) copy (scripts/precompress), sent as is by the server.
# Tools: cargo with the wasm32-unknown-unknown target, wasm-bindgen (same version as the crate, fetched by
# scripts/web-tools.sh into $ALTIM_TOOLS), wasm-opt (optional, smaller .wasm).
set -eu
ROOT=$(cd "$(dirname "$0")/.." && pwd)
OUT="$ROOT/web/dist"
TOOLS=${ALTIM_TOOLS:-$ROOT/target/web-tools}
TARGET_DIR=${CARGO_TARGET_DIR:-$ROOT/target}
PATH="$TOOLS:$PATH"
# The groups of app screens: APP_BUNDLES of core/src/web/bundle.rs, with the `app-<group>` features of frontend/Cargo.toml.
GROUPS="radar actif avoirs selection actu reglages bot"
# The groups every app page prefetches once its screen is shown (the most common moves): their .js and .wasm.
PREFETCH="radar actif"

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

# One part of the front ($1 = site | app-<group>, the cargo feature): its .wasm, glue and loader in web/dist, their
# names in $tmp/<part>.names (glue, wasm, loader).
bundle() {
  echo "-----> Altim : $1 (Rust + Yew → WebAssembly)"
  cargo build --manifest-path "$ROOT/Cargo.toml" -p altim-web --profile wasm-release --target wasm32-unknown-unknown --locked \
    --no-default-features --features "$1"
  dir="$tmp/$1"
  wasm-bindgen --target web --no-typescript --out-dir "$dir" --out-name altim "$TARGET_DIR/wasm32-unknown-unknown/wasm-release/altim_web.wasm"
  if command -v wasm-opt >/dev/null 2>&1 && [ "${ALTIM_WASM_OPT:-1}" != 0 ]; then
    # -O2 without its inlining pass: inlining shrinks the raw file but the copies of the inlined code compress worse
    # (-Oz/-O2 with it: +12 % brotli), and the network carries the compressed file; without it -O2 is 0.8 % below -O1
    # in brotli (measured on this front, Binaryen 133).
    wasm-opt -O2 --skip-pass=inlining-optimizing --enable-bulk-memory --enable-nontrapping-float-to-int --enable-sign-ext --enable-mutable-globals \
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
  echo "$glue $wasm $main" > "$tmp/$1.names"
  echo "       $wasm : $(wc -c < "$OUT/$wasm") octets ($(gzip -9c "$OUT/$wasm" | wc -c) gzip)"
}

# The HTML page ($2) of a part ($1), with the files other parts to prefetch ($3).
page() {
  read -r glue wasm main < "$tmp/$1.names"
  meta=""
  if [ -n "$3" ]; then
    meta="<meta name=\"altim-prefetch\" content=\"$3\" />"
  fi
  sed -e "s#{{GLOBAL_CSS}}#$gcss#" -e "s#{{APP_CSS}}#$acss#" -e "s#{{GLUE_JS}}#$glue#" -e "s#{{WASM}}#$wasm#" -e "s#{{MAIN_JS}}#$main#" \
    -e "s#{{PREFETCH}}#$meta#" "$ROOT/frontend/index.html" > "$2"
}

bundle site
for g in $GROUPS; do
  bundle "app-$g"
done

page site "$OUT/index.html" ""
for g in $GROUPS; do
  urls=""
  for p in $PREFETCH; do
    if [ "$p" != "$g" ]; then
      read -r glue wasm main < "$tmp/app-$p.names"
      urls="$urls /$glue /$wasm"
    fi
  done
  name=$g
  [ "$g" = radar ] && name=index
  page "app-$g" "$OUT/app/$name.html" "${urls# }"
done

# Brotli (strongest level) and gzip copies of the hashed files, sent as is by the server: ~20 % less over the network
# than its on-the-fly compression. Without them (ALTIM_PRECOMPRESS=0, or the tool cannot be built) the server still
# compresses on the fly.
if [ "${ALTIM_PRECOMPRESS:-1}" != 0 ]; then
  echo "-----> Altim : copies compressées (brotli, gzip)"
  if cargo build --release --locked --quiet --manifest-path "$ROOT/scripts/precompress/Cargo.toml" --target-dir "$TARGET_DIR/precompress"; then
    find "$OUT" -type f \( -name '*.wasm' -o -name '*.js' -o -name '*.css' \) -exec "$TARGET_DIR/precompress/release/altim-precompress" {} +
  else
    echo "       (outil de compression indisponible : le serveur compressera à la volée)"
  fi
fi
