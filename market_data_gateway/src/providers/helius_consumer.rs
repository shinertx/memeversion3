use crate::config::CONFIG;
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use reqwest::Client;
use serde_json::Value;
use shared_models::{MarketEvent, PriceTick, DepthEvent, BridgeEvent, OnChainEvent};
use tokio::sync::mpsc;
use tracing::{error, info, warn};
use std::time::Duration;

pub struct HeliusConsumer;

pub async fn run(tx: mpsc::Sender<MarketEvent>) -> Result<()> {
    info!("Starting Helius Data Consumer...");
    let client = Client::new();
    let api_key = CONFIG.helius_api_key.clone();
    let rpc_url = format!("https://rpc.helius.xyz/?api-key={}", api_key);

    loop {
        // Simulate various event types for demo purposes
        // In production, these would come from real Helius WebSocket connections
        
        // Price events
        if let Err(e) = tx.send(MarketEvent::Price(PriceTick {
            token_address: "So11111111111111111111111111111111111111112".to_string(),
            price_usd: 150.0 + (rand::random::<f64>() * 10.0 - 5.0),
            volume_usd_1m: rand::random::<f64>() * 100000.0,
        })).await { error!("Failed to send PriceTick: {}", e); }

        // Depth events
        if let Err(e) = tx.send(MarketEvent::Depth(DepthEvent {
            token_address: "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v".to_string(),
            bid_price: 0.999,
            ask_price: 1.001,
            bid_size_usd: rand::random::<f64>() * 50000.0,
            ask_size_usd: rand::random::<f64>() * 50000.0,
        })).await { error!("Failed to send DepthEvent: {}", e); }

        // Bridge events
        if rand::random::<f64>() < 0.1 {
            if let Err(e) = tx.send(MarketEvent::Bridge(BridgeEvent {
                token_address: format!("MEME{}", rand::random::<u32>() % 100),
                source_chain: "ethereum".to_string(),
                destination_chain: "solana".to_string(),
                volume_usd: rand::random::<f64>() * 1000000.0,
            })).await { error!("Failed to send BridgeEvent: {}", e); }
        }

        // OnChain events
        if rand::random::<f64>() < 0.05 {
            if let Err(e) = tx.send(MarketEvent::OnChain(OnChainEvent {
                token_address: format!("MEME{}", rand::random::<u32>() % 100),
                event_type: "LP_LOCK".to_string(),
                details: serde_json::json!({"locked": true, "duration_days": 30}),
            })).await { error!("Failed to send OnChainEvent: {}", e); }
        }

        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}
