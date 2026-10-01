#!/bin/sh
# Builds the web front (Rust + Yew, frontend/) into web/dist, where the server serves it ($ALTIM_WEB_ROOT/dist). It is
# compiled to WebAssembly once for the presentation site and once per group of app screens (altim_core::web::bundle),
# so that a first visit downloads only the screens of its page:
# - the presentation site (/, legal pages): web/dist/index.html, altim-site-<hash>.js and altim-site-<hash>_bg.wasm;
# - each group of the web app (/app/*): web/dist/app/<group>.html (index.html for the Radar),
#   altim-app-<group>-<hash>.js and altim-app-<group>-<hash>_bg.wasm;
# - the whole app, altim-app-<hash>.js and altim-app-<hash>_bg.wasm, which each group's page fetches in the background
#   and hands the page over to at the first move to another group (no reload; frontend/loader-app.js, altim_web::part;
#   before it is ready, the target group's own .wasm when prepared: on hover, or as a neighbour);
# plus the shared styles (global-<hash>.css, app-<hash>.css). Icons and other static files stay in web/public.
# The .wasm, .js and .css also get a brotli (.br) and a gzip (.gz) copy (scripts/precompress), sent as is by the server.
# The parts are the entry points of frontend/bundles (Cargo examples of altim-bundles): one compilation of the library,
# then one link per part, in parallel. Tools: cargo with the wasm32-unknown-unknown target, wasm-bindgen (same version
# as the crate, fetched by scripts/web-tools.sh into $ALTIM_TOOLS), wasm-opt (optional, smaller .wasm).
set -eu
ROOT=$(cd "$(dirname "$0")/.." && pwd)
OUT="$ROOT/web/dist"
TOOLS=${ALTIM_TOOLS:-$ROOT/target/web-tools}
TARGET_DIR=${CARGO_TARGET_DIR:-$ROOT/target}
PATH="$TOOLS:$PATH"
# The groups of app screens: APP_BUNDLES of core/src/web/bundle.rs, entry points frontend/bundles/src/<group>.rs.
GROUPS="radar actif avoirs simulation selection actu reglages bot"
# The groups a group's page prepares as soon as its screen is shown (their screens one tap away): "group:neighbours".
NEIGHBOURS="avoirs:simulation simulation:avoirs"

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
gate="gate-$(hash "$ROOT/frontend/gate.js").js"
cp "$ROOT/frontend/gate.js" "$OUT/$gate"

# The .wasm of one part ($1 = site | app-<group> | app, from the entry point frontend/bundles/src/<site | group | app>.rs)
# through wasm-bindgen and wasm-opt: its .wasm and glue in web/dist, their names in $tmp/<part>.names (glue, wasm).
finish() {
  dir="$tmp/$1"
  wasm-bindgen --target web --no-typescript --out-dir "$dir" --out-name altim "$TARGET_DIR/wasm32-unknown-unknown/wasm-release/examples/${1#app-}.wasm"
  if command -v wasm-opt >/dev/null 2>&1 && [ "${ALTIM_WASM_OPT:-1}" != 0 ]; then
    # -O2 without its inlining pass: inlining shrinks the raw file but the copies of the inlined code compress worse
    # (-Oz/-O2 with it: +12 % brotli), and the network carries the compressed file; without it -O2 is 0.8 % below -O1
    # in brotli (measured on this front, Binaryen 133).
    wasm-opt -O2 --skip-pass=inlining-optimizing --enable-bulk-memory --enable-nontrapping-float-to-int --enable-sign-ext --enable-mutable-globals \
      --enable-reference-types --enable-multivalue -o "$dir/altim_bg.opt.wasm" "$dir/altim_bg.wasm"
    mv "$dir/altim_bg.opt.wasm" "$dir/altim_bg.wasm"
  fi
  wasm="altim-$1-$(hash "$dir/altim_bg.wasm")_bg.wasm"
  glue="altim-$1-$(hash "$dir/altim.js").js"
  cp "$dir/altim_bg.wasm" "$OUT/$wasm"
  cp "$dir/altim.js" "$OUT/$glue"
  echo "$glue $wasm" > "$tmp/$1.names"
  echo "       $wasm : $(wc -c < "$OUT/$wasm") octets"
}

