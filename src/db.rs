use anyhow::Result;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{Row, SqlitePool};
use std::{fs, path::Path, str::FromStr};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct Db(pub SqlitePool);

/// A single ledger entry (moved to module scope so Rust is happy)
#[derive(Debug, Clone)]
pub struct Entry {
    pub amount_cents: i64,
    pub kind: String,
    pub reason: Option<String>,
    pub created_at: String,
}

impl Db {
    pub async fn new(database_url: &str) -> Result<Self> {
        // If it's a SQLite file path, ensure its parent directory exists
        if let Some(path) = sqlite_path_from_url(database_url) {
            if path != ":memory:" {
                if let Some(parent) = Path::new(&path).parent() {
                    if !parent.as_os_str().is_empty() {
                        fs::create_dir_all(parent)?;
                    }
                }
            }
        }

        // Create DB file if missing
        let opts = SqliteConnectOptions::from_str(database_url)?.create_if_missing(true);
        let pool = SqlitePoolOptions::new().connect_with(opts).await?;
        let db = Self(pool);
        db.init().await?;
        Ok(db)
    }

    async fn init(&self) -> Result<()> {
        let schema = r#"
        PRAGMA journal_mode=WAL;

        CREATE TABLE IF NOT EXISTS users(
          id TEXT PRIMARY KEY,
          tg_user_id INTEGER NOT NULL UNIQUE,
          tg_username TEXT,
          first_name TEXT,
          last_name TEXT,
          created_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS entries(
          id INTEGER PRIMARY KEY AUTOINCREMENT,
          user_id TEXT NOT NULL,
          amount_cents INTEGER NOT NULL,
          kind TEXT NOT NULL,
          reason TEXT,
          created_at TEXT NOT NULL,
          FOREIGN KEY(user_id) REFERENCES users(id)
        );

        CREATE INDEX IF NOT EXISTS idx_entries_user ON entries(user_id);
        "#;

        sqlx::query(schema).execute(&self.0).await?;
        Ok(())
    }

    pub async fn ensure_user(
        &self,
        tg_user_id: i64,
        tg_username: Option<String>,
        first_name: String, // <- String (not Option)
        last_name: Option<String>,
    ) -> Result<Uuid> {
        if let Some(row) = sqlx::query("SELECT id FROM users WHERE tg_user_id = ?")
            .bind(tg_user_id)
            .fetch_optional(&self.0)
            .await?
        {
            let id: String = row.get("id");
            return Ok(Uuid::parse_str(&id)?);
        }

        let id = Uuid::new_v4();
        let now = OffsetDateTime::now_utc()
            .format(&Rfc3339)
            .unwrap_or_else(|_| "now".into());

        sqlx::query(
            "INSERT INTO users(id, tg_user_id, tg_username, first_name, last_name, created_at)
             VALUES(?, ?, ?, ?, ?, ?)",
        )
        .bind(id.to_string())
        .bind(tg_user_id)
        .bind(tg_username)
        .bind(first_name)
        .bind(last_name)
        .bind(now)
        .execute(&self.0)
        .await?;

        Ok(id)
    }

    pub async fn add_entry(
        &self,
        user_id: Uuid,
        amount_cents: i64,
        kind: &str,
        reason: Option<String>,
    ) -> Result<()> {
        let now = OffsetDateTime::now_utc()
            .format(&Rfc3339)
            .unwrap_or_else(|_| "now".into());

        sqlx::query(
            "INSERT INTO entries(user_id, amount_cents, kind, reason, created_at)
             VALUES(?, ?, ?, ?, ?)",
        )
        .bind(user_id.to_string())
        .bind(amount_cents)
        .bind(kind)
        .bind(reason)
        .bind(now)
        .execute(&self.0)
        .await?;
        Ok(())
    }

    pub async fn total_cents(&self, user_id: Uuid) -> Result<i64> {
        let row = sqlx::query(
            "SELECT COALESCE(SUM(amount_cents),0) AS total FROM entries WHERE user_id = ?",
        )
        .bind(user_id.to_string())
        .fetch_one(&self.0)
        .await?;
        let total: i64 = row.get("total");
        Ok(total)
    }

    pub async fn last_entries(&self, user_id: Uuid, limit: i64) -> Result<Vec<Entry>> {
        let rows = sqlx::query(
            "SELECT amount_cents, kind, reason, created_at
             FROM entries
             WHERE user_id = ?
             ORDER BY id DESC
             LIMIT ?",
        )
        .bind(user_id.to_string())
        .bind(limit)
        .fetch_all(&self.0)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| Entry {
                amount_cents: r.get::<i64, _>("amount_cents"),
                kind: r.get::<String, _>("kind"),
                reason: r.get::<Option<String>, _>("reason"),
                created_at: r.get::<String, _>("created_at"),
            })
            .collect())
    }
}

fn sqlite_path_from_url(url: &str) -> Option<String> {
    if !url.starts_with("sqlite:") {
        return None;
    }
    // Accept both sqlite:PATH and sqlite://PATH
    let path = if let Some(rest) = url.strip_prefix("sqlite://") {
        rest
    } else if let Some(rest) = url.strip_prefix("sqlite:") {
        rest
    } else {
        url
    };
    Some(path.to_string())
}
