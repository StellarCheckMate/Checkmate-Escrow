FROM rust:1.75-slim AS builder
WORKDIR /app
COPY . .
RUN cargo build --release --manifest-path services/event-indexer/Cargo.toml

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=builder /app/target/release/event-indexer /usr/local/bin/event-indexer
EXPOSE 8080
CMD ["event-indexer"]
