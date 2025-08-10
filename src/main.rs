use anyhow::{anyhow, Context, Result};
use dotenvy::dotenv;
use regex::Regex;
use std::env;
use teloxide::{prelude::*, utils::command::BotCommands};

mod db;
use db::Db;

#[derive(BotCommands, Clone)]
#[command(
    rename_rule = "lowercase",
    description = "Commands:\n\
    /start - register or show your UUID\n\
    /save {amount} [reason] - save money with optional reason\n\
    /adjust {+/-amount} [reason] - adjust balance with optional reason\n\
    /allinvoo - invest current stash and reset current to 0 (moves to history)\n\
    /query [n] - list your last n entries (default 10)\n\
    /help - this help"
)]
enum Command {
    Start,
    Save(String),
    Adjust(String),
    Allinvoo,
    Query(String),
    Help,
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenv().ok();
    pretty_env_logger::init();

    let bot_token = env::var("BOT_TOKEN").context("BOT_TOKEN env var is required")?;
    let database_url = env::var("DATABASE_URL").unwrap_or_else(|_| "sqlite:./data/bot.db".into());

    let bot = Bot::new(bot_token);
    let me = bot.get_me().send().await?;
    let bot_name = me.user.username.as_deref().unwrap_or("").to_string();

    let db = Db::new(&database_url).await?;

    teloxide::repl(bot, move |bot: Bot, msg: Message| {
        let db = db.clone();
        let bot_name = bot_name.clone();
        async move {
            if let Some(text) = msg.text() {
                if let Ok(cmd) = Command::parse(text, &bot_name) {
                    if let Err(err) = handle_command(bot.clone(), &db, &msg, cmd).await {
                        eprintln!("handle_command error: {err:?}");
                    }
                } else {
                    // Inline completion hints for /save and /adjust when typing
                    if text.starts_with("/save ") || text.starts_with("/adjust ") {
                        let hint = "Format: /save 12.34 [reason] or /adjust -5.50 [reason]";
                        if let Err(err) = bot
                            .send_message(msg.chat.id, hint)
                            .reply_to_message_id(msg.id)
                            .send()
                            .await
                        {
                            eprintln!("hint send error: {err:?}");
                        }
                    }
                }
            }
            respond(())
        }
    })
    .await;

    Ok(())
}

async fn handle_command(bot: Bot, db: &Db, msg: &Message, cmd: Command) -> Result<()> {
    let from = match msg.from() {
        Some(u) => u,
        None => {
            bot.send_message(msg.chat.id, "I can only respond to user messages.")
                .await?;
            return Ok(());
        }
    };

    // Ensure user exists (note: first_name is String)
    let uuid = db
        .ensure_user(
            from.id.0 as i64,
            from.username.clone(),
            from.first_name.clone(),
            from.last_name.clone(),
        )
        .await?;

    match cmd {
        Command::Start => {
            bot.send_message(
                msg.chat.id,
                format!(
                    "Welcome, {}!\nYour user UUID: `{}`\nUse /save, /adjust, /allinvoo, /query.",
                    display_name(from),
                    uuid
                ),
            )
            .parse_mode(teloxide::types::ParseMode::MarkdownV2)
            .await?;
        }
        Command::Help => {
            bot.send_message(msg.chat.id, Command::descriptions().to_string())
                .await?;
        }
        Command::Save(args) => {
            let (amount_cents, reason) = parse_amount_and_reason(&args, false)?;
            if amount_cents <= 0 {
                bot.send_message(msg.chat.id, "Amount must be positive for /save.")
                    .await?;
            } else {
                db.add_entry(uuid, amount_cents, "save", reason.clone())
                    .await?;
                let total = db.total_cents(uuid).await?;
                bot.send_message(
                    msg.chat.id,
                    format!(
                        "Saved {}.{}\n{}Total now: {}.{}",
                        cents_to_major(amount_cents),
                        cents_to_minor(amount_cents),
                        reason_prefix(&reason),
                        cents_to_major(total),
                        cents_to_minor(total),
                    ),
                )
                .await?;
            }
        }
        Command::Adjust(args) => {
            let (delta_cents, reason) = parse_amount_and_reason(&args, true)?;
            if delta_cents == 0 {
                bot.send_message(msg.chat.id, "Adjustment must be non-zero.")
                    .await?;
            } else {
                db.add_entry(uuid, delta_cents, "adjust", reason.clone())
                    .await?;
                let total = db.total_cents(uuid).await?;
                let sign = if delta_cents > 0 {
                    "added"
                } else {
                    "subtracted"
                };
                bot.send_message(
                    msg.chat.id,
                    format!(
                        "Adjustment {} {}.{}\n{}Total now: {}.{}",
                        sign,
                        cents_to_major(delta_cents.abs()),
                        cents_to_minor(delta_cents.abs()),
                        reason_prefix(&reason),
                        cents_to_major(total),
                        cents_to_minor(total),
                    ),
                )
                .await?;
            }
        }
        Command::Allinvoo => {
            let current = db.total_cents(uuid).await?;
            if current == 0 {
                bot.send_message(
                    msg.chat.id,
                    "Nothing to invest yet. Your current total is 0.",
                )
                .await?;
            } else {
                let moved = db.archive_user_entries(uuid).await?;
                let history = db.history_total_cents(uuid).await?;
                bot.send_message(
                    msg.chat.id,
                    format!(
                        "Invested {}.{} into VOO (moved to history).\nCurrent now: 0.00\nHistory total: {}.{}",
                        cents_to_major(moved),
                        cents_to_minor(moved),
                        cents_to_major(history),
                        cents_to_minor(history),
                    ),
                )
                .await?;
            }
        }
        Command::Query(args) => {
            let n = args.trim().parse::<i64>().unwrap_or(10).clamp(1, 50);
            let items = db.last_entries(uuid, n).await?;
            let current_total = db.total_cents(uuid).await?;
            let history_total = db.history_total_cents(uuid).await?;
            if items.is_empty() {
                bot.send_message(msg.chat.id, "No entries yet. Use /save to start!")
                    .await?;
            } else {
                let mut lines = Vec::new();
                lines.push(format!(
                    "Last {} entries for {}:",
                    items.len(),
                    display_name(from)
                ));
                for e in items {
                    let sign = if e.amount_cents >= 0 { "+" } else { "-" };
                    let amt = e.amount_cents.abs();
                    let reason = e.reason.unwrap_or_default();
                    lines.push(format!(
                        "{} {}.{} [{}] {}{}",
                        sign,
                        cents_to_major(amt),
                        cents_to_minor(amt),
                        e.kind,
                        e.created_at,
                        if reason.is_empty() {
                            "".to_string()
                        } else {
                            format!(" — {}", reason)
                        }
                    ));
                }
                lines.push(format!(
                    "\nCurrent total: {}.{}\nHistory total: {}.{}\nGrand total: {}.{}",
                    cents_to_major(current_total),
                    cents_to_minor(current_total),
                    cents_to_major(history_total),
                    cents_to_minor(history_total),
                    cents_to_major(current_total + history_total),
                    cents_to_minor(current_total + history_total),
                ));
                bot.send_message(msg.chat.id, lines.join("\n")).await?;
            }
        }
    }
    Ok(())
}

