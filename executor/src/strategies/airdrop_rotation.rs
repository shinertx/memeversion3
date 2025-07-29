use crate::{register_strategy, strategies::{Strategy, MarketEvent, StrategyAction, OrderDetails, EventType}};
use anyhow::Result;
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::Value;
use std::collections::{HashSet, HashMap};
use tracing::info;
use shared_models::Side;

#[derive(Default, Deserialize)]
pub struct AirdropRotation {
    min_new_holders: u32,
    #[serde(skip)] token_holder_counts: HashMap<String, u32>,
}

#[async_trait]
impl Strategy for AirdropRotation {
    fn id(&self) -> &'static str { "airdrop_rotation" }
    fn subscriptions(&self) -> HashSet<EventType> { 
        [EventType::Social, EventType::Price, EventType::OnChain].iter().cloned().collect() 
    }

    async fn init(&mut self, params: &Value) -> Result<()> {
        #[derive(Deserialize)] struct P { min_new_holders: u32 }
        let p: P = serde_json::from_value(params.clone())?;
        self.min_new_holders = p.min_new_holders;
        info!(strategy = self.id(), "Initialized with min_new_holders: {}", self.min_new_holders);
        Ok(())
    }

    async fn on_event(&mut self, event: &MarketEvent) -> Result<StrategyAction> {
        match event {
            MarketEvent::OnChain(on_chain) if on_chain.event_type == "HOLDER_DELTA" => {
                if let Some(delta) = on_chain.details.get("holder_delta").and_then(|v| v.as_u64()) {
                    if delta as u32 > self.min_new_holders {
                        info!(id = self.id(), token = %on_chain.token_address, "BUY signal: Detected airdrop with {} new holders.", delta);
                        return Ok(StrategyAction::Execute(OrderDetails {
                            token_address: on_chain.token_address.clone(),
                            suggested_size_usd: 600.0,
                            confidence: 0.7,
                            side: Side::Long,
                        }));
                    }
                }
            }
            MarketEvent::Social(mention) if mention.sentiment > 0.5 => {
                let current_holders = self.token_holder_counts.entry(mention.token_address.clone()).or_insert(100);
                let new_holders_simulated = (rand::random::<u32>() % 200) + 50;
                *current_holders += new_holders_simulated;

                if new_holders_simulated > self.min_new_holders {
                    info!(id = self.id(), token = %mention.token_address, "BUY signal: Simulated airdrop detected with {} new holders.", new_holders_simulated);
                    return Ok(StrategyAction::Execute(OrderDetails {
                        token_address: mention.token_address.clone(),
                        suggested_size_usd: 600.0,
                        confidence: 0.7,
                        side: Side::Long,
                    }));
                }
            }
            _ => {}
        }
        Ok(StrategyAction::Hold)
    }
}
register_strategy!(AirdropRotation, "airdrop_rotation");
