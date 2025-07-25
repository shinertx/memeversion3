mod config;
mod database;
mod executor;
mod jito_client;
mod jupiter;
mod signer_client;
mod strategies;

use crate::config::CONFIG;
use anyhow::Result;
use database::Database;
use executor::MasterExecutor;
use std::sync::Arc;
use tracing::{info, level_filters::LevelFilter};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    let filter = EnvFilter::builder()
        .with_default_directive(LevelFilter::INFO.into())
        .from_env_lossy();
    tracing_subscriber::fmt().with_env_filter(filter).init();

    info!("🚀 Starting MemeSnipe v24 Executor - The Live Simulation Engine");

    let db = Arc::new(Database::new(&CONFIG.database_path)?);
    let mut master_executor = MasterExecutor::new(db.clone()).await?;

    master_executor.run().await?;
    Ok(())
}
