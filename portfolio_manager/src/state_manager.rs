use shared_models::{StrategySpec, BacktestResult};
use std::collections::HashMap;

use anyhow::Result;
use shared_models::{StrategySpec, TradeMode};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info};

#[derive(Debug, Clone)]
pub struct StrategyState {
    pub spec: StrategySpec,
    pub mode: TradeMode,
    pub sharpe_ratio: f64,
    pub realized_pnl: f64,
    pub unrealized_pnl: f64,
    pub trade_count: u32,
    pub win_count: u32,
    pub run_time_secs: u64,
    pub last_updated: std::time::Instant,
    pub capital_allocated: f64,
}

impl StrategyState {
    pub fn new(spec: StrategySpec) -> Self {
        // Use the fitness score from the spec as initial Sharpe ratio
        let initial_sharpe = spec.fitness;
        
        Self {
            spec,
            mode: TradeMode::Simulating, // All strategies start in Simulating mode
            sharpe_ratio: initial_sharpe,
            realized_pnl: 0.0,
            unrealized_pnl: 0.0,
            trade_count: 0,
            win_count: 0,
            run_time_secs: 0,
            last_updated: std::time::Instant::now(),
            capital_allocated: 0.0,
        }
    }
    
    pub fn update_performance(&mut self, pnl: f64, is_win: bool) {
        self.realized_pnl += pnl;
        self.trade_count += 1;
        if is_win {
            self.win_count += 1;
        }
        
        // Update runtime
        self.run_time_secs = self.last_updated.elapsed().as_secs();
        
        // Recalculate Sharpe ratio (simplified version)
        if self.trade_count > 0 && self.run_time_secs > 0 {
            let avg_pnl_per_trade = self.realized_pnl / self.trade_count as f64;
            let time_factor = (self.run_time_secs as f64 / 3600.0).max(1.0); // Hours
            self.sharpe_ratio = avg_pnl_per_trade / time_factor;
        }
        
        self.last_updated = std::time::Instant::now();
    }
}

use anyhow::Result;
use shared_models::{StrategySpec, TradeMode};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info};

#[derive(Debug, Clone)]
pub struct StrategyState {
    pub spec: StrategySpec,
    pub mode: TradeMode,
    pub sharpe_ratio: f64,
    pub realized_pnl: f64,
    pub unrealized_pnl: f64,
    pub trade_count: u32,
    pub win_count: u32,
    pub run_time_secs: u64,
    pub last_updated: std::time::Instant,
    pub capital_allocated: f64,
}

impl StrategyState {
    pub fn new(spec: StrategySpec) -> Self {
        // Use the fitness score from the spec as initial Sharpe ratio
        let initial_sharpe = spec.fitness;
        
        Self {
            spec,
            mode: TradeMode::Simulating, // All strategies start in Simulating mode
            sharpe_ratio: initial_sharpe,
            realized_pnl: 0.0,
            unrealized_pnl: 0.0,
            trade_count: 0,
            win_count: 0,
            run_time_secs: 0,
            last_updated: std::time::Instant::now(),
            capital_allocated: 0.0,
        }
    }
    
    pub fn update_performance(&mut self, pnl: f64, is_win: bool) {
        self.realized_pnl += pnl;
        self.trade_count += 1;
        if is_win {
            self.win_count += 1;
        }
        
        // Update runtime
        self.run_time_secs = self.last_updated.elapsed().as_secs();
        
        // Recalculate Sharpe ratio (simplified version)
        if self.trade_count > 0 && self.run_time_secs > 0 {
            let avg_pnl_per_trade = self.realized_pnl / self.trade_count as f64;
            let time_factor = (self.run_time_secs as f64 / 3600.0).max(1.0); // Hours
            self.sharpe_ratio = avg_pnl_per_trade / time_factor;
        }
        
        self.last_updated = std::time::Instant::now();
    }
}

pub struct StateManager {
    strategies: Arc<RwLock<HashMap<String, StrategyState>>>,
    specs: Arc<RwLock<HashMap<String, StrategySpec>>>,
    stream_ids: Arc<RwLock<HashMap<String, String>>>,
    total_capital: f64,
    initial_capital: f64,
}

