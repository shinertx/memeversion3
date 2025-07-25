mod config;
mod state_manager;
mod backtest_client;

use anyhow::Result;
use redis::AsyncCommands;
use shared_models::{StrategySpec, StrategyAllocation, TradeMode};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::time::{interval, Duration};
use tracing::{debug, error, info, instrument, level_filters::LevelFilter, warn};
use tracing_subscriber::EnvFilter;

use config::CONFIG;
use state_manager::{StrategyState, StrategyStateManager};
use backtest_client::{BacktestClient, BacktestResult};

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

    info!("📊 Starting Portfolio Manager v24...");

    let redis_url = config::CONFIG.redis_url.clone();
    let client = redis::Client::open(redis_url)?;
    let mut conn = client.get_async_connection().await?;
    
    let mut portfolio_state_manager = state_manager::StateManager::new(config::CONFIG.initial_capital_usd);

    let mut strategy_registry_stream_id = HashMap::new();
    strategy_registry_stream_id.insert("strategy_registry_stream".to_string(), "0".to_string());

    let backtest_client = Arc::new(BacktestClient::new(
        CONFIG.backtesting_platform_api_key.clone()
    ));
    
    let pending_backtests: Arc<Mutex<HashMap<String, PendingBacktest>>> = Arc::new(Mutex::new(HashMap::new()));

    // Spawn backtest job monitor
    let backtest_monitor_handle = tokio::spawn({
        let redis_client = client.clone();
        let pending_backtests = pending_backtests.clone();
        async move {
            monitor_backtest_jobs(redis_client, pending_backtests).await
        }
    });

    // Spawn backtest result poller
    let backtest_poller_handle = tokio::spawn({
        let redis_client = client.clone();
        let backtest_client = backtest_client.clone();
        let pending_backtests = pending_backtests.clone();
        async move {
            poll_backtest_results(redis_client, backtest_client, pending_backtests).await
        }
    });

    loop {
        info!("Portfolio Manager loop iteration...");
        
        // Process new strategy specs
        match conn.xread_map(&strategy_registry_stream_id, &[("strategy_registry_stream", ">")]).await {
            Ok(streams) => {
                for (_, messages) in streams {
                    for (id, payload) in messages {
                        if let Some(spec_bytes) = payload.get("spec") {
                            if let Ok(spec) = serde_json::from_slice::<StrategySpec>(spec_bytes) {
                                portfolio_state_manager.add_strategy_spec(spec);
                            }
                        }
                        strategy_registry_stream_id.insert("strategy_registry_stream".to_string(), String::from_utf8_lossy(&id.id).to_string());
                    }
                }
            }
            Err(e) => error!("Error reading from strategy_registry_stream: {}", e),
        }

        // Get current portfolio state
        let current_nav = portfolio_state_manager.get_current_nav();
        let realized_pnl = portfolio_state_manager.get_realized_pnl();
        
        // Dynamic GLOBAL_MAX_POSITION_USD based on NAV
        let dynamic_global_max_pos_usd = (current_nav * 0.1).max(50.0);
        conn.set("config:dynamic:global_max_position_usd", dynamic_global_max_pos_usd).await?;
        conn.set("metrics:portfolio:realized_pnl", realized_pnl).await?;
        info!("Current NAV: ${:.2}, Realized PnL: ${:.2}, Dynamic Max Pos: ${:.2}", current_nav, realized_pnl, dynamic_global_max_pos_usd);

        // Create allocations
        let specs = portfolio_state_manager.get_all_specs();
        if specs.is_empty() {
            warn!("No strategy specs available. Waiting...");
            tokio::time::sleep(Duration::from_secs(30)).await;
            continue;
        }

        let mut allocations: Vec<StrategyAllocation> = Vec::new();
        let total_specs = specs.len() as f64;

        for spec in specs {
            let weight = 1.0 / total_specs; // Equal weight for now
            let mode = TradeMode::Simulating; // Start all strategies in simulation

            allocations.push(StrategyAllocation {
                id: spec.id.clone(),
                weight,
                sharpe_ratio: 0.0, // Will be updated as simulation results come in
                mode,
                params: spec.params.clone(),
            });
        }

        info!("Publishing {} allocations with equal weights.", allocations.len());
        let payload = serde_json::to_string(&allocations)?;
        
        conn.set("active_allocations", &payload).await?; 
        conn.xadd("allocations_channel", "*", &[("allocations", payload.as_bytes())]).await?;

        tokio::time::sleep(Duration::from_secs(60)).await;

        tokio::select! {
            res = backtest_monitor_handle => {
                error!("Backtest monitor task exited: {:?}", res);
            }
            res = backtest_poller_handle => {
                error!("Backtest poller task exited: {:?}", res);
            }
            _ = tokio::signal::ctrl_c() => {
                info!("Received shutdown signal");
            }
        }
    }
}

