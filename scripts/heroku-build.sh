#!/bin/sh
# Heroku build of Altim, whatever buildpacks the app has (package.json "heroku-postbuild"):
# - Rust buildpack then Bun buildpack (recommended, see DEPLOIEMENT.md): the Rust buildpack has compiled the server
#   (backend/target/release/altim) and exported its toolchain; here the web front is built with it (Rust + Yew →
#   WebAssembly, plus the React web app during the migration), its build cached next to the buildpack's.
# - Heroku's automatic detection (Node.js buildpack only): Rust is installed in /tmp (outside the slug) to build the
#   web front and compile the server. Slower (no cache: ~6 min) but the deployment works.
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
# Run by the Bun buildpack placed before the Rust one (former order): the Rust buildpack compiles the server next.
case "${npm_config_user_agent:-}${npm_execpath:-}" in
  *bun*)
    echo "-----> Altim : le serveur sera compilé par le buildpack Rust"
    exit 0
    ;;
esac

echo "-----> Altim : aucun buildpack Rust, compilation du serveur"
cargo build --release --locked --bin altim --manifest-path backend/Cargo.toml
mkdir -p backend/target/release
cp "$CARGO_TARGET_DIR/release/altim" backend/target/release/altim
