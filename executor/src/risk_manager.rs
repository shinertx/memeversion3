use crate::config::Config;
use crate::database::Database;
use anyhow::Result;
use shared_models::CircuitBreaker;
use std::sync::Arc;
use std::time::Duration;
use tokio::task::JoinHandle;
use tracing::{debug, error, info};

pub struct RiskManager {
    config: Arc<Config>,
    db: Arc<Database>,
    circuit_breaker: Arc<CircuitBreaker>,
}

impl RiskManager {
    pub fn new(config: Arc<Config>, db: Arc<Database>) -> Self {
        Self {
            config,
            db,
            circuit_breaker: Arc::new(CircuitBreaker::new()),
        }
    }

    pub fn get_circuit_breaker(&self) -> Arc<CircuitBreaker> {
        self.circuit_breaker.clone()
    }

    pub fn start_monitoring(&self) -> JoinHandle<()> {
        let config = self.config.clone();
        let db = self.db.clone();
        let circuit_breaker = self.circuit_breaker.clone();

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(30));
            
            info!("🛡️  Risk manager started, monitoring portfolio health every 30s");
            
            loop {
                interval.tick().await;
                
                if let Err(e) = Self::check_portfolio_health(
                    &config,
                    &db,
                    &circuit_breaker,
                ).await {
                    error!("Portfolio health check failed: {}", e);
                }
            }
        })
    }

    async fn check_portfolio_health(
        config: &Config,
        db: &Database,
        circuit_breaker: &CircuitBreaker,
    ) -> Result<()> {
        let total_pnl = db.get_total_realized_pnl()?;
        let initial_capital = config.initial_capital_usd;
        
        let current_nav = initial_capital + total_pnl;
        let max_nav = db.get_max_nav(initial_capital).unwrap_or(initial_capital);
        
        let drawdown_pct = if max_nav > 0.0 {
            ((max_nav - current_nav) / max_nav) * 100.0
        } else {
            0.0
        };
        
        let risk_level = circuit_breaker.update_drawdown(drawdown_pct);
        
        debug!(
            "📊 Portfolio health: NAV=${:.2}, Drawdown={:.2}%, Risk={:?}",
            current_nav, drawdown_pct, risk_level
        );
        
        Ok(())
    }
}
