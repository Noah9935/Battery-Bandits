# Entwicklungsumgebung: alles, was zum Bauen, Prüfen und Testen nötig ist.
# Es muss nichts auf dem eigenen Rechner installiert werden (NF06).
FROM rust:1-bookworm

RUN rustup target add wasm32-unknown-unknown \
 && rustup component add clippy rustfmt llvm-tools-preview

RUN curl -L --proto '=https' --tlsv1.2 -sSf \
    https://raw.githubusercontent.com/cargo-bins/cargo-binstall/main/install-from-binstall-release.sh | bash

RUN cargo binstall -y trunk cargo-llvm-cov \
 && cargo binstall -y sqlx-cli --version '^0.8' --force \
 || cargo install sqlx-cli --version '^0.8' --no-default-features --features rustls,postgres --locked

RUN apt-get update \
 && apt-get install -y --no-install-recommends postgresql-client mosquitto-clients jq \
 && rm -rf /var/lib/apt/lists/*

WORKDIR /work
