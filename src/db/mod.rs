//! Database connection and migration support.

use anyhow::{Context, Result, bail};
use sqlx::{
    SqlitePool,
    migrate::Migrator,
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions},
};
use std::{
    env,
    path::{Path, PathBuf},
    str::FromStr,
    time::Duration,
};

pub const DATABASE_PATH_ENV: &str = "LITE_VOTE_DATABASE_PATH";
pub const BUSY_TIMEOUT_MS_ENV: &str = "LITE_VOTE_DATABASE_BUSY_TIMEOUT_MS";
pub const DEFAULT_DATABASE_PATH: &str = "var/lite-vote.sqlite3";
pub const DEFAULT_BUSY_TIMEOUT_MS: u64 = 5_000;
pub static MIGRATOR: Migrator = sqlx::migrate!();

#[derive(Debug, Clone)]
pub struct DatabaseConfig {
    pub path: PathBuf,
    pub busy_timeout: Duration,
}

impl DatabaseConfig {
    pub fn from_env() -> Result<Self> {
        let path = env::var_os(DATABASE_PATH_ENV)
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(DEFAULT_DATABASE_PATH));
        let timeout = match env::var(BUSY_TIMEOUT_MS_ENV) {
            Ok(value) => value
                .parse::<u64>()
                .with_context(|| format!("{BUSY_TIMEOUT_MS_ENV} must be an integer"))?,
            Err(env::VarError::NotPresent) => DEFAULT_BUSY_TIMEOUT_MS,
            Err(error) => return Err(error.into()),
        };
        Ok(Self {
            path,
            busy_timeout: Duration::from_millis(timeout),
        })
    }
}

#[derive(Debug, Clone)]
pub struct Database {
    pub pool: SqlitePool,
    pub config: DatabaseConfig,
}

pub async fn connect(config: DatabaseConfig) -> Result<Database> {
    ensure_parent(&config.path)?;
    let pool = connect_pool(&config).await?;
    if let Err(error) = validate_migrations(&pool).await {
        pool.close().await;
        return Err(error);
    }
    Ok(Database { pool, config })
}

pub async fn connect_pool(config: &DatabaseConfig) -> Result<SqlitePool> {
    ensure_parent(&config.path)?;
    let options = connect_options(config)?;
    SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .with_context(|| format!("failed to connect to {}", config.path.display()))
}

pub fn connect_options(config: &DatabaseConfig) -> Result<SqliteConnectOptions> {
    let url = format!("sqlite://{}", config.path.display());
    Ok(SqliteConnectOptions::from_str(&url)?
        .create_if_missing(true)
        .foreign_keys(true)
        .journal_mode(SqliteJournalMode::Wal)
        .busy_timeout(config.busy_timeout))
}

pub async fn validate_migrations(pool: &SqlitePool) -> Result<()> {
    let table_exists: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master
         WHERE type = 'table' AND name = '_sqlx_migrations'",
    )
    .fetch_one(pool)
    .await
    .context("failed to inspect migration history")?;
    if table_exists == 0 {
        bail!("database is not migrated; run `sqlx migrate run` first");
    }

    let applied: Vec<(i64, Vec<u8>, bool)> =
        sqlx::query_as("SELECT version, checksum, success FROM _sqlx_migrations ORDER BY version")
            .fetch_all(pool)
            .await
            .context("failed to read migration history")?;
    let expected: Vec<_> = MIGRATOR.iter().collect();

    if applied.len() != expected.len() {
        bail!(
            "database migration set is incomplete or unexpected (expected {}, found {})",
            expected.len(),
            applied.len()
        );
    }
    for ((version, checksum, success), migration) in applied.iter().zip(expected) {
        if !success {
            bail!("database migration {version} did not complete successfully");
        }
        if *version != migration.version || checksum.as_slice() != migration.checksum.as_ref() {
            bail!("database migration {version} does not match the embedded migration");
        }
    }
    Ok(())
}

fn ensure_parent(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent().filter(|path| !path.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests;
