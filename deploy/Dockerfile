# syntax=docker/dockerfile:1
# The Opencord server and voice node, from the repository root:
#
#   docker build -f deploy/Dockerfile -t opencord-server .
#
# The server keeps its config, database, TLS certificate and voice keys in
# /data. It listens on 7710/tcp, sends voice and video over 7711/udp, and
# takes music from Lavalink on 7712/udp. Tunnels and reverse proxies send
# plain HTTP to 7710 too (docs/domains.md); a proxy in another container
# comes from a private network, so the image takes plain HTTP from private
# ranges. Narrow OPENCORD_PLAIN_HTTP_FROM to the proxy's network, or set it
# empty when the container uses the host's network.

FROM rust:1.99-trixie AS build
WORKDIR /src
RUN apt-get update && apt-get install -y --no-install-recommends cmake && rm -rf /var/lib/apt/lists/*
COPY . .
# Builds only the server's crates, never the app's media ones.
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    SQLX_OFFLINE=true CARGO_PROFILE_RELEASE_DEBUG=0 \
    cargo build --locked --release -p opencord-server --bins \
 && install -Dm755 target/release/opencord-server /out/opencord-server \
 && install -Dm755 target/release/opencord-voice-node /out/opencord-voice-node

FROM debian:trixie-slim
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates \
 && rm -rf /var/lib/apt/lists/* \
 && useradd --system --home-dir /data --uid 7710 opencord \
 && install -d -o opencord -g opencord /data
COPY --from=build /out/ /usr/local/bin/
USER opencord
WORKDIR /data
ENV OPENCORD_CONFIG=/data/opencord.toml \
    OPENCORD_DATA_DIR=/data \
    OPENCORD_BIND=0.0.0.0:7710 \
    OPENCORD_VOICE_UDP_PORT=7711 \
    OPENCORD_MUSIC_UDP_PORT=7712 \
    OPENCORD_PLAIN_HTTP_FROM=10.0.0.0/8,172.16.0.0/12,192.168.0.0/16,fc00::/7
VOLUME ["/data"]
EXPOSE 7710/tcp 7711/udp 7712/udp 80/tcp
ENTRYPOINT ["opencord-server"]
