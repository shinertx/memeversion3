use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CircuitState {
    Closed,      // Normal operation
    HalfOpen,    // Testing recovery
    Open,        // Emergency stop
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RiskLevel {
    Normal,      // < 5% drawdown
    Warning,     // 5-10% drawdown  
    Critical,    // 10-15% drawdown
    Emergency,   // > 15% drawdown
}

#[derive(Debug)]
pub struct CircuitBreaker {
    state: Arc<AtomicU64>, // Stores CircuitState as u64
    risk_level: Arc<AtomicU64>, // Stores RiskLevel as u64
    last_state_change: Arc<AtomicU64>, // Unix timestamp
    trading_halted: Arc<AtomicBool>,
    position_size_multiplier: Arc<AtomicU64>, // Scaled by 1000 (e.g., 500 = 0.5x)
}

impl Default for CircuitBreaker {
    fn default() -> Self {
        Self::new()
    }
}

impl CircuitBreaker {
    pub fn new() -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        Self {
            state: Arc::new(AtomicU64::new(CircuitState::Closed as u64)),
            risk_level: Arc::new(AtomicU64::new(RiskLevel::Normal as u64)),
            last_state_change: Arc::new(AtomicU64::new(now)),
            trading_halted: Arc::new(AtomicBool::new(false)),
            position_size_multiplier: Arc::new(AtomicU64::new(1000)), // 1.0x
        }
    }

    pub fn update_drawdown(&self, current_drawdown_pct: f64) -> RiskLevel {
        let new_risk_level = match current_drawdown_pct {
            x if x < 5.0 => RiskLevel::Normal,
            x if x < 10.0 => RiskLevel::Warning,
            x if x < 15.0 => RiskLevel::Critical,
            _ => RiskLevel::Emergency,
        };

        let old_risk_level = self.get_risk_level();
        
        if new_risk_level != old_risk_level {
            self.risk_level.store(new_risk_level as u64, Ordering::SeqCst);
            self.handle_risk_level_change(old_risk_level, new_risk_level, current_drawdown_pct);
        }

        new_risk_level
    }

    fn handle_risk_level_change(&self, _old: RiskLevel, new: RiskLevel, _drawdown: f64) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        
        self.last_state_change.store(now, Ordering::SeqCst);

        match new {
            RiskLevel::Normal => {
                // Full trading resumed
                self.position_size_multiplier.store(1000, Ordering::SeqCst); // 1.0x
                self.trading_halted.store(false, Ordering::SeqCst);
                self.state.store(CircuitState::Closed as u64, Ordering::SeqCst);
            }
            RiskLevel::Warning => {
                // Reduce position sizes by 50%
                self.position_size_multiplier.store(500, Ordering::SeqCst); // 0.5x
                self.trading_halted.store(false, Ordering::SeqCst);
                self.state.store(CircuitState::HalfOpen as u64, Ordering::SeqCst);
            }
            RiskLevel::Critical => {
                // Close only mode
                self.position_size_multiplier.store(0, Ordering::SeqCst); // 0x - close only
                self.trading_halted.store(false, Ordering::SeqCst); // Allow closing
                self.state.store(CircuitState::Open as u64, Ordering::SeqCst);
            }
            RiskLevel::Emergency => {
                // All trading stopped
                self.position_size_multiplier.store(0, Ordering::SeqCst);
                self.trading_halted.store(true, Ordering::SeqCst);
                self.state.store(CircuitState::Open as u64, Ordering::SeqCst);
            }
        }
    }

    pub fn get_state(&self) -> CircuitState {
        match self.state.load(Ordering::SeqCst) {
            0 => CircuitState::Closed,
            1 => CircuitState::HalfOpen,
            2 => CircuitState::Open,
            _ => CircuitState::Open, // Default to safe state
        }
    }

    pub fn get_risk_level(&self) -> RiskLevel {
        match self.risk_level.load(Ordering::SeqCst) {
            0 => RiskLevel::Normal,
            1 => RiskLevel::Warning,
            2 => RiskLevel::Critical,
            3 => RiskLevel::Emergency,
            _ => RiskLevel::Emergency, // Default to safe level
        }
    }

    pub fn is_trading_allowed(&self) -> bool {
        !self.trading_halted.load(Ordering::SeqCst)
    }

    pub fn get_position_size_multiplier(&self) -> f64 {
        self.position_size_multiplier.load(Ordering::SeqCst) as f64 / 1000.0
    }

    pub fn can_open_new_positions(&self) -> bool {
        self.get_position_size_multiplier() > 0.0 && self.is_trading_allowed()
    }

    pub fn can_close_positions(&self) -> bool {
        // Always allow closing unless in emergency halt
        self.get_risk_level() != RiskLevel::Emergency || !self.trading_halted.load(Ordering::SeqCst)
    }

    /// Force reset to normal state (admin override)
    pub fn reset(&self) {
        // Manual reset to normal state
        self.state.store(CircuitState::Closed as u64, Ordering::SeqCst);
        self.risk_level.store(RiskLevel::Normal as u64, Ordering::SeqCst);
        self.trading_halted.store(false, Ordering::SeqCst);
        self.position_size_multiplier.store(1000, Ordering::SeqCst);
        
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        self.last_state_change.store(now, Ordering::SeqCst);
    }

    /// Get time since last state change in seconds
    pub fn time_since_last_change(&self) -> u64 {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let last_change = self.last_state_change.load(Ordering::SeqCst);
        now.saturating_sub(last_change)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_circuit_breaker_states() {
        let cb = CircuitBreaker::new();
        
        // Initial state
        assert_eq!(cb.get_risk_level(), RiskLevel::Normal);
        assert_eq!(cb.get_state(), CircuitState::Closed);
        assert!(cb.is_trading_allowed());
        assert_eq!(cb.get_position_size_multiplier(), 1.0);

        // Warning level
        cb.update_drawdown(7.5);
        assert_eq!(cb.get_risk_level(), RiskLevel::Warning);
        assert_eq!(cb.get_state(), CircuitState::HalfOpen);
        assert!(cb.is_trading_allowed());
        assert_eq!(cb.get_position_size_multiplier(), 0.5);

        // Critical level
        cb.update_drawdown(12.0);
        assert_eq!(cb.get_risk_level(), RiskLevel::Critical);
        assert_eq!(cb.get_state(), CircuitState::Open);
        assert!(cb.is_trading_allowed()); // Can still close
        assert_eq!(cb.get_position_size_multiplier(), 0.0);

        // Emergency level
        cb.update_drawdown(20.0);
        assert_eq!(cb.get_risk_level(), RiskLevel::Emergency);
        assert_eq!(cb.get_state(), CircuitState::Open);
        assert!(!cb.is_trading_allowed()); // Complete halt
        assert_eq!(cb.get_position_size_multiplier(), 0.0);

        // Recovery
        cb.update_drawdown(2.0);
        assert_eq!(cb.get_risk_level(), RiskLevel::Normal);
        assert_eq!(cb.get_state(), CircuitState::Closed);
        assert!(cb.is_trading_allowed());
        assert_eq!(cb.get_position_size_multiplier(), 1.0);
    }
}
