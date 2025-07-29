use lazy_static::lazy_static;
use std::env;

pub struct Config {
    pub redis_url: String,
    pub initial_capital_usd: f64,
    pub backtesting_platform_api_key: String,
    pub min_sharpe_for_promotion: f64,
    pub strategy_promotion_interval_secs: u64,
    pub rebalance_interval_secs: u64,
}

impl Config {
    pub fn from_env() -> Self {
        Self {
            redis_url: env::var("REDIS_URL").expect("REDIS_URL must be set"),
            initial_capital_usd: env::var("INITIAL_CAPITAL_USD")
                .unwrap_or_else(|_| "10000.0".to_string()).parse().unwrap(),
            backtesting_platform_api_key: env::var("BACKTESTING_PLATFORM_API_KEY")
                .unwrap_or_else(|_| "demo_key_simulation_only".to_string()),
            min_sharpe_for_promotion: env::var("MIN_SHARPE_FOR_PROMOTION")
                .unwrap_or_else(|_| "1.5".to_string()).parse().unwrap(),
            strategy_promotion_interval_secs: env::var("STRATEGY_PROMOTION_INTERVAL_SECS")
                .unwrap_or_else(|_| "300".to_string()).parse().unwrap(),
            rebalance_interval_secs: env::var("REBALANCE_INTERVAL_SECS")
                .unwrap_or_else(|_| "300".to_string()).parse().unwrap(),
        }
    }
}

lazy_static! {
    pub static ref CONFIG: Config = Config::from_env();
}
