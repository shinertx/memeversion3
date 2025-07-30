// Strategy modules for MemeSnipe v25
pub mod airdrop_rotation;
pub mod bridge_inflow; 
pub mod dev_wallet_drain;
pub mod korean_time_burst;
pub mod liquidity_migration;
pub mod perp_basis_arb;

// Re-export all strategies
pub use airdrop_rotation::*;
pub use bridge_inflow::*;
pub use dev_wallet_drain::*;
pub use korean_time_burst::*;
pub use liquidity_migration::*;
pub use perp_basis_arb::*;

// Re-export Strategy trait and related types from shared-models
pub use shared_models::{Strategy, MarketEvent, StrategyAction, OrderDetails, EventType};

// Strategy creation function
pub fn create_strategy(strategy_type: &str) -> Result<Box<dyn Strategy + Send>, anyhow::Error> {
    match strategy_type {
        "airdrop_rotation" => Ok(Box::new(airdrop_rotation::AirdropRotation::default())),
        "bridge_inflow" => Ok(Box::new(bridge_inflow::BridgeInflow::default())),
        "dev_wallet_drain" => Ok(Box::new(dev_wallet_drain::DevWalletDrain::default())),
        "korean_time_burst" => Ok(Box::new(korean_time_burst::KoreanTimeBurst::default())),
        "liquidity_migration" => Ok(Box::new(liquidity_migration::LiquidityMigration::default())),
        "perp_basis_arb" => Ok(Box::new(perp_basis_arb::PerpBasisArb::default())),
        _ => Err(anyhow::anyhow!("Unknown strategy type: {}", strategy_type))
    }
}
