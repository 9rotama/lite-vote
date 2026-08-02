# syntax=docker/dockerfile:1

FROM rust:1.95.0-bookworm AS builder
WORKDIR /app

COPY Cargo.toml Cargo.lock rust-toolchain.toml build.rs components.toml styles.css ./
COPY migrations ./migrations
COPY src ./src

RUN cargo install topcoat-cli --version 0.4.0 --locked
RUN topcoat asset bundle --release
RUN install -d -o 65532 -g 65532 /data

FROM rust:1.95.0-bookworm AS migrator
RUN cargo install sqlx-cli \
    --version 0.9.0 \
    --no-default-features \
    --features sqlite \
    --locked
RUN install -d -o 65532 -g 65532 /data
WORKDIR /app
COPY migrations ./migrations
COPY --chmod=755 container/migrate.sh ./migrate.sh
USER 65532:65532
ENTRYPOINT ["/app/migrate.sh"]

FROM gcr.io/distroless/cc-debian12:nonroot AS runtime
WORKDIR /app
COPY --from=builder /app/target/release/lite-vote /usr/local/bin/lite-vote
COPY --from=builder /app/target/assets /usr/local/bin/assets
COPY --from=builder --chown=65532:65532 /data /data

ENV HOST=0.0.0.0 \
    PORT=3000 \
    LITE_VOTE_ENV=production \
    LITE_VOTE_DATABASE_PATH=/data/lite-vote.sqlite3

EXPOSE 3000
VOLUME ["/data"]
ENTRYPOINT ["/usr/local/bin/lite-vote"]
