use anyhow::Result;
use async_trait::async_trait;
use shared_models::MarketEvent;
use tokio::sync::mpsc;

#[async_trait]
pub trait DataProvider: Send + Sync {
    async fn run(tx: mpsc::Sender<MarketEvent>) -> Result<()>;
}

// Simple validation for simulation mode
pub fn validate_simulated_event(event: &MarketEvent) -> bool {
    match event {
        MarketEvent::Price(price_tick) => price_tick.price_usd > 0.0,
        MarketEvent::SolPrice(sol_price) => sol_price.price_usd > 0.0,
        _ => true, // Other events are considered valid in simulation
    }
}
