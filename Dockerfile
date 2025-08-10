# ---- Build stage
FROM rust:latest as builder
WORKDIR /app
# Cache dependencies
COPY Cargo.toml Cargo.lock* ./
RUN mkdir src && echo "fn main() {}" > src/main.rs
RUN cargo build --release || true
# Build actual source
COPY src ./src
RUN cargo build --release

# ---- Runtime stage
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates tzdata && rm -rf /var/lib/apt/lists/*
WORKDIR /app
ENV RUST_LOG=info
# Default DB path (override with DATABASE_URL)
ENV DATABASE_URL="sqlite:/app/data/bot.db"
ENV BOT_TOKEN=""
VOLUME ["/app/data"]
RUN mkdir -p /app/data
COPY --from=builder /app/target/release/telegram-all-in-voo /usr/local/bin/telegram-all-in-voo
ENTRYPOINT ["/usr/local/bin/telegram-all-in-voo"]
