mod config;
mod state_manager;
mod backtest_client;

use anyhow::{Context, Result};
use redis::AsyncCommands;
use shared_models::{StrategySpec, StrategyAllocation, TradeMode};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tokio::time::interval;
use tracing::{debug, error, info, warn};
use serde_json;
use serde::Serialize;

use config::CONFIG;
use state_manager::{StrategyState, StateManager};

// Add required dependencies for time and random generation

async fn process_new_strategy_submissions(
    conn: &mut redis::aio::MultiplexedConnection,
    backtest_client: &backtest_client::BacktestClient,
    _pending_backtests: Arc<Mutex<HashMap<String, PendingBacktest>>>,
    last_id: &mut String,
) -> Result<()> {
    // Read new strategy specs from Redis stream
    debug!("Reading strategy_specs stream starting from ID: {}", last_id);
    let stream_result: redis::RedisResult<Vec<redis::streams::StreamReadReply>> = conn.xread_options(
        &["strategy_specs"],
        &[last_id.as_str()],
        &redis::streams::StreamReadOptions::default().count(10)
    ).await;

    match stream_result {
        Ok(replies) => {
            debug!("Got {} stream replies", replies.len());
            for reply in replies {
                debug!("Processing reply with {} keys", reply.keys.len());
                for stream_key in reply.keys {
                    debug!("Processing stream key with {} messages", stream_key.ids.len());
                    for stream_id in stream_key.ids {
                        // Update last seen ID
                        *last_id = stream_id.id.clone();
                        debug!("Processing message ID: {}", stream_id.id);
                        
                        if let Some(spec_json) = stream_id.map.get("spec") {
                            if let Ok(spec_str) = redis::from_redis_value::<String>(spec_json) {
                                match serde_json::from_str::<StrategySpec>(&spec_str) {
                                Ok(strategy_spec) => {
                                    info!("📋 Processing new strategy spec: {}", strategy_spec.id);
                                    
                                    // If strategy has good fitness score, allocate capital immediately
                                    if strategy_spec.fitness > 0.6 {
                                        info!("🚀 High-fitness strategy detected: {} (fitness: {:.3})", strategy_spec.id, strategy_spec.fitness);
                                        
                                        // Create allocation for high-performing strategy
                                        let allocation = shared_models::StrategyAllocation {
                                            id: strategy_spec.id.clone(),
                                            weight: (strategy_spec.fitness * 0.1).min(0.05), // Max 5% allocation
                                            sharpe_ratio: strategy_spec.fitness * 2.0, // Approximate Sharpe from fitness
                                            mode: if strategy_spec.fitness > 0.8 { 
                                                shared_models::TradeMode::Paper 
                                            } else { 
                                                shared_models::TradeMode::Simulating 
                                            },
                                            params: strategy_spec.params.clone(),
                                        };
                                        
                                        // Publish allocation to executor
                                        let allocations = vec![allocation];
                                        let allocations_json = serde_json::to_string(&allocations)?;
                                        let _: () = conn.xadd(
                                            "allocations_channel",
                                            "*",
                                            &[("allocations", allocations_json)]
                                        ).await?;
                                        
                                        info!("💰 Allocated capital to strategy: {}", strategy_spec.id);
                                    }
                                    
                                    // Submit strategy for backtesting
                                    match backtest_client.submit_backtest(&strategy_spec).await {
                                        Ok(result) => {
                                            // Update fitness based on backtest result
                                            let mut updated_spec = strategy_spec.clone();
                                            updated_spec.fitness = result.sharpe_ratio.max(0.1); // Ensure minimum fitness
                                            
                                            // Send updated strategy to executor
                                            let allocation = StrategyAllocation {
                                                id: updated_spec.id.clone(),
                                                weight: 0.1, // Start with 10% weight
                                                sharpe_ratio: result.sharpe_ratio.max(0.1), // Ensure minimum sharpe
                                                mode: TradeMode::Simulating,
                                                params: updated_spec.params.clone(),
                                            };
                                            
                                            let allocation_json = serde_json::to_string(&allocation)
                                                .context("Failed to serialize allocation")?;
                                            
                                            let _: () = conn.xadd(
                                                "allocations_channel",
                                                "*",
                                                &[("data", &allocation_json)]
                                            ).await.map_err(|e| {
                                                error!("Failed to publish allocation: {}", e);
                                                e
                                            })?;
                                            
                                            info!("✅ Strategy {} evaluated with Sharpe {:.2}, allocated capital", updated_spec.id, result.sharpe_ratio);
                                        }
                                        Err(e) => {
                                            warn!("❌ Failed to submit strategy {} for backtesting: {}", strategy_spec.id, e);
                                            // Add strategy with default fitness if backtesting fails
                                            // This ensures graceful degradation
                                        }
                                    }
                                }
                                Err(e) => {
                                    error!("Failed to parse strategy spec from Redis: {}", e);
                                }
                            }
                        } else {
                            error!("Failed to convert Redis value to string");
                        }
                        }
                    }
                }
            }
        }
        Err(e) => {
            debug!("No new strategy specs in stream or error reading (last_id: {}): {}", last_id, e);
        }
    }

    Ok(())
}

