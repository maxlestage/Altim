# Altim image for Heroku (container stack, see heroku.yml): the web front is built with Rust + Yew (WebAssembly, plus
# the React web app during the migration, with Bun), the server with Rust; only the server binary + the built web files
# go into the final image.

# 1. Web front → web/dist (scripts/build-web.sh: wasm32 target, wasm-bindgen and wasm-opt as prebuilt binaries)
FROM rust:1.98.1-slim-trixie AS web
RUN apt-get update && apt-get install -y --no-install-recommends curl ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=oven/bun:1.4.2 /usr/local/bin/bun /usr/local/bin/bun
WORKDIR /app
COPY scripts/web-tools.sh scripts/
COPY Cargo.lock ./
RUN ALTIM_TOOLS=/opt/web-tools sh scripts/web-tools.sh
COPY package.json bun.lock .bun-version ./
COPY web/package.json web/
RUN bun install --frozen-lockfile
COPY Cargo.toml rustfmt.toml ./
COPY core core
COPY frontend frontend
COPY backend/Cargo.toml backend/
RUN mkdir -p backend/src && echo 'fn main() {}' > backend/src/main.rs && touch backend/src/lib.rs
COPY web web
COPY scripts scripts
RUN ALTIM_TOOLS=/opt/web-tools sh scripts/build-web.sh

# 2. Server (Rust + Axum), dependencies compiled in their own layer
FROM rust:1.98.1-slim-trixie AS server
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY backend/Cargo.toml backend/
COPY core/Cargo.toml core/
COPY frontend/Cargo.toml frontend/
RUN mkdir -p backend/src core/src frontend/src && echo 'fn main() {}' > backend/src/main.rs && touch backend/src/lib.rs core/src/lib.rs frontend/src/lib.rs \
  && cargo build --release --locked -p altim && rm -rf backend/src core/src
COPY core/src core/src
COPY backend/src backend/src
RUN touch core/src/lib.rs backend/src/main.rs backend/src/lib.rs && cargo build --release --locked -p altim

# 3. Runtime: the binary, the web files and the root certificates for the market APIs
FROM debian:trixie-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/* \
  && useradd --system --uid 10001 --no-create-home altim
WORKDIR /app
COPY --from=server /app/target/release/altim /app/altim
COPY --from=web /app/web/dist /app/web/dist
COPY web/public /app/web/public
ENV ALTIM_WEB_ROOT=/app/web
USER altim
CMD ["/app/altim"]
