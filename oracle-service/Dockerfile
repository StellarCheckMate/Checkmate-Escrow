FROM rust:1.82-slim AS builder
WORKDIR /app
COPY . .
RUN cargo build --release --manifest-path oracle-service/Cargo.toml

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates curl && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=builder /app/target/release/oracle-service /usr/local/bin/oracle-service
EXPOSE 8000
CMD ["oracle-service"]
