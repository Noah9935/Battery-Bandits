# Werkzeugkasten zum Bauen, Prüfen und Testen. Auf dem Rechner muss nichts installiert sein.
FROM rust:1-bookworm

RUN rustup target add wasm32-unknown-unknown \
 && rustup component add clippy rustfmt llvm-tools-preview

RUN curl -L --proto '=https' --tlsv1.2 -sSf \
    https://raw.githubusercontent.com/cargo-bins/cargo-binstall/main/install-from-binstall-release.sh | bash

RUN cargo binstall -y trunk cargo-llvm-cov

RUN apt-get update \
 && apt-get install -y --no-install-recommends postgresql-client mosquitto-clients jq \
 && rm -rf /var/lib/apt/lists/*

WORKDIR /work
