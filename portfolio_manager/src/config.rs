use lazy_static::lazy_static;
use std::env;

pub struct Config {
    pub redis_url: String,
    pub initial_capital_usd: f64,
    pub backtesting_platform_api_key: String,
}

impl Config {
    pub fn from_env() -> Self {
        Self {
            redis_url: env::var("REDIS_URL").expect("REDIS_URL must be set"),
            initial_capital_usd: env::var("INITIAL_CAPITAL_USD").expect("INITIAL_CAPITAL_USD must be set").parse().unwrap(),
            backtesting_platform_api_key: env::var("BACKTESTING_PLATFORM_API_KEY")
                .expect("BACKTESTING_PLATFORM_API_KEY must be set"),
        }
    }
}

lazy_static! {
    pub static ref CONFIG: Config = Config::from_env();
}
