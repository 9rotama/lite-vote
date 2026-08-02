//! Runtime logging configuration.

use anyhow::{Result, anyhow, bail};
use std::env;
use tracing_subscriber::EnvFilter;

pub const ENVIRONMENT_ENV: &str = "LITE_VOTE_ENV";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Environment {
    Local,
    Production,
}

impl Environment {
    pub fn from_env() -> Result<Self> {
        Self::parse(env::var(ENVIRONMENT_ENV).as_deref().unwrap_or("local"))
    }

    fn parse(value: &str) -> Result<Self> {
        match value {
            "local" => Ok(Self::Local),
            "production" => Ok(Self::Production),
            _ => bail!("{ENVIRONMENT_ENV} must be `local` or `production`"),
        }
    }
}

pub fn init_logging(environment: Environment) -> Result<()> {
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("lite_vote=info"));
    let subscriber = tracing_subscriber::fmt().with_env_filter(filter);

    match environment {
        Environment::Local => subscriber
            .try_init()
            .map_err(|error| anyhow!("failed to initialize text logging: {error}")),
        Environment::Production => subscriber
            .json()
            .try_init()
            .map_err(|error| anyhow!("failed to initialize JSON logging: {error}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn environment_accepts_only_documented_values() {
        assert_eq!(Environment::parse("local").unwrap(), Environment::Local);
        assert_eq!(
            Environment::parse("production").unwrap(),
            Environment::Production
        );
        assert!(Environment::parse("staging").is_err());
    }
}
