use crate::strategies::{Strategy, MarketEvent, StrategyAction, OrderDetails, EventType};
use anyhow::Result;
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashSet;
use tracing::info;
use chrono::{Timelike, Utc};
use shared_models::Side;

#[derive(Default, Deserialize)]
pub struct KoreanTimeBurst {
    volume_multiplier_threshold: f64,
    #[serde(skip)] active_burst_tokens: HashSet<String>,
}

#[async_trait]
impl Strategy for KoreanTimeBurst {
    fn id(&self) -> &'static str { "korean_time_burst" }
    fn subscriptions(&self) -> HashSet<EventType> { [EventType::Price].iter().cloned().collect() }

    async fn init(&mut self, params: &Value) -> Result<()> {
        #[derive(Deserialize)] struct P { volume_multiplier_threshold: f64 }
        let p: P = serde_json::from_value(params.clone())?;
        self.volume_multiplier_threshold = p.volume_multiplier_threshold;
        info!(strategy = self.id(), "Initialized with volume_multiplier_threshold: {}", self.volume_multiplier_threshold);
        Ok(())
    }

    async fn on_event(&mut self, event: &MarketEvent) -> Result<StrategyAction> {
        if let MarketEvent::Price(tick) = event {
            let now = Utc::now().with_timezone(&chrono_tz::Asia::Seoul);
            let hour = now.hour();

            // Korean market hours: 9 AM - 3 PM KST
            let is_korean_trading_hour = hour >= 9 && hour <= 15;

            if is_korean_trading_hour {
                if tick.volume_usd_1m > 50_000.0 * self.volume_multiplier_threshold && 
                   !self.active_burst_tokens.contains(&tick.token_address) {
                    info!(
                        id = self.id(), 
                        token = %tick.token_address, 
                        "BUY signal: Detected Korean time volume burst (V: {:.0} USD, Hour: {} KST).", 
                        tick.volume_usd_1m,
                        hour
                    );
                    self.active_burst_tokens.insert(tick.token_address.clone());
                    return Ok(StrategyAction::Execute(OrderDetails {
                        token_address: tick.token_address.clone(),
                        suggested_size_usd: 650.0,
                        confidence: 0.7,
                        side: Side::Long,
                    }));
                }
            } else if !is_korean_trading_hour {
                // Clear the active tokens list outside of Korean hours
                self.active_burst_tokens.clear();
            }
        }
        Ok(StrategyAction::Hold)
    }
}