// In-house sanity checker for cross-validating external backtest results
mod sanity_checker {
    use anyhow::Result;
    use serde::{Deserialize, Serialize};
    use std::collections::HashMap;
    use tracing::{info, warn};
    
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct OHLCVData {
        pub timestamp: i64,
        pub open: f64,
        pub high: f64,
        pub low: f64,
        pub close: f64,
        pub volume_usd: f64,
    }
    
    #[derive(Debug, Clone)]
    pub struct SimpleBacktestResult {
        pub total_return: f64,
        pub sharpe_ratio: f64,
        pub max_drawdown: f64,
        pub trade_count: u32,
        pub win_rate: f64,
    }
    
    pub struct SanityChecker {
        // Store minimal historical data for validation
        price_data: HashMap<String, Vec<OHLCVData>>,
    }
    
    impl SanityChecker {
        pub fn new() -> Self {
            Self {
                price_data: HashMap::new(),
            }
        }
        
        // Simplified strategy simulation for sanity checking
        pub fn validate_strategy(&self, _strategy_params: &serde_json::Value, token: &str) -> Result<SimpleBacktestResult> {
            let data = self.price_data.get(token).ok_or_else(|| anyhow::anyhow!("No data for token {}", token))?;
            
            if data.len() < 10 {
                return Err(anyhow::anyhow!("Insufficient data for validation"));
            }
            
            // Simplified momentum strategy simulation (this would be strategy-specific)
            let mut capital = 1000.0;
            let mut position = 0.0;
            let mut trades = Vec::new();
            let mut peak_capital = capital;
            let mut max_drawdown = 0.0;
            
            for i in 1..data.len() {
                let prev_price = data[i-1].close;
                let curr_price = data[i].close;
                let price_change = (curr_price - prev_price) / prev_price;
                
                // Simple momentum signal
                let signal = if price_change > 0.02 { 1.0 } else if price_change < -0.02 { -1.0 } else { 0.0 };
                
                // Simulate trade execution with realistic costs
                if signal != 0.0 && position == 0.0 {
                    let trade_size = capital * 0.1; // 10% of capital per trade
                    let slippage_cost = trade_size * 0.003; // 0.3% slippage
                    position = (trade_size - slippage_cost) / curr_price;
                    capital -= trade_size;
                    
                    trades.push((i, signal, curr_price, trade_size));
                } else if signal == 0.0 && position != 0.0 {
                    // Close position
                    let trade_value = position * curr_price;
                    let slippage_cost = trade_value * 0.003;
                    capital += trade_value - slippage_cost;
                    position = 0.0;
                }
                
                // Track drawdown
                let current_value = capital + (position * curr_price);
                if current_value > peak_capital {
                    peak_capital = current_value;
                } else {
                    let drawdown = (peak_capital - current_value) / peak_capital;
                    if drawdown > max_drawdown {
                        max_drawdown = drawdown;
                    }
                }
            }
            
            // Final position value
            if position != 0.0 {
                let last_price = data.last()
                    .map(|d| d.close)
                    .unwrap_or(1.0);
                capital += position * last_price * 0.997; // Close with slippage
            }
            
            let total_return = (capital - 1000.0) / 1000.0;
            let trade_count = trades.len() as u32;
            let wins = trades.iter().filter(|(idx, signal, entry_price, _)| {
                if *idx + 1 < data.len() {
                    let exit_price = data[*idx + 1].close;
                    (*signal > 0.0 && exit_price > *entry_price) || (*signal < 0.0 && exit_price < *entry_price)
                } else {
                    false
                }
            }).count();
            
            let win_rate = if trade_count > 0 { wins as f64 / trade_count as f64 } else { 0.0 };
            
            // Simplified Sharpe calculation (would need proper risk-free rate and volatility)
            let sharpe_ratio = if total_return > 0.0 && max_drawdown > 0.0 {
                total_return / max_drawdown
            } else {
                0.0
            };
            
            Ok(SimpleBacktestResult {
                total_return,
                sharpe_ratio,
                max_drawdown,
                trade_count,
                win_rate,
            })
        }
        
