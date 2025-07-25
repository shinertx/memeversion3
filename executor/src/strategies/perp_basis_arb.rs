use crate::{register_strategy, strategies::{Strategy, MarketEvent, StrategyAction, OrderDetails, EventType}};
use anyhow::Result;
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::Value;
use std::collections::{HashSet, HashMap};
use tracing::info;
use shared_models::Side;

#[derive(Default, Deserialize)]
struct PerpBasisArb {
    basis_threshold_pct: f64,
    #[serde(skip)] spot_prices: HashMap<String, f64>,
    #[serde(skip)] funding_rates: HashMap<String, f64>,
}

#[async_trait]
impl Strategy for PerpBasisArb {
    fn id(&self) -> &'static str { "perp_basis_arb" }
    fn subscriptions(&self) -> HashSet<EventType> {
        [EventType::Price, EventType::Funding].iter().cloned().collect()
    }

    async fn init(&mut self, params: &Value) -> Result<()> {
        #[derive(Deserialize)] struct P { basis_threshold_pct: f64 }
        let p: P = serde_json::from_value(params.clone())?;
        self.basis_threshold_pct = p.basis_threshold_pct;
        info!(strategy = self.id(), "Initialized with basis_threshold_pct: {}", self.basis_threshold_pct);
        Ok(())
    }

    async fn on_event(&mut self, event: &MarketEvent) -> Result<StrategyAction> {
        match event {
            MarketEvent::Price(tick) => {
                self.spot_prices.insert(tick.token_address.clone(), tick.price_usd);
            }
            MarketEvent::Funding(funding_event) => {
                self.funding_rates.insert(funding_event.token_address.clone(), funding_event.funding_rate_pct);
                
                // Check for arbitrage opportunity when we have both spot and funding data
                if let Some(&spot_price) = self.spot_prices.get(&funding_event.token_address) {
                    let basis = funding_event.funding_rate_pct;
                    
                    if basis.abs() > self.basis_threshold_pct {
                        if basis > 0.0 {
                            info!(
                                id = self.id(), 
                                token = %funding_event.token_address, 
                                "SHORT PERP signal: Positive basis {:.2}% exceeds threshold.",
                                basis
                            );
                            return Ok(StrategyAction::Execute(OrderDetails {
                                token_address: funding_event.token_address.clone(),
                                suggested_size_usd: 800.0,
                                confidence: 0.9,
                                side: Side::Short,
                            }));
                        } else {
                            info!(
                                id = self.id(), 
                                token = %funding_event.token_address, 
                                "LONG PERP signal: Negative basis {:.2}% exceeds threshold.",
                                basis
                            );
                            return Ok(StrategyAction::Execute(OrderDetails {
                                token_address: funding_event.token_address.clone(),
                                suggested_size_usd: 800.0,
                                confidence: 0.9,
                                side: Side::Long,
                            }));
                        }
                    }
                }
            }
            _ => {}
        }
        Ok(StrategyAction::Hold)
    }
}
register_strategy!(PerpBasisArb, "perp_basis_arb");
