mod config;
mod database;
mod executor;
mod risk_manager;
mod jito_client;
mod jupiter;
mod signer_client;
mod strategies;

use anyhow::{Context, Result};
use database::Database;
use executor::MasterExecutor;
use std::sync::Arc;
use tracing::{error, info};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging first
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    info!("🚀 Starting MemeSnipe v25 Executor - Restored Working Version");

    // Load configuration explicitly
    let _config = Arc::new(config::Config::load().context("Failed to load configuration")?);
    info!("✅ Configuration loaded");

    // Initialize database
    let db = Arc::new(
        database::Database::new(&config::CONFIG.database_path)
            .context("Failed to initialize database")?,
    );
    info!("💾 Database initialized");

    // Create master executor
    let mut executor = MasterExecutor::new(db)
        .await
        .context("Failed to create master executor")?;
    info!("🎯 MasterExecutor initialized, starting event loop...");

    // Run the executor - this blocks forever
    if let Err(e) = executor.run().await {
        error!("💥 Executor failed: {}", e);
        return Err(e);
    }

    Ok(())
}
        return Err(e);
    }

    Ok(())
}
