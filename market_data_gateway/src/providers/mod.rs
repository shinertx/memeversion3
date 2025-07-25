use anyhow::Result;
use async_trait::async_trait;
use shared_models::MarketEvent;
use tokio::sync::mpsc;

pub mod helius_consumer;
pub mod pyth_consumer;

#[async_trait]
pub trait DataProvider: Send + Sync {
    async fn run(tx: mpsc::Sender<MarketEvent>) -> Result<()>;
}
