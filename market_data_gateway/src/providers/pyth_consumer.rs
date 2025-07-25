use crate::config::CONFIG;
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use reqwest::Client;
use serde_json::Value;
use shared_models::{MarketEvent, SolPriceEvent, FundingEvent};
use tokio::sync::mpsc;
use tracing::{error, info, warn};
use std::time::Duration;

pub struct PythConsumer;

pub async fn run(tx: mpsc::Sender<MarketEvent>) -> Result<()> {
    info!("Starting Pyth Data Consumer...");
    let client = Client::new();
    let pyth_api_key = CONFIG.pyth_api_key.clone();

    loop {
        // SOL price updates
        let sol_price = 150.0 + (rand::random::<f64>() * 20.0 - 10.0);
        if let Err(e) = tx.send(MarketEvent::SolPrice(SolPriceEvent { 
            price_usd: sol_price 
        })).await {
            error!("Failed to send SolPriceEvent: {}", e);
        }

        // Funding rate events
        if rand::random::<f64>() < 0.2 {
            if let Err(e) = tx.send(MarketEvent::Funding(FundingEvent {
                token_address: "So11111111111111111111111111111111111111112".to_string(),
                funding_rate_pct: (rand::random::<f64>() * 0.02 - 0.01) * 100.0,
            })).await { error!("Failed to send FundingEvent: {}", e); }
        }

        tokio::time::sleep(Duration::from_secs(5)).await;
    }
}
