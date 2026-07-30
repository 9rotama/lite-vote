mod actions;
mod components;
mod pages;

use anyhow::{Context, Result};
use lite_vote::{
    db::{DatabaseConfig, connect},
    realtime::RoomUpdateHub,
};
use topcoat::{
    asset::{AssetBundle, RouterBuilderAssetExt},
    cookie::RouterBuilderCookieExt,
    router::{Router, RouterBuilderDiscoverExt},
};

#[tokio::main]
async fn main() -> Result<()> {
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
