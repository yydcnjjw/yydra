# syntax=docker/dockerfile:1
# SPDX-License-Identifier: MIT OR Apache-2.0

FROM rust:bookworm AS build
WORKDIR /app
COPY rust-toolchain.toml ./
RUN rustup toolchain install nightly --profile minimal --component rustfmt,clippy \
    && rustc +nightly --version && cargo +nightly --version
COPY . .
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/app/target \
    cargo build --locked --release --manifest-path crates/server/Cargo.toml \
      --bin server --bin migrate \
    && mkdir /out && cp target/release/server target/release/migrate /out/

FROM debian:bookworm-slim AS runtime
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/* \
    && groupadd --gid 10001 yydra \
    && useradd --uid 10001 --gid 10001 --no-create-home yydra \
    && mkdir /var/lib/yydra && chown 10001:10001 /var/lib/yydra \
    && chmod 0700 /var/lib/yydra
COPY --from=build /out/server /out/migrate /usr/local/bin/
COPY LICENSE-APACHE LICENSE-MIT /usr/share/doc/yydra/
COPY container/entrypoint.sh /usr/local/bin/yydra-entrypoint.sh
USER 10001:10001
ENV YYDRA_BIND_ADDRESS=0.0.0.0:4000
EXPOSE 4000
ENTRYPOINT ["/bin/sh", "/usr/local/bin/yydra-entrypoint.sh"]
CMD ["server"]