impl StateManager {
    pub fn new(initial_capital: f64) -> Self {
        Self {
            strategies: Arc::new(RwLock::new(HashMap::new())),
            specs: Arc::new(RwLock::new(HashMap::new())),
            stream_ids: Arc::new(RwLock::new(HashMap::new())),
            total_capital: initial_capital,
            initial_capital,
        }
    }
    
    pub async fn add_strategy_spec(&self, spec: StrategySpec) {
        let strategy_id = spec.id.clone();
        
        // Add to specs collection
        self.specs.write().await.insert(strategy_id.clone(), spec.clone());
        
        // Create initial strategy state
        let state = StrategyState::new(spec);
        self.strategies.write().await.insert(strategy_id.clone(), state);
        
        info!("Added new strategy: {} with initial Sharpe: {:.2}", strategy_id, spec.fitness);
    }
    
    pub async fn get_all_specs(&self) -> Vec<StrategySpec> {
        self.specs.read().await.values().cloned().collect()
    }
    
    pub async fn get_all_strategy_states(&self) -> Vec<StrategyState> {
        self.strategies.read().await.values().cloned().collect()
    }
    
    pub async fn get_all_strategy_states_mut(&self) -> Vec<StrategyState> {
        // For the simulation, we return a copy that can be modified
        self.strategies.read().await.values().cloned().collect()
    }
    
    pub async fn update_strategy_state<F>(&self, strategy_id: &str, update_fn: F) -> Result<()>
    where
        F: FnOnce(&mut StrategyState),
    {
        let mut strategies = self.strategies.write().await;
        if let Some(state) = strategies.get_mut(strategy_id) {
            update_fn(state);
            debug!("Updated state for strategy {}", strategy_id);
        }
        Ok(())
    }
    
    pub async fn promote_strategy(&self, strategy_id: &str, new_mode: TradeMode) -> Result<()> {
        self.update_strategy_state(strategy_id, |state| {
            state.mode = new_mode;
            info!("Promoted strategy {} to {:?} mode", strategy_id, new_mode);
        }).await
    }
    
    pub fn get_total_capital(&self) -> f64 {
        self.total_capital
    }
    
    pub fn get_current_nav(&self) -> f64 {
        // NAV = initial capital + all realized PnL
        // In a real system, this would include unrealized PnL
        self.total_capital
    }
    
    pub fn get_realized_pnl(&self) -> f64 {
        self.total_capital - self.initial_capital
    }
    
    pub async fn update_total_capital(&mut self, pnl: f64) {
        self.total_capital += pnl;
    }
    
    pub async fn get_last_stream_id(&self, stream_key: &str) -> Option<String> {
        self.stream_ids.read().await.get(stream_key).cloned()
    }
    
    pub async fn set_last_stream_id(&self, stream_key: &str, id: String) {
        self.stream_ids.write().await.insert(stream_key.to_string(), id);
    }
}

    pub fn get_current_nav(&self) -> f64 {
        self.initial_capital + self.realized_pnl
    }

    pub fn get_realized_pnl(&self) -> f64 {
        self.realized_pnl
    }

    pub fn add_realized_pnl(&mut self, pnl: f64) {
        self.realized_pnl += pnl;
    }

    pub fn add_strategy_spec(&mut self, spec: StrategySpec) {
        self.strategy_specs.insert(spec.id.clone(), spec);
    }

    pub fn get_all_specs(&self) -> Vec<&StrategySpec> {
        self.strategy_specs.values().collect()
    }

    pub fn process_backtest_result(&mut self, result: BacktestResult) {
        let state = self.strategy_states.entry(result.spec_id.clone()).or_insert_with(|| StrategyState {
            total_pnl: 0.0,
            trade_count: 0,
        });
        state.total_pnl = result.total_pnl;
        state.trade_count = result.trade_count as u64;
    }

    pub fn get_strategy_trade_count(&self, strategy_id: &str) -> u64 {
        self.strategy_states.get(strategy_id).map_or(0, |s| s.trade_count)
    }

    pub async fn promote_to_paper(&self, spec: &StrategySpec) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Implementation for promoting strategy from simulation to paper trading
        println!("Promoting strategy {} to paper trading mode", spec.id);
        Ok(())
    }

    pub async fn add_to_simulating(&self, spec: &StrategySpec) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Implementation for adding strategy to simulation mode
        println!("Adding strategy {} to simulation mode", spec.id);
        Ok(())
    }
}
