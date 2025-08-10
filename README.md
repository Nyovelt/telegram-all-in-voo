# telegram-all-in-voo

A tiny Telegram bot that helps you “not spend it, save it” — every time you resist a purchase, `/save 12.50 coffee` adds to your personal stash. Think of it as your notional VOO pile. Works in groups (per‑person tracking) and persists to SQLite.

## Features

- **Commands**

  - `/start` — register or show your UUID
  - `/save {amount} [reason]` — e.g. `/save 12.34 latte`
  - `/adjust {+/-amount} [reason]` — e.g. `/adjust -5 fees` or `/adjust +10 bonus`
  - `/allinvoo` — shows your total (aka your VOO pile)
  - `/query [n]` — list your last `n` entries (default 10)

- **Group‑friendly**: tracks per user based on Telegram ID, stored with your own **UUID**.
- **Persistence**: SQLite database in a Docker volume.
- **Rust async**: `teloxide` + `sqlx` + `tokio`.

## Quick start (Docker)

1. Copy `.env.example` → `.env` and set your bot token:

```

BOT_TOKEN=123456789\:YOUR_TOKEN

```

2. Build & run:

```bash
docker compose up --build -d
```

3. Add the bot to Telegram, send `/start`.

The database lives at `sqlite:/app/data/bot.db` inside the container and is persisted via the `bot_data` volume.

## Running locally (without Docker)

```bash
rustup default stable
cp .env.example .env   # set BOT_TOKEN
cargo run
```

## Data model

- `users(id UUID, tg_user_id UNIQUE, tg_username, first_name, last_name, created_at)`
- `entries(id AUTOINC, user_id UUID, amount_cents INTEGER, kind TEXT ['save'|'adjust'], reason, created_at)`

Amounts are stored as **cents** (integers). `/save` requires a positive amount. `/adjust` accepts `+` or `-` deltas.

## Examples

- `/save 8.99 sandwich`
- `/adjust -3 tip`
- `/adjust +10 cashback`
- `/allinvoo`
- `/query 20`

In groups, each person’s totals are private to them (the bot keys off the sender). Everyone can show their own record in the shared chat if they choose.

## Env vars

- `BOT_TOKEN` **(required)** — Telegram bot token.
- `DATABASE_URL` _(optional)_ — default: `sqlite:/app/data/bot.db`.
- `RUST_LOG` _(optional)_ — e.g., `info` or `debug`.

## Avatar

An SVG is provided at `assets/avatar.svg`. It’s a minimalist piggy‑bank with “VOO” — feel free to swap it.

## Notes

- This bot does **not** perform real investing. It just tracks what you saved so you can invest manually (e.g., in VOO) later.
- Back up your DB volume if you care about history.
- PRs welcome!
