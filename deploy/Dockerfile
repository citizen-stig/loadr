# Multi-stage build: static-ish binary in a distroless image.
# Build from the repository root:
#   docker build -f deploy/Dockerfile -t loadr/loadr:latest .

FROM rust:1.92-bookworm AS builder
WORKDIR /src

# Pre-fetch dependencies for better layer caching.
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY plugins ./plugins
COPY testsupport ./testsupport
RUN cargo fetch --locked

RUN cargo build --release --locked -p loadr-cli \
    && cp target/release/loadr /loadr \
    && /loadr version

# Distroless final image: no shell, no package manager, tiny attack surface.
FROM gcr.io/distroless/cc-debian12:nonroot
LABEL org.opencontainers.image.title="loadr" \
      org.opencontainers.image.description="Modern load testing platform: k6 + JMeter in one Rust binary" \
      org.opencontainers.image.source="https://loadr.io" \
      org.opencontainers.image.licenses="LicenseRef-Proprietary"

COPY --from=builder /loadr /usr/local/bin/loadr

# Web UI (controller) and Prometheus scrape ports.
EXPOSE 6464 7625 9091
USER nonroot
ENTRYPOINT ["/usr/local/bin/loadr"]
CMD ["--help"]
