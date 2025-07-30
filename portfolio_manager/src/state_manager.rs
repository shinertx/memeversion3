use shared_models::{StrategySpec, TradeMode};
use std::collections::HashMap;
use anyhow::Result;
use tracing::{info, debug};

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
    pub strategies: HashMap<String, StrategyState>,
    total_capital: f64,
    initial_capital: f64,
    realized_pnl: f64,
}

impl StateManager {
    pub fn new(initial_capital: f64) -> Self {
        Self {
            strategies: HashMap::new(),
            total_capital: initial_capital,
            initial_capital,
            realized_pnl: 0.0,
        }
    }
    
    pub fn add_strategy_spec(&mut self, spec: StrategySpec) {
        let strategy_id = spec.id.clone();
        let initial_fitness = spec.fitness;
        
        // Create initial strategy state
        let state = StrategyState::new(spec);
        self.strategies.insert(strategy_id.clone(), state);
        
        info!("Added new strategy: {} with initial Sharpe: {:.2}", strategy_id, initial_fitness);
    }
    
    pub fn get_all_specs(&self) -> Vec<StrategySpec> {
        self.strategies.values().map(|s| s.spec.clone()).collect()
    }
    
    pub fn get_all_strategy_states(&self) -> Vec<StrategyState> {
        self.strategies.values().cloned().collect()
    }
    
    pub fn get_all_strategy_states_mut(&mut self) -> Vec<StrategyState> {
        // For the simulation, we return a copy that can be modified
        self.strategies.values().cloned().collect()
    }
    
    pub fn update_strategy_state<F>(&mut self, strategy_id: &str, update_fn: F) -> Result<()>
    where
        F: FnOnce(&mut StrategyState),
    {
        if let Some(state) = self.strategies.get_mut(strategy_id) {
            update_fn(state);
            debug!("Updated state for strategy {}", strategy_id);
        }
        Ok(())
    }
    
    pub fn promote_strategy(&mut self, strategy_id: &str, new_mode: TradeMode) -> Result<()> {
        self.update_strategy_state(strategy_id, |state| {
            state.mode = new_mode;
            info!("Promoted strategy {} to {:?} mode", strategy_id, new_mode);
        })?;
        Ok(())
    }
    
    pub fn get_total_capital(&self) -> f64 {
        self.total_capital
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
