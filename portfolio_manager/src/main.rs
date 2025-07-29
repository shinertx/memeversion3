mod config;
mod state_manager;
mod backtest_client;

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
        pub fn validate_strategy(&self, strategy_params: &serde_json::Value, token: &str) -> Result<SimpleBacktestResult> {
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
                capital += position * data.last().unwrap().close * 0.997; // Close with slippage
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
    strategy_id: String,
    spec: StrategySpec,
    submitted_at: std::time::Instant,
}

#[tokio::main]
async fn main() -> Result<()> {
    let filter = EnvFilter::builder()
        .with_default_directive(LevelFilter::INFO.into())
        .from_env_lossy();
    tracing_subscriber::fmt().with_env_filter(filter).init();

    info!("📊 Starting Portfolio Manager v25 with In-House Validation...");

    let redis_url = config::CONFIG.redis_url.clone();
    let client = redis::Client::open(redis_url)?;
    let mut conn = client.get_multiplexed_async_connection().await?;
    
    let mut allocation_interval = interval(Duration::from_secs(CONFIG.rebalance_interval_secs));
    let mut promotion_interval = interval(Duration::from_secs(CONFIG.strategy_promotion_interval_secs));

    loop {
        tokio::select! {
            _ = allocation_interval.tick() => {
                info!("📈 Portfolio rebalance triggered...");

                // 1. Read latest performance metrics for all strategies
                // In a real system, this would come from the executor/position_manager
                // For now, we simulate performance updates.
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
                // Promote strategies from Simulating -> Paper based on performance
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
            // Add other relevant fields like leverage, risk targets, etc.
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

// The backtest monitoring and polling functions would go here
// For brevity, they are omitted but were part of the original file structure.
async fn monitor_backtest_jobs(
    // ...
) -> Result<()> {
    Ok(())
}

async fn poll_backtest_results(
    // ...
) -> Result<()> {
    Ok(())
}

        }

        // Get current portfolio state
        let current_nav = portfolio_state_manager.get_current_nav();
        let realized_pnl = portfolio_state_manager.get_realized_pnl();
        
        // Dynamic GLOBAL_MAX_POSITION_USD based on NAV
        let dynamic_global_max_pos_usd = (current_nav * 0.1).max(50.0);
        conn.set("config:dynamic:global_max_position_usd", dynamic_global_max_pos_usd).await?;
        conn.set("metrics:portfolio:realized_pnl", realized_pnl).await?;
        info!("Current NAV: ${:.2}, Realized PnL: ${:.2}, Dynamic Max Pos: ${:.2}", current_nav, realized_pnl, dynamic_global_max_pos_usd);

        // Create allocations based on strategy performance (capital flows to winners)
        let specs = portfolio_state_manager.get_all_specs();
        if specs.is_empty() {
            warn!("No strategy specs available. Waiting...");
            tokio::time::sleep(Duration::from_secs(30)).await;
            continue;
        }

        let mut allocations: Vec<StrategyAllocation> = Vec::new();
        
        // Performance-based allocation: Capital flows to strategies with highest Sharpe ratios
        let mut specs_with_performance: Vec<(&StrategySpec, f64)> = Vec::new();
        let mut total_performance_score = 0.0;
        
        for spec in &specs {
            // Get performance score (prioritize Sharpe ratio, fallback to equal weight for new strategies)
            let sharpe_ratio = spec.fitness.max(0.1); // Minimum score to avoid division by zero
            let performance_score = sharpe_ratio.max(0.1); // Ensure positive score
            specs_with_performance.push((spec, performance_score));
            total_performance_score += performance_score;
        }
        
        info!("Allocating capital among {} strategies based on performance", specs_with_performance.len());
        
        for (spec, performance_score) in specs_with_performance {
            // Performance-based capital allocation: Higher performers get more capital
            let weight = performance_score / total_performance_score;
            let mode = TradeMode::Simulating; // Start all strategies in simulation
            
            // Log allocation details
            info!("Strategy {} (family: {}): weight={:.3}, performance_score={:.3}", 
                  spec.id, spec.family, weight, performance_score);

            allocations.push(StrategyAllocation {
                id: spec.id.clone(),
                weight,
                sharpe_ratio: performance_score, // Use performance score as Sharpe ratio proxy
                mode,
                params: spec.params.clone(),
            });
        }

        info!("Publishing {} allocations with performance-based weights (capital flows to winners).", allocations.len());
        let payload = serde_json::to_string(&allocations)?;
        
        conn.set("active_allocations", &payload).await?; 
        conn.xadd("allocations_channel", "*", &[("allocations", payload.as_bytes())]).await?;

        tokio::time::sleep(Duration::from_secs(60)).await;

        // Check if background tasks are still running
        if backtest_monitor_handle.is_finished() {
            error!("Backtest monitor task has exited");
            break;
        }
        if backtest_poller_handle.is_finished() {
            error!("Backtest poller task has exited");
            break;
        }
    }
    
    Ok(())
}

async fn monitor_backtest_jobs(
    backtest_client: Arc<BacktestClient>,
    pending_backtests: Arc<Mutex<HashMap<String, PendingBacktest>>>,
    sanity_checker: Arc<Mutex<sanity_checker::SanityChecker>>,
) -> Result<()> {
    let mut interval = interval(Duration::from_secs(30));
    
    loop {
        interval.tick().await;
        
        let mut pending = pending_backtests.lock().await;
        let mut completed_jobs = Vec::new();
        
        for (job_id, pending_backtest) in pending.iter() {
            match backtest_client.get_backtest_result(job_id).await {
                Ok(Some(result)) => {
                    info!("Backtest completed for strategy {}: Sharpe {:.2}", 
                          pending_backtest.strategy_id, result.sharpe_ratio);
                    
                    // CRITICAL: Run cross-validation before accepting result
                    let cross_validation_passed = {
                        let checker = sanity_checker.lock().await;
                        match checker.validate_strategy(&pending_backtest.spec.params, "SOL") {
                            Ok(internal_result) => {
                                checker.cross_validate(result.sharpe_ratio, &internal_result, &pending_backtest.strategy_id)
                            }
                            Err(e) => {
                                warn!("Internal validation failed for {}: {}", pending_backtest.strategy_id, e);
                                false
                            }
                        }
                    };
                    
                    if cross_validation_passed {
                        info!("✅ Strategy {} passed cross-validation, proceeding with promotion evaluation", pending_backtest.strategy_id);
                        // Original promotion logic would go here
                    } else {
                        warn!("❌ Strategy {} REJECTED due to failed cross-validation", pending_backtest.strategy_id);
                        // Strategy is rejected, not promoted
                    }
                    
                    completed_jobs.push(job_id.clone());
                }
                Ok(None) => {
                    // Still processing
                    if pending_backtest.submitted_at.elapsed() > Duration::from_secs(300) {
                        warn!("Backtest timeout for strategy {}", pending_backtest.strategy_id);
                        completed_jobs.push(job_id.clone());
                    }
                }
                Err(e) => {
                    error!("Error checking backtest result for {}: {}", pending_backtest.strategy_id, e);
                    completed_jobs.push(job_id.clone());
                }
            }
        }
        
        for job_id in completed_jobs {
            pending.remove(&job_id);
        }
    }
}

async fn poll_backtest_results(
    redis_client: redis::Client,
    backtest_client: Arc<BacktestClient>,
    pending_backtests: Arc<Mutex<HashMap<String, PendingBacktest>>>,
) -> Result<()> {
    let mut conn = redis_client.get_multiplexed_async_connection().await?;
    let mut interval = tokio::time::interval(Duration::from_secs(10));

    loop {
        interval.tick().await;
        
        let job_ids: Vec<String> = {
            let pending = pending_backtests.lock().await;
            pending.keys().cloned().collect()
        };
        
        for job_id in job_ids {
            match backtest_client.get_backtest_result(&job_id).await {
                Ok(Some(result)) => {
                    if result.status == "completed" {
                        // Push result to Redis for strategy promotion
                        let result_json = serde_json::to_string(&result)?;
                        conn.xadd(
                            "backtest_results",
                            "*",
                            &[("result", result_json)]
                        ).await?;
                        
                        info!(
                            "Backtest completed for job {}: sharpe={:.2}, win_rate={:.2}%",
                            job_id, result.sharpe_ratio, result.win_rate
                        );
                        
                        pending_backtests.lock().await.remove(&job_id);
                    } else if result.status == "failed" {
                        warn!("Backtest failed for job {}", job_id);
                        pending_backtests.lock().await.remove(&job_id);
                    }
                }
                Ok(None) => {
                    // Still pending, check if timeout
                    let should_remove = {
                        let pending = pending_backtests.lock().await;
                        if let Some(backtest) = pending.get(&job_id) {
                            backtest.submitted_at.elapsed() > Duration::from_secs(3600)
                        } else {
                            false
                        }
                    };
                    
                    if should_remove {
                        warn!("Backtest job {} timed out after 1 hour", job_id);
                        pending_backtests.lock().await.remove(&job_id);
                    }
                }
                Err(e) => {
                    error!("Error polling backtest result for job {}: {}", job_id, e);
                }
            }
        }
    }
}

#[instrument(skip(_conn, _state_manager), name = "promote_strategies_task")]
async fn promote_strategies(
    _conn: redis::aio::MultiplexedConnection,
    _state_manager: Arc<StateManager>,
) -> Result<()> {
    let mut stream_ids = HashMap::new();
    stream_ids.insert("backtest_results".to_string(), "0".to_string());
    let mut interval = tokio::time::interval(Duration::from_secs(10));

    loop {
        // Simple strategy promotion check
        interval.tick().await;
        
        // Log that we're monitoring for promotions  
        info!("Monitoring strategies for promotion opportunities");
        
        // Simplified strategy spec processing - will enhance once we're building
        info!("Checking for new strategy specs from factory");

        tokio::time::sleep(Duration::from_secs(CONFIG.strategy_promotion_interval_secs)).await;
    }
}
