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

// Strategy registration macro
#[macro_export]
macro_rules! register_strategy {
    ($strategy_type:ty, $strategy_id:expr) => {
        // Macro for strategy registration - implementation in executor
    };
}
