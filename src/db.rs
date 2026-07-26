use crate::models::{Choice, Participant, Vote, VotingRoom};
use anyhow::{Context, Result, bail};
use rusqlite::Connection;
use std::{
    env,
    path::{Path, PathBuf},
    time::Duration,
};
use toasty::Db;
use toasty_driver_sqlite::Sqlite;

pub const DATABASE_PATH_ENV: &str = "LITE_VOTE_DATABASE_PATH";
pub const BUSY_TIMEOUT_MS_ENV: &str = "LITE_VOTE_DATABASE_BUSY_TIMEOUT_MS";
pub const DEFAULT_DATABASE_PATH: &str = "var/lite-vote.sqlite3";
pub const DEFAULT_BUSY_TIMEOUT_MS: u64 = 5_000;
pub const INITIAL_MIGRATION: &str = include_str!("../migrations/0001_initial.sql");

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
pub struct Database {
    pub orm: Db,
    pub config: DatabaseConfig,
}
pub fn models() -> toasty::schema::ModelSet {
    toasty::models!(VotingRoom, Choice, Participant, Vote)
}
pub async fn connect(config: DatabaseConfig) -> Result<Database> {
    check(&config)?;
    let mut builder = Db::builder();
    builder.models(models()).max_pool_size(1);
    let orm = builder
        .build(Sqlite::open(&config.path))
        .await
        .context("Toasty SQLite connection failed")?;
    Ok(Database { orm, config })
}
pub fn migrate(config: &DatabaseConfig) -> Result<()> {
    ensure_parent(&config.path)?;
    let mut conn = open_configured(config)?;
    let tx = conn.transaction()?;
    tx.execute_batch(INITIAL_MIGRATION)?;
    tx.commit()?;
    Ok(())
}
pub fn open_configured(config: &DatabaseConfig) -> Result<Connection> {
    let conn = Connection::open(&config.path)
        .with_context(|| format!("failed to open {}", config.path.display()))?;
    conn.busy_timeout(config.busy_timeout)?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    Ok(conn)
}
fn check(config: &DatabaseConfig) -> Result<()> {
    ensure_parent(&config.path)?;
    let conn = open_configured(config)?;
    let applied = conn
        .query_row("SELECT MAX(version) FROM schema_migrations", [], |r| {
            r.get::<_, Option<i64>>(0)
        })
        .context("database is not migrated; run `cargo run --bin migrate` first")?;
    if applied != Some(1) {
        bail!("database migration is incomplete (expected 1, found {applied:?})");
    }
    Ok(())
}
fn ensure_parent(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    Ok(())
}
