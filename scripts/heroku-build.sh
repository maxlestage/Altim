#!/bin/sh
# Heroku build of Altim (package.json "heroku-postbuild", run by the Node.js buildpack; package.json has no JavaScript
# dependency, it is only Heroku's entry point):
# - Rust buildpack then Node.js buildpack (recommended, see DEPLOIEMENT.md): the Rust buildpack has compiled the
#   server (backend/target/release/altim) and exported its toolchain; here the web front is built with it (Rust + Yew
#   → WebAssembly), its build cached next to the buildpack's.
# - Heroku's automatic detection (Node.js buildpack only): Rust is installed in /tmp (outside the slug) to build the
#   web front and compile the server. Slower (no cache) but the deployment works.
set -eu

# `cargo --version` rather than `command -v cargo`: a rustup proxy with no default toolchain is on PATH but unusable.
if ! cargo --version >/dev/null 2>&1; then
  echo "-----> Altim : installation de Rust (hors du slug)"
  version=$(sed -n 's/^VERSION=//p' RustConfig)
  export RUSTUP_HOME=/tmp/altim-rust/rustup CARGO_HOME=/tmp/altim-rust/cargo
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --quiet --profile minimal --default-toolchain "$version" --no-modify-path
  PATH="$CARGO_HOME/bin:$PATH"
fi
# Build outputs and tools outside the slug: next to the Rust buildpack's cache when there is one (kept between deploys).
cache=$(dirname "${CARGO_HOME:-/tmp/altim-rust/cargo}")
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$cache/altim-target}" ALTIM_TOOLS="${ALTIM_TOOLS:-$cache/altim-web-tools}"

if [ ! -f web/dist/index.html ]; then
  echo "-----> Altim : construction du site (web/dist)"
  sh scripts/build-web.sh
fi

if [ -x backend/target/release/altim ]; then
  exit 0
fi
echo "-----> Altim : aucun buildpack Rust, compilation du serveur"
cargo build --release --locked --bin altim --manifest-path backend/Cargo.toml
mkdir -p backend/target/release
cp "$CARGO_TARGET_DIR/release/altim" backend/target/release/altim
