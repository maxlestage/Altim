#!/bin/sh
# Heroku build of Altim, whatever buildpacks the app has (package.json "heroku-postbuild"):
# - Bun buildpack then Rust buildpack (recommended, see DEPLOIEMENT.md): Bun has just built web/dist and the Rust
#   buildpack compiles backend/ next, with its cache: nothing to do here.
# - Heroku's automatic detection (Node.js buildpack only, e.g. GitHub integration without buildpacks set): Bun is
#   fetched through npm to build web/dist, then Rust is installed in /tmp (outside the slug) to compile the server.
#   Slower (no cache: ~5 min) but the deployment works.
set -eu

bun_run() {
  if command -v bun >/dev/null 2>&1; then
    bun run "$@"
  else
    npx --yes "bun@$(cat .bun-version)" run "$@"
  fi
}

if [ ! -f web/dist/index.html ]; then
  echo "-----> Altim : construction du site (web/dist)"
  (cd web && bun_run build)
fi

# Run by the Bun buildpack: the Rust buildpack comes next and compiles the server with its cache.
case "${npm_config_user_agent:-}${npm_execpath:-}" in
  *bun*)
    echo "-----> Altim : le serveur sera compilé par le buildpack Rust"
    exit 0
    ;;
esac
if [ -x backend/target/release/altim ]; then
  exit 0
fi

echo "-----> Altim : aucun buildpack Rust, installation de Rust et compilation du serveur (≈ 5 min)"
version=$(sed -n 's/^VERSION=//p' RustConfig)
export RUSTUP_HOME=/tmp/altim-rust/rustup CARGO_HOME=/tmp/altim-rust/cargo CARGO_TARGET_DIR=/tmp/altim-rust/target
curl -sSf https://sh.rustup.rs | sh -s -- -y --quiet --profile minimal --default-toolchain "$version" --no-modify-path
"$CARGO_HOME/bin/cargo" build --release --locked --bin altim --manifest-path backend/Cargo.toml
mkdir -p backend/target/release
cp "$CARGO_TARGET_DIR/release/altim" backend/target/release/altim