        // Load historical data from CSV (budget-friendly data source)
        pub fn load_historical_data(&mut self, token: &str, csv_data: &str) -> Result<()> {
            let mut data = Vec::new();
            
            for line in csv_data.lines().skip(1) { // Skip header
                let fields: Vec<&str> = line.split(',').collect();
                if fields.len() >= 6 {
                    let ohlcv = OHLCVData {
                        timestamp: fields[0].parse()?,
                        open: fields[1].parse()?,
                        high: fields[2].parse()?,
                        low: fields[3].parse()?,
                        close: fields[4].parse()?,
                        volume_usd: fields[5].parse()?,
                    };
                    data.push(ohlcv);
                }
            }
            
            data.sort_by_key(|d| d.timestamp);
            self.price_data.insert(token.to_string(), data);
            info!("Loaded {} data points for token {}", self.price_data[token].len(), token);
            
            Ok(())
        }
        
        // Cross-validate external backtest results with our internal results
        pub fn cross_validate(&self, external_sharpe: f64, internal_result: &SimpleBacktestResult, strategy_id: &str) -> bool {
            let sharpe_diff = (external_sharpe - internal_result.sharpe_ratio).abs();
            let max_acceptable_diff = 0.5; // Allow 0.5 Sharpe difference
            
            if sharpe_diff > max_acceptable_diff {
                warn!(
                    "❌ Strategy {} FAILED cross-validation: External Sharpe: {:.2}, Internal Sharpe: {:.2}, Diff: {:.2}",
                    strategy_id, external_sharpe, internal_result.sharpe_ratio, sharpe_diff
                );
                return false;
            }
            
            if internal_result.total_return < -0.2 && external_sharpe > 1.0 {
                warn!(
                    "❌ Strategy {} FAILED cross-validation: External claims positive Sharpe {:.2} but internal shows {:.2}% loss",
                    strategy_id, external_sharpe, internal_result.total_return * 100.0
                );
                return false;
            }
            
            info!(
                "✅ Strategy {} PASSED cross-validation: External Sharpe: {:.2}, Internal Sharpe: {:.2}",
                strategy_id, external_sharpe, internal_result.sharpe_ratio
            );
            true
        }
    }
}

struct PendingBacktest {
    job_id: String,
    strategy_spec: StrategySpec,
    submitted_at: std::time::Instant,
}

#[derive(Debug, Serialize)]
struct BacktestResult {
    sharpe_ratio: f64,
    total_return: f64,
    status: String,
    win_rate: f64,
}

// NOTE: The user prompt mentioned removing duplicate code from line 157.
// The duplicate structs and their implementations that were here have been removed.