fn display_name(u: &teloxide::types::User) -> String {
    if let Some(username) = &u.username {
        format!("@{}", username)
    } else {
        u.first_name.clone() // first_name is a String
    }
}

/// Parses "amount [reason...]" where:
/// - for /save: amount must be positive "12" or "12.34"
/// - for /adjust: amount may be signed: "+5", "-3.50"
fn parse_amount_and_reason(input: &str, allow_signed: bool) -> Result<(i64, Option<String>)> {
    let s = input.trim();
    if s.is_empty() {
        return Err(anyhow!("Missing amount"));
    }

    let re = if allow_signed {
        Regex::new(r#"^\s*([+-]?\d+(?:[.,]\d{1,2})?)\s*(.*)$"#).unwrap()
    } else {
        Regex::new(r#"^\s*(\d+(?:[.,]\d{1,2})?)\s*(.*)$"#).unwrap()
    };

    let caps = re.captures(s).ok_or_else(|| anyhow!("Bad amount format"))?;
    let amount_str = caps.get(1).unwrap().as_str().replace(',', ".");
    let reason = caps
        .get(2)
        .map(|m| m.as_str().trim().to_string())
        .filter(|t| !t.is_empty());

    let cents = decimal_to_cents(&amount_str)?;
    Ok((cents, reason))
}

fn decimal_to_cents(s: &str) -> Result<i64> {
    // Accept "12", "12.3", "12.34", "+5", "-3.5"
    let neg = s.starts_with('-');
    let s = s.trim_start_matches(['+', '-']);
    let parts: Vec<&str> = s.split('.').collect();
    let cents = match parts.as_slice() {
        [whole] => whole.parse::<i64>()? * 100,
        [whole, frac] => {
            let mut f = frac.to_string();
            if f.len() == 1 {
                f.push('0');
            }
            if f.len() > 2 {
                return Err(anyhow!("Too many decimal places"));
            }
            whole.parse::<i64>()? * 100 + f.parse::<i64>()?
        }
        _ => return Err(anyhow!("Invalid number")),
    };
    Ok(if neg { -cents } else { cents })
}

fn cents_to_major(cents: i64) -> i64 {
    cents / 100
}
fn cents_to_minor(cents: i64) -> String {
    format!("{:02}", (cents.abs() % 100))
}
fn reason_prefix(reason: &Option<String>) -> String {
    reason
        .as_ref()
        .map(|r| format!("Reason: {}\n", r))
        .unwrap_or_default()
}
