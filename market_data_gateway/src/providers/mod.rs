use anyhow::Result;
use async_trait::async_trait;
use shared_models::MarketEvent;
use tokio::sync::mpsc;
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{warn, error, info};

pub mod helius_consumer;
pub mod pyth_consumer;
pub mod twitter_consumer;
pub mod farcaster_consumer;

// Data validation thresholds
const MAX_DATA_AGE_MS: u64 = 500; // 500ms max age for market data
const MAX_PRICE_DEVIATION: f64 = 0.05; // 5% max deviation between sources
const CIRCUIT_BREAKER_THRESHOLD: f64 = 0.3; // 30% bad data triggers circuit breaker

#[async_trait]
pub trait DataProvider: Send + Sync {
    async fn run(tx: mpsc::Sender<MarketEvent>) -> Result<()>;
}

#[derive(Clone, Debug)]
pub struct ValidatedEvent {
    pub event: MarketEvent,
    pub timestamp_ms: u64,
    pub source: String,
    pub is_valid: bool,
    pub validation_errors: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct PriceValidationData {
    pub price: f64,
    pub timestamp_ms: u64,
    pub source: String,
}

pub struct DataValidator {
    // Store recent prices for cross-validation
    recent_prices: HashMap<String, Vec<PriceValidationData>>,
    circuit_breaker_active: bool,
    invalid_data_count: u64,
    total_data_count: u64,
}

impl DataValidator {
    pub fn new() -> Self {
        Self {
            recent_prices: HashMap::new(),
            circuit_breaker_active: false,
            invalid_data_count: 0,
            total_data_count: 0,
        }
    }

    pub async fn validate_event(&mut self, event: MarketEvent, source: &str) -> ValidatedEvent {
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        let mut validation_errors = Vec::new();
        let mut is_valid = true;

        // Validate based on event type
        match &event {
            MarketEvent::Price(price_tick) => {
                // Check for reasonable price bounds
                if price_tick.price_usd <= 0.0 || price_tick.price_usd > 1_000_000.0 {
                    validation_errors.push(format!("Price out of bounds: {}", price_tick.price_usd));
                    is_valid = false;
                }

                // Cross-validate with other sources
                if let Some(recent_prices) = self.recent_prices.get(&price_tick.token_address) {
                    let valid_recent: Vec<_> = recent_prices.iter()
                        .filter(|p| now_ms - p.timestamp_ms < MAX_DATA_AGE_MS * 2)
                        .collect();

                    if valid_recent.len() > 0 {
                        let avg_price: f64 = valid_recent.iter().map(|p| p.price).sum::<f64>() / valid_recent.len() as f64;
                        let deviation = (price_tick.price_usd - avg_price).abs() / avg_price;
                        
                        if deviation > MAX_PRICE_DEVIATION {
                            validation_errors.push(format!(
                                "Price deviation {}% exceeds threshold {}%", 
                                deviation * 100.0, 
                                MAX_PRICE_DEVIATION * 100.0
                            ));
                            is_valid = false;
                        }
                    }
                }

                // Store this price for future validation
                let price_data = PriceValidationData {
                    price: price_tick.price_usd,
                    timestamp_ms: now_ms,
                    source: source.to_string(),
                };

                self.recent_prices
                    .entry(price_tick.token_address.clone())
                    .or_insert_with(Vec::new)
                    .push(price_data);

                // Keep only recent data (last 5 seconds)
                if let Some(prices) = self.recent_prices.get_mut(&price_tick.token_address) {
                    prices.retain(|p| now_ms - p.timestamp_ms < 5000);
                }
            }
            MarketEvent::Depth(depth_event) => {
                // Validate depth data
                if depth_event.bid_price <= 0.0 || depth_event.ask_price <= 0.0 {
                    validation_errors.push("Invalid bid/ask prices".to_string());
                    is_valid = false;
                }
                if depth_event.ask_price <= depth_event.bid_price {
                    validation_errors.push("Ask price must be greater than bid price".to_string());
                    is_valid = false;
                }
                if depth_event.bid_size_usd < 0.0 || depth_event.ask_size_usd < 0.0 {
                    validation_errors.push("Negative size values".to_string());
                    is_valid = false;
                }
            }
            MarketEvent::Bridge(bridge_event) => {
                // Validate bridge events
                if bridge_event.volume_usd <= 0.0 {
                    validation_errors.push("Invalid bridge volume".to_string());
                    is_valid = false;
                }
            }
            MarketEvent::Social(_social_event) => {
                // Social events are generally trusted if they pass basic parsing
                // Could add sentiment validation here
            }
            MarketEvent::Funding(funding_event) => {
                // Validate funding rate bounds
                if funding_event.funding_rate_pct.abs() > 100.0 {
                    validation_errors.push(format!("Extreme funding rate: {}%", funding_event.funding_rate_pct));
                    is_valid = false;
                }
            }
            MarketEvent::SolPrice(sol_price) => {
                // Validate SOL price bounds
                if sol_price.price_usd <= 0.0 || sol_price.price_usd > 10000.0 {
                    validation_errors.push(format!("SOL price out of bounds: {}", sol_price.price_usd));
                    is_valid = false;
                }
            }
            MarketEvent::TwitterRaw(_twitter_event) => {
                // Twitter events are generally trusted if they pass basic parsing
            }
            MarketEvent::FarcasterRaw(_farcaster_event) => {
                // Farcaster events are generally trusted if they pass basic parsing
            }
            MarketEvent::OnChain(_) => {
                // OnChain events are generally trusted if they pass basic parsing
            }
        }

        // Update circuit breaker metrics
        self.total_data_count += 1;
        if !is_valid {
            self.invalid_data_count += 1;
        }

        // Check circuit breaker condition
        if self.total_data_count > 100 { // Only check after we have some data
            let invalid_ratio = self.invalid_data_count as f64 / self.total_data_count as f64;
            if invalid_ratio > CIRCUIT_BREAKER_THRESHOLD && !self.circuit_breaker_active {
                error!("CIRCUIT BREAKER ACTIVATED: Invalid data ratio: {:.2}%", invalid_ratio * 100.0);
                self.circuit_breaker_active = true;
                is_valid = false;
                validation_errors.push("Circuit breaker active - too much invalid data".to_string());
            } else if invalid_ratio < CIRCUIT_BREAKER_THRESHOLD / 2.0 && self.circuit_breaker_active {
                info!("Circuit breaker deactivated - data quality improved");
                self.circuit_breaker_active = false;
            }
        }

        // Log validation issues
        if !is_valid {
            warn!("Invalid data from {}: {:?}", source, validation_errors);
        }

        ValidatedEvent {
            event,
            timestamp_ms: now_ms,
            source: source.to_string(),
            is_valid,
            validation_errors,
        }
    }

    pub fn is_circuit_breaker_active(&self) -> bool {
        self.circuit_breaker_active
    }

    pub fn get_data_quality_stats(&self) -> (u64, u64, f64) {
        let invalid_ratio = if self.total_data_count > 0 {
            self.invalid_data_count as f64 / self.total_data_count as f64
        } else {
            0.0
        };
        (self.invalid_data_count, self.total_data_count, invalid_ratio)
    }
}