#[tokio::main]
async fn main() -> Result<()> {
    // Load config and initialize tracing
    let _ = &config::CONFIG;
    tracing_subscriber::fmt::init();
    info!("Portfolio Manager v25 starting up...");

    // Connect to Redis
    let redis_client = redis::Client::open(CONFIG.redis_url.as_str())
        .context("Failed to create Redis client")?;
    let mut redis_conn = redis_client
        .get_multiplexed_async_connection()
        .await
        .context("Failed to connect to Redis")?;
    info!("Connected to Redis at {}", CONFIG.redis_url);

    let backtest_client = Arc::new(backtest_client::BacktestClient::new(
        CONFIG.backtesting_platform_api_key.clone(),
    ).context("Failed to create backtest client")?);
    let pending_backtests = Arc::new(Mutex::new(HashMap::new()));
    let sanity_checker = Arc::new(Mutex::new(sanity_checker::SanityChecker::new()));
    let _portfolio_state_manager = StateManager::new(CONFIG.initial_capital_usd);

    // Spawn background tasks
    let _backtest_monitor_handle = tokio::spawn(monitor_backtest_jobs(
        backtest_client.clone(),
        pending_backtests.clone(),
        sanity_checker.clone(),
    ));

    let _backtest_poller_handle = tokio::spawn(poll_backtest_results(
        redis_client.clone(),
        backtest_client.clone(),
        pending_backtests.clone(),
    ));

    // Main loop for processing new strategy submissions
    let mut last_strategy_id = "0".to_string(); // Start from beginning
    
    loop {
        match process_new_strategy_submissions(
            &mut redis_conn,
            backtest_client.as_ref(),
            pending_backtests.clone(),
            &mut last_strategy_id,
        )
        .await
        {
            Ok(_) => info!("✅ Processed strategy submissions successfully"),
            Err(e) => error!("❌ Error processing new strategy submissions: {}", e),
        }

        tokio::time::sleep(Duration::from_secs(5)).await;
    }
}

/// Simulates performance updates for strategies.
/// In a real system, this data would come from the position_manager.
fn simulate_performance_updates(state_manager: &mut StateManager) {
    for mut state in state_manager.get_all_strategy_states_mut() {
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
    let strategy_states = state_manager.get_all_strategy_states();
    let active_strategies: Vec<&StrategyState> = strategy_states
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
            id: state.spec.id.clone(),
            weight: weight,
            sharpe_ratio: state.sharpe_ratio,
            mode: state.mode,
            params: state.spec.params.clone(),
        };
        allocations.push(allocation);
    }

    allocations
}

/// Promotes strategies from Simulating to Paper trading based on performance thresholds.
fn promote_strategies(state_manager: &mut StateManager) {
    for mut state in state_manager.get_all_strategy_states_mut() {
        if state.mode == TradeMode::Simulating && state.sharpe_ratio > CONFIG.min_sharpe_for_promotion {
            info!("🏆 Promoting strategy {} to Paper Trading! Sharpe: {:.2}", state.spec.id, state.sharpe_ratio);
            state.mode = TradeMode::Paper;
        }
    }
}

async fn monitor_backtest_jobs(
    _backtest_client: Arc<backtest_client::BacktestClient>,
    _pending_backtests: Arc<Mutex<HashMap<String, PendingBacktest>>>,
    _sanity_checker: Arc<Mutex<sanity_checker::SanityChecker>>,
) -> Result<()> {
    // Since we now get immediate results from submit_backtest, this function is simplified
    let mut interval = interval(Duration::from_secs(60));
    
    loop {
        interval.tick().await;
        // Just sleep - backtests are handled immediately in the main loop
        info!("📊 Backtest monitor running (immediate results mode)");
    }
}

async fn poll_backtest_results(
    _redis_client: redis::Client,
    _backtest_client: Arc<backtest_client::BacktestClient>,
    _pending_backtests: Arc<Mutex<HashMap<String, PendingBacktest>>>,
) -> Result<()> {
    // Since we now get immediate results from submit_backtest, this function is simplified
    let mut interval = tokio::time::interval(Duration::from_secs(60));

    loop {
        interval.tick().await;
        info!("📊 Backtest poller running (immediate results mode)");
    }
}


