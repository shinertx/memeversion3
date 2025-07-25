use shared_models::{StrategySpec, BacktestResult};
use std::collections::HashMap;

pub struct StrategyState {
    pub total_pnl: f64,
    pub trade_count: u64,
}

pub struct StateManager {
    initial_capital: f64,
    realized_pnl: f64,
    strategy_states: HashMap<String, StrategyState>,
    strategy_specs: HashMap<String, StrategySpec>,
}

impl StateManager {
    pub fn new(initial_capital: f64) -> Self {
        Self {
            initial_capital,
            realized_pnl: 0.0,
            strategy_states: HashMap::new(),
            strategy_specs: HashMap::new(),
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
}
