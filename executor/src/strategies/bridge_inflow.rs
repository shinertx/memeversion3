use crate::{register_strategy, strategies::{Strategy, MarketEvent, StrategyAction, OrderDetails, EventType}};
use anyhow::Result;
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashSet;
use tracing::info;
use shared_models::Side;

#[derive(Default, Deserialize)]
pub struct BridgeInflow {
    min_bridge_volume_usd: f64,
    #[serde(skip)] tokens_with_recent_inflow: HashSet<String>,
}

#[async_trait]
impl Strategy for BridgeInflow {
    fn id(&self) -> &'static str { "bridge_inflow" }
    fn subscriptions(&self) -> HashSet<EventType> {
        [EventType::Bridge].iter().cloned().collect()
    }

    async fn init(&mut self, params: &Value) -> Result<()> {
        #[derive(Deserialize)] struct P { min_bridge_volume_usd: f64 }
        let p: P = serde_json::from_value(params.clone())?;
        self.min_bridge_volume_usd = p.min_bridge_volume_usd;
        info!(strategy = self.id(), "Initialized with min_bridge_volume_usd: {}", self.min_bridge_volume_usd);
        Ok(())
    }

    async fn on_event(&mut self, event: &MarketEvent) -> Result<StrategyAction> {
        if let MarketEvent::Bridge(bridge_event) = event {
            if bridge_event.volume_usd > self.min_bridge_volume_usd && !self.tokens_with_recent_inflow.contains(&bridge_event.token_address) {
                info!(
                    id = self.id(),
                    token = %bridge_event.token_address,
                    "BUY signal: Detected significant bridge inflow of {:.0} USD from {} to {}.",
                    bridge_event.volume_usd,
                    bridge_event.source_chain,
                    bridge_event.destination_chain
                );
                self.tokens_with_recent_inflow.insert(bridge_event.token_address.clone());
                return Ok(StrategyAction::Execute(OrderDetails {
                    token_address: bridge_event.token_address.clone(),
                    suggested_size_usd: 800.0,
                    confidence: 0.85,
                    side: Side::Long,
                }));
            }
        }
        Ok(StrategyAction::Hold)
    }
}
register_strategy!(BridgeInflow, "bridge_inflow");
