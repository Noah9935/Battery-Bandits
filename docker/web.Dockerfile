# Baut die Web-App (Leptos, WebAssembly) und liefert sie mit Caddy aus.
# Caddy leitet /api an den API-Server weiter, alles andere sind statische Dateien.
FROM rust:1-bookworm AS build
RUN rustup target add wasm32-unknown-unknown \
 && curl -L --proto '=https' --tlsv1.2 -sSf \
    https://raw.githubusercontent.com/cargo-bins/cargo-binstall/main/install-from-binstall-release.sh | bash \
 && cargo binstall -y trunk
WORKDIR /src
COPY . .
RUN --mount=type=cache,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,target=/src/target,id=bb-target-web \
    cd web && trunk build --release --dist /out

FROM caddy:2.10-alpine
COPY docker/web.Caddyfile /etc/caddy/Caddyfile
COPY --from=build /out /srv/www
