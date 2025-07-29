use crate::{register_strategy, strategies::{Strategy, MarketEvent, StrategyAction, OrderDetails, EventType}};
use anyhow::Result;
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashSet;
use tracing::info;
use shared_models::Side;

#[derive(Default, Deserialize)]
pub struct DevWalletDrain {
    dev_balance_threshold_pct: f64,
    #[serde(skip)] monitored_dev_wallets: HashSet<String>,
}

#[async_trait]
impl Strategy for DevWalletDrain {
    fn id(&self) -> &'static str { "dev_wallet_drain" }
    fn subscriptions(&self) -> HashSet<EventType> { 
        [EventType::OnChain, EventType::Price].iter().cloned().collect() 
    }

    async fn init(&mut self, params: &Value) -> Result<()> {
        #[derive(Deserialize)] struct P { dev_balance_threshold_pct: f64 }
        let p: P = serde_json::from_value(params.clone())?;
        self.dev_balance_threshold_pct = p.dev_balance_threshold_pct;
        info!(strategy = self.id(), "Initialized with dev_balance_threshold_pct: {}", self.dev_balance_threshold_pct);
        Ok(())
    }

    async fn on_event(&mut self, event: &MarketEvent) -> Result<StrategyAction> {
        match event {
            MarketEvent::OnChain(on_chain) if on_chain.event_type == "DEV_WALLET_TRANSFER" => {
                if let Some(transfer_pct) = on_chain.details.get("transfer_percentage").and_then(|v| v.as_f64()) {
                    if transfer_pct > self.dev_balance_threshold_pct {
                        info!(id = self.id(), token = %on_chain.token_address, "SHORT signal: Dev wallet dump detected ({:.1}% transferred).", transfer_pct);
                        return Ok(StrategyAction::Execute(OrderDetails {
                            token_address: on_chain.token_address.clone(),
                            suggested_size_usd: 1200.0,
                            confidence: 0.85,
                            side: Side::Short,
                        }));
                    }
                }
            }
            MarketEvent::Price(tick) if tick.price_usd < 0.2 && tick.volume_usd_1m > 200_000.0 => {
                info!(id = self.id(), token = %tick.token_address, "SHORT signal: Possible dev wallet dump detected (simulated).");
                return Ok(StrategyAction::Execute(OrderDetails {
                    token_address: tick.token_address.clone(),
                    suggested_size_usd: 1200.0,
                    confidence: 0.85,
                    side: Side::Short,
                }));
            }
            _ => {}
        }
        Ok(StrategyAction::Hold)
    }
}
register_strategy!(DevWalletDrain, "dev_wallet_drain");
