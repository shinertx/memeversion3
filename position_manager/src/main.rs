mod config;
mod database;
mod jupiter;
mod position_monitor;
mod signer_client;

use anyhow::Result;
use std::sync::Arc;
use tracing::info;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    info!("Position Manager v25 starting up...");

    let db = Arc::new(database::Database::new(&config::CONFIG.database_path)?);

    position_monitor::run_monitor(db).await?;

    Ok(())
}
