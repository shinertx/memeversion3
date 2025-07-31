use lazy_static::lazy_static;
use std::env;
use anyhow::{Context, Result};

pub struct Config {
    pub redis_url: String,
    pub initial_capital_usd: f64,
    pub backtesting_platform_api_key: String,
    pub min_sharpe_for_promotion: f64,
    pub strategy_promotion_interval_secs: u64,
    pub rebalance_interval_secs: u64,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        Ok(Self {
            redis_url: env::var("REDIS_URL")
                .context("REDIS_URL environment variable must be set")?,
            initial_capital_usd: env::var("INITIAL_CAPITAL_USD")
                .unwrap_or_else(|_| "10000.0".to_string())
                .parse()
                .context("INITIAL_CAPITAL_USD must be a valid number")?,
            backtesting_platform_api_key: env::var("BACKTESTING_PLATFORM_API_KEY")
                .unwrap_or_else(|_| "demo_key_simulation_only".to_string()),
            min_sharpe_for_promotion: env::var("MIN_SHARPE_FOR_PROMOTION")
                .unwrap_or_else(|_| "1.5".to_string())
                .parse()
                .context("MIN_SHARPE_FOR_PROMOTION must be a valid number")?,
            strategy_promotion_interval_secs: env::var("STRATEGY_PROMOTION_INTERVAL_SECS")
                .unwrap_or_else(|_| "300".to_string())
                .parse()
                .context("STRATEGY_PROMOTION_INTERVAL_SECS must be a valid number")?,
            rebalance_interval_secs: env::var("REBALANCE_INTERVAL_SECS")
                .unwrap_or_else(|_| "300".to_string())
                .parse()
                .context("REBALANCE_INTERVAL_SECS must be a valid number")?,
        })
    }
}

lazy_static! {
    pub static ref CONFIG: Config = Config::from_env()
        .expect("Failed to load configuration - check environment variables");
}
