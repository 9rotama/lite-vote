use anyhow::Result;
use lite_vote::db::{DatabaseConfig, migrate};
fn main() -> Result<()> {
    let config = DatabaseConfig::from_env()?;
    migrate(&config)?;
    println!("database is migrated: {}", config.path.display());
    Ok(())
}
