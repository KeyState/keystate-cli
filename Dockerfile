# syntax=docker/dockerfile:1

# keystate-cli release image (GHCR). Multi-stage: build the release binary in
# the rust image (which has git, needed to fetch the core/adapter git
# dependencies pinned in Cargo.lock), then copy it into a slim Debian runtime.

FROM rust:1.85 AS build
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo build --release --locked \
    && cp target/release/keystate /keystate

FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*
RUN useradd --create-home --uid 10001 keystate
COPY --from=build /keystate /usr/local/bin/keystate
USER keystate
WORKDIR /home/keystate
ENTRYPOINT ["/usr/local/bin/keystate"]