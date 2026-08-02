mod actions;
mod components;
mod pages;

use anyhow::{Context, Result};
use lite_vote::{
    db::{DatabaseConfig, connect},
    observability::{Environment, init_logging},
    realtime::RoomUpdateHub,
};
use topcoat::{
    asset::{AssetBundle, RouterBuilderAssetExt},
    cookie::RouterBuilderCookieExt,
    router::{Router, RouterBuilderDiscoverExt},
};

#[tokio::main]
async fn main() -> Result<()> {
    let environment = Environment::from_env()?;
    init_logging(environment)?;
    if let Err(error) = run().await {
        tracing::error!(error = %error, "application failed");
        return Err(error);
    }
    Ok(())
}

async fn run() -> Result<()> {
    let database = connect(DatabaseConfig::from_env()?)
        .await
        .context("database startup check failed")?;
    let router = Router::builder()
        .discover()
        .assets(AssetBundle::load().unwrap())
        .app_context(database.pool)
        .app_context(RoomUpdateHub::default())
        .cookies()
        .build();
    topcoat::start(router).await?;
    Ok(())
}

#[cfg(test)]
mod http_tests;
