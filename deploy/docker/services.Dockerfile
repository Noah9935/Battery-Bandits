# Images der vier Dienste und des Test-Publishers. Ein gemeinsamer Bauabschnitt (cargo-chef), danach
# je Dienst ein schlankes Laufzeit-Image:
#
#   docker build -f deploy/docker/services.Dockerfile --target ingestion-service .
#
# Ablauf: planner erzeugt ein Rezept der Abhängigkeiten, builder übersetzt sie in einer eigenen Schicht
# (bleibt im Cache, solange sich Cargo.toml und Cargo.lock nicht ändern) und danach erst den eigenen Code.

FROM rust:1-bookworm AS chef
RUN cargo install cargo-chef --locked
WORKDIR /app

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json
ARG PACKAGES="-p ingestion-service -p monitoring-service -p operations-service -p dashboard-service -p test-publisher"
RUN cargo chef cook --release --locked ${PACKAGES} --recipe-path recipe.json
COPY . .
RUN cargo build --release --locked ${PACKAGES}

# Gemeinsame Basis: Zertifikate (TLS zum Broker) und ein Benutzer ohne Rechte.
FROM debian:bookworm-slim AS runtime
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates \
 && rm -rf /var/lib/apt/lists/* \
 && useradd --system --no-create-home --uid 10001 app
USER app
ENV RUST_LOG=info

FROM runtime AS ingestion-service
COPY --from=builder /app/target/release/ingestion-service /usr/local/bin/service
ENTRYPOINT ["/usr/local/bin/service"]

FROM runtime AS monitoring-service
COPY --from=builder /app/target/release/monitoring-service /usr/local/bin/service
ENTRYPOINT ["/usr/local/bin/service"]

FROM runtime AS operations-service
COPY --from=builder /app/target/release/operations-service /usr/local/bin/service
EXPOSE 8082
ENTRYPOINT ["/usr/local/bin/service"]

FROM runtime AS dashboard-service
COPY --from=builder /app/target/release/dashboard-service /usr/local/bin/service
EXPOSE 8080
ENTRYPOINT ["/usr/local/bin/service"]

FROM runtime AS test-publisher
COPY --from=builder /app/target/release/test-publisher /usr/local/bin/service
ENTRYPOINT ["/usr/local/bin/service"]
