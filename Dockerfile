# Altim image for Heroku (container stack, see heroku.yml): the web app is built with Bun, the server with Rust,
# and only the server binary + the built web files go into the final image.

# 1. Web app (React + TypeScript) → web/dist
FROM oven/bun:1.4.2 AS web
WORKDIR /app
COPY package.json bun.lock .bun-version ./
COPY web/package.json web/
RUN bun install --frozen-lockfile
COPY web web
RUN cd web && bun run build

# 2. Server (Rust + Axum), dependencies compiled in their own layer
FROM rust:1.98.1-slim-trixie AS server
WORKDIR /app/backend
COPY backend/Cargo.toml backend/Cargo.lock ./
RUN mkdir src && echo 'fn main() {}' > src/main.rs && touch src/lib.rs && cargo build --release --locked && rm -rf src
COPY backend/src src
RUN touch src/main.rs src/lib.rs && cargo build --release --locked

# 3. Runtime: the binary, the web files and the root certificates for the market APIs
FROM debian:trixie-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/* \
  && useradd --system --uid 10001 --no-create-home altim
WORKDIR /app
COPY --from=server /app/backend/target/release/altim /app/altim
COPY --from=web /app/web/dist /app/web/dist
COPY web/public /app/web/public
ENV ALTIM_WEB_ROOT=/app/web
USER altim
CMD ["/app/altim"]
