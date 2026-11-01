# Baut einen der Rust-Dienste als schlankes Image.
#   docker build -f docker/service.Dockerfile --build-arg PACKAGE=ingest .
#
# SQLx prüft die Abfragen ohne Datenbankverbindung anhand von .sqlx/
# (erzeugt mit tools/prepare-sqlx.sh).
FROM rust:1-bookworm AS build
ARG PACKAGE
WORKDIR /src
ENV SQLX_OFFLINE=true
COPY . .
RUN --mount=type=cache,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,target=/src/target,id=bb-target-${PACKAGE} \
    cargo build --release --locked -p "${PACKAGE}" \
 && cp "target/release/${PACKAGE}" /service

FROM debian:bookworm-slim
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates \
 && rm -rf /var/lib/apt/lists/*
COPY --from=build /service /usr/local/bin/service
USER nobody
ENTRYPOINT ["/usr/local/bin/service"]
