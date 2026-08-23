use std::{fs, str::FromStr};

use anyhow::{Context, Result};
use sqlx::{
    SqlitePool,
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions},
};

use crate::config::Config;

pub async fn connect(config: &Config) -> Result<SqlitePool> {
    fs::create_dir_all(&config.data_dir).context("failed to create Blank data directory")?;
    let database_path = config.data_dir.join("blank.db");
    let options = SqliteConnectOptions::from_str(&format!("sqlite://{}", database_path.display()))?
        .create_if_missing(true)
        .foreign_keys(true)
        .journal_mode(SqliteJournalMode::Wal);
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await?;
    sqlx::migrate!()
        .run(&pool)
        .await
        .context("failed to apply database migrations")?;
    Ok(pool)
}