async fn monitor_backtest_jobs(
    redis_client: redis::Client,
    pending_backtests: Arc<Mutex<HashMap<String, PendingBacktest>>>,
) -> Result<()> {
    let mut conn = redis_client.get_async_connection().await?;
    let mut stream_ids = HashMap::new();
    stream_ids.insert("backtest_jobs_submitted".to_string(), "0".to_string());

    loop {
        match conn.xread_map(&stream_ids, &[("backtest_jobs_submitted", ">")]).await {
            Ok(streams) => {
                for (stream_name, messages) in streams {
                    for (id, payload) in messages {
                        if let (Some(job_id), Some(strategy_id), Some(spec_json)) = (
                            payload.get("job_id").and_then(|v| std::str::from_utf8(v).ok()),
                            payload.get("strategy_id").and_then(|v| std::str::from_utf8(v).ok()),
                            payload.get("spec").and_then(|v| std::str::from_utf8(v).ok()),
                        ) {
                            if let Ok(spec) = serde_json::from_str::<StrategySpec>(spec_json) {
                                let pending = PendingBacktest {
                                    job_id: job_id.to_string(),
                                    strategy_id: strategy_id.to_string(),
                                    spec,
                                    submitted_at: std::time::Instant::now(),
                                };
                                
                                pending_backtests.lock().await.insert(job_id.to_string(), pending);
                                info!("Tracking backtest job {} for strategy {}", job_id, strategy_id);
                            }
                        }
                        stream_ids.insert(stream_name.clone(), String::from_utf8_lossy(&id.id).to_string());
                    }
                }
            }
            Err(e) => error!("Error reading backtest jobs stream: {}", e),
        }
        
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

async fn poll_backtest_results(
    redis_client: redis::Client,
    backtest_client: Arc<BacktestClient>,
    pending_backtests: Arc<Mutex<HashMap<String, PendingBacktest>>>,
) -> Result<()> {
    let mut conn = redis_client.get_async_connection().await?;
    let mut interval = interval(Duration::from_secs(10));

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

#[instrument(skip(conn, state_manager), name = "promote_strategies_task")]
async fn promote_strategies(
    mut conn: redis::aio::Connection,
    state_manager: Arc<StrategyStateManager>,
) -> Result<()> {
    let mut stream_ids = HashMap::new();
    stream_ids.insert("backtest_results".to_string(), "0".to_string());

    loop {
        // Read backtest results from external API
        match conn.xread_map(&stream_ids, &[("backtest_results", ">")]).await {
            Ok(streams) => {
                for (stream_name, messages) in streams {
                    for (id, payload) in messages {
                        if let Some(result_json) = payload.get("result").and_then(|v| std::str::from_utf8(v).ok()) {
                            if let Ok(result) = serde_json::from_str::<BacktestResult>(result_json) {
                                if result.sharpe_ratio > CONFIG.min_sharpe_for_promotion {
                                    info!(
                                        strategy_id = %result.strategy_id,
                                        sharpe_ratio = result.sharpe_ratio,
                                        "Strategy passed promotion threshold"
                                    );
                                    
                                    // Create a basic spec from the result
                                    let spec = StrategySpec {
                                        id: result.strategy_id.clone(),
                                        family: result.metadata.get("family")
                                            .and_then(|v| v.as_str())
                                            .unwrap_or("unknown")
                                            .to_string(),
                                        params: result.metadata.get("params")
                                            .cloned()
                                            .unwrap_or_else(|| serde_json::json!({})),
                                    };
                                    
                                    state_manager.promote_to_paper(&spec).await;
                                }
                            }
                        }
                        stream_ids.insert(stream_name.clone(), String::from_utf8_lossy(&id.id).to_string());
                    }
                }
            }
            Err(e) => error!("Error reading backtest results: {}", e),
        }

        // Also check for new strategy specs from factory
        let mut spec_stream_ids = HashMap::new();
        spec_stream_ids.insert("strategy_specs".to_string(), "$".to_string());
        
        match conn.xread_map(&spec_stream_ids, &[("strategy_specs", ">")]).await {
            Ok(streams) => {
                for (_, messages) in streams {
                    for (_, payload) in messages {
                        if let Some(spec_json) = payload.get("spec").and_then(|v| std::str::from_utf8(v).ok()) {
                            if let Ok(spec) = serde_json::from_str::<StrategySpec>(spec_json) {
                                info!("New strategy spec received from factory: {}", spec.id);
                                state_manager.add_to_simulating(&spec).await;
                            }
                        }
                    }
                }
            }
            Err(e) => error!("Error reading strategy specs: {}", e),
        }

        tokio::time::sleep(Duration::from_secs(CONFIG.strategy_promotion_interval_secs)).await;
    }
}
