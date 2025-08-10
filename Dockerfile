# ---- Build stage
FROM rust:latest
WORKDIR /app

# Install dependencies
RUN apt-get update && apt-get install -y --no-install-recommends libsqlite3-dev pkg-config ca-certificates && \
    rm -rf /var/lib/apt/lists/*

# Build 
COPY . /app
RUN mkdir -p /app/data
RUN cargo build --release

# Prepare ENV
ENV DATABASE_URL=sqlite:/app/data/bot.db
ENV BOT_TOKEN=""
ENV RUST_LOG=info

CMD ["/app/target/release/telegram-all-in-voo"]
