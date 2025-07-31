mod config;
mod state_manager;

use anyhow::Result;
use redis::AsyncCommands;
use shared_models::{StrategySpec, StrategyAllocation, TradeMode};
use std::collections::HashMap;
use tokio::time::{interval, Duration};
use tracing::{debug, error, info, warn};
use tracing_subscriber::EnvFilter;
use serde_json;

use config::CONFIG;
use state_manager::{StrategyState, StateManager};

#[tokio::main]
async fn main() -> Result<()> {
    let filter = EnvFilter::builder()
        .with_default_directive(tracing::level_filters::LevelFilter::INFO.into())
        .from_env_lossy();
    tracing_subscriber::fmt().with_env_filter(filter).init();

    info!("📊 Starting Portfolio Manager v25...");

    let redis_url = CONFIG.redis_url.clone();
    let client = redis::Client::open(redis_url)?;
    let mut conn = client.get_multiplexed_async_connection().await?;
    
    let mut portfolio_state_manager = StateManager::new(CONFIG.initial_capital_usd);

    let mut allocation_interval = interval(Duration::from_secs(CONFIG.rebalance_interval_secs));
    let mut promotion_interval = interval(Duration::from_secs(CONFIG.strategy_promotion_interval_secs));

    loop {
        tokio::select! {
            _ = allocation_interval.tick() => {
                info!("📈 Portfolio rebalance triggered...");

                // 1. Read latest performance metrics for all strategies
                simulate_performance_updates(&mut portfolio_state_manager);

                // 2. Calculate new allocations based on performance
                let allocations = calculate_allocations(&portfolio_state_manager);

                // 3. Publish allocations to Redis
                if !allocations.is_empty() {
                    info!("Publishing {} new allocations...", allocations.len());
                    let serialized_allocations = serde_json::to_string(&allocations)?;
                    conn.xadd("allocations_channel", "*", &[("allocations", serialized_allocations)]).await?;
                } else {
                    info!("No active strategies to allocate to.");
                }
            },
            _ = promotion_interval.tick() => {
                info!("🏆 Strategy promotion check triggered...");
                promote_strategies(&mut portfolio_state_manager);
            }
        }

        // Read new strategy specs from Redis stream written by Strategy Factory
        let strategy_stream_key = "strategy_specs";
        let last_id = portfolio_state_manager.get_last_stream_id(strategy_stream_key).unwrap_or("0-0".to_string());

        let result: Result<Option<redis::streams::StreamReadReply>, _> = conn
            .xread_options(&[strategy_stream_key], &[&last_id], &redis::streams::StreamReadOptions::default().count(100).block(1000))
            .await;

        if let Ok(Some(reply)) = result {
            for stream_read in reply.keys {
                for message in stream_read.ids {
                    if let Ok(spec_str) = message.get::<String, _>("spec") {
                        if let Ok(spec) = serde_json::from_str::<StrategySpec>(&spec_str) {
                            info!("Discovered new strategy spec: {} (family: {})", spec.id, spec.family);
                            portfolio_state_manager.add_strategy_spec(spec);
                        } else {
                            warn!("Failed to deserialize strategy spec: {}", spec_str);
                        }
                    }
                    portfolio_state_manager.set_last_stream_id(strategy_stream_key, message.id);
                }
            }
        }
    }
}

/// Simulates performance updates for strategies.
/// In a real system, this data would come from the position_manager.
fn simulate_performance_updates(state_manager: &mut StateManager) {
    for state in state_manager.get_all_strategy_states_mut() {
        // Simulate some random performance
        let random_pnl = (rand::random::<f64>() - 0.45) * 100.0; // Skew towards positive
        state.realized_pnl += random_pnl;
        state.sharpe_ratio = state.realized_pnl / (state.run_time_secs as f64 + 1.0); // Simplified Sharpe
        info!("Simulated performance for {}: PnL ${:.2}, Sharpe {:.2}", state.spec.id, state.realized_pnl, state.sharpe_ratio);
    }
}

/// Calculates new capital allocations based on strategy performance.
fn calculate_allocations(state_manager: &StateManager) -> Vec<StrategyAllocation> {
    let mut allocations = Vec::new();
    let total_capital = state_manager.get_total_capital();

    // Filter for strategies in Paper or Live mode
    let active_strategies: Vec<&StrategyState> = state_manager
        .get_all_strategy_states()
        .iter()
        .filter(|s| s.mode == TradeMode::Paper || s.mode == TradeMode::Live)
        .collect();

    if active_strategies.is_empty() {
        return allocations;
    }

    // Performance-weighted allocation (e.g., based on Sharpe ratio)
    let total_performance_score: f64 = active_strategies.iter().map(|s| s.sharpe_ratio.max(0.0)).sum();

    if total_performance_score <= 0.0 {
        info!("No strategies with positive performance score. No allocations will be made.");
        return allocations;
    }

    for state in active_strategies {
        let weight = state.sharpe_ratio.max(0.0) / total_performance_score;
        let capital_allocation = total_capital * weight;

        let allocation = StrategyAllocation {
            strategy_id: state.spec.id.clone(),
            capital_usd: capital_allocation,
            mode: state.mode,
        };
        allocations.push(allocation);
    }

    allocations
}

/// Promotes strategies from Simulating to Paper trading based on performance thresholds.
fn promote_strategies(state_manager: &mut StateManager) {
    for state in state_manager.get_all_strategy_states_mut() {
        if state.mode == TradeMode::Simulating && state.sharpe_ratio > CONFIG.min_sharpe_for_promotion {
            info!("🏆 Promoting strategy {} to Paper Trading! Sharpe: {:.2}", state.spec.id, state.sharpe_ratio);
            state.mode = TradeMode::Paper;
        }
    }
}
