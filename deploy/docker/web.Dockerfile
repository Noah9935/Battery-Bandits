# Weboberfläche: Leptos nach WebAssembly übersetzen (Trunk), danach mit Caddy ausliefern.
# Caddy liefert die statischen Dateien unter / und leitet /api/* an den Dashboard-Service weiter.

FROM rust:1-bookworm AS chef
RUN rustup target add wasm32-unknown-unknown \
 && cargo install cargo-chef --locked \
 && cargo install trunk --locked
WORKDIR /app

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --locked --target wasm32-unknown-unknown -p frontend --recipe-path recipe.json
COPY . .
WORKDIR /app/frontend
RUN trunk build --release

FROM caddy:2-alpine AS web
COPY deploy/Caddyfile /etc/caddy/Caddyfile
COPY --from=builder /app/frontend/dist /srv
EXPOSE 80
