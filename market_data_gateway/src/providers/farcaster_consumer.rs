use crate::config::CONFIG;
use anyhow::Result;
use reqwest::Client;
use shared_models::{MarketEvent, FarcasterRawEvent};
use tokio::sync::mpsc;
use tracing::{error, info};
use std::time::Duration;

pub async fn run(tx: mpsc::Sender<MarketEvent>) -> Result<()> {
    info!("Starting Farcaster Data Consumer...");
    let client = Client::new();
    let farcaster_api_url = CONFIG.farcaster_api_url.clone();

    loop {
        // Simulate Farcaster data
        let cast_hash = format!("cast_{}", rand::random::<u64>());
        
        if let Err(e) = tx.send(MarketEvent::FarcasterRaw(FarcasterRawEvent {
            cast_hash,
            text: "New memecoin alert: $PEPE2 launching on pump.fun".to_string(),
            author_fid: format!("fid_{}", rand::random::<u32>() % 1000),
            timestamp: chrono::Utc::now().timestamp(),
        })).await { error!("Failed to send FarcasterRawEvent: {}", e); }

        tokio::time::sleep(Duration::from_secs(5)).await;
    }
}