# The HTML page ($2) of a part ($1) and its loader: the site's starts its .wasm; an app group's also prepares the whole
# app (frontend/loader-app.js), and its page holds the app's frame with the tab $3 lit. The loader is a file (CSP:
# script-src 'self', no inline script).
page() {
  read -r glue wasm < "$tmp/$1.names"
  gate_tag=""
  shell=""
  if [ "$1" = site ]; then
    printf 'import init from "/%s";\ninit({ module_or_path: "/%s" });\n' "$glue" "$wasm" > "$tmp/$1.main.js"
  else
    group=${1#app-}
    near=""
    for n in $NEIGHBOURS; do
      [ "${n%%:*}" = "$group" ] && near="$near,\"${n#*:}\""
    done
    sed -e "s#{{GLUE_JS}}#$glue#" -e "s#{{WASM}}#$wasm#" -e "s#{{GROUP}}#$group#" -e "s#{{PARTS}}#$parts_json#" \
      -e "s#{{NEIGHBOURS}}#[${near#,}]#" "$ROOT/frontend/loader-app.js" > "$tmp/$1.main.js"
    # The app's frame (header and tabs, the group's tab lit, as the app draws it), shown before the .wasm draws the
    # screen; frontend/gate.js hides it when the disclaimer is still to be accepted.
    gate_tag="<script src=\"/$gate\"></script>"
    tab=$3
    shell=$(sed -e "s#{{$tab}}# aria-current=\"page\" class=\"on\"#g" -e "s#{{[a-z]*}}##g" "$ROOT/frontend/shell.html")
  fi
  main="main-$1-$(hash "$tmp/$1.main.js").js"
  cp "$tmp/$1.main.js" "$OUT/$main"
  sed -e "s#{{GLOBAL_CSS}}#$gcss#" -e "s#{{APP_CSS}}#$acss#" -e "s#{{GLUE_JS}}#$glue#" -e "s#{{WASM}}#$wasm#" -e "s#{{MAIN_JS}}#$main#" \
    -e "s#{{GATE}}#$gate_tag#" -e "s#{{SHELL}}#$shell#" "$ROOT/frontend/index.html" > "$2"
}

# One build: the library once, then the link of every entry point (the link-time optimisation of each .wasm), in
# parallel; then wasm-bindgen and wasm-opt of every part, in parallel too. `-C linker-plugin-lto`: the libraries keep
# their code lightly optimised (the "pre-link" pipeline) until the link optimises each .wasm as a whole, which inlines
# less: -1.5 % brotli against libraries optimised on their own.
PARTS="site app"
for g in $GROUPS; do PARTS="$PARTS app-$g"; done
echo "-----> Altim : front Rust + Yew → WebAssembly ($PARTS)"
command -v wasm-opt >/dev/null 2>&1 && [ "${ALTIM_WASM_OPT:-1}" != 0 ] || echo "       (wasm-opt absent : .wasm non optimisés par Binaryen)"
CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUSTFLAGS="-C linker-plugin-lto" cargo build --manifest-path "$ROOT/Cargo.toml" -p altim-bundles --profile wasm-release --target wasm32-unknown-unknown --locked --examples
pids=""
for p in $PARTS; do
  finish "$p" &
  pids="$pids $!"
done
for pid in $pids; do
  wait "$pid"
done
for p in $PARTS; do
  [ -f "$tmp/$p.names" ] || { echo "Partie $p non construite" >&2; exit 1; }
done

# Every app part's glue and .wasm, for the loaders: {"radar":["/glue.js","/x_bg.wasm"],…,"app":[…]}.
parts_json=""
for p in app $GROUPS; do
  [ "$p" = app ] && f=app || f="app-$p"
  read -r glue wasm < "$tmp/$f.names"
  parts_json="$parts_json,\"$p\":[\"/$glue\",\"/$wasm\"]"
done
parts_json="{${parts_json#,}}"

page site "$OUT/index.html"
for g in $GROUPS; do
  name=$g
  [ "$g" = radar ] && name=index
  # The tab lit for the group (app::active_tab): the asset screen sits under the Radar, the bot under the settings.
  case $g in
    actif) tab=radar ;;
    bot) tab=reglages ;;
    simulation) tab=avoirs ;;
    *) tab=$g ;;
  esac
  page "app-$g" "$OUT/app/$name.html" "$tab"
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
