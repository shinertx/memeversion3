use lazy_static::lazy_static;
use std::env;

pub struct Config {
    pub database_path: String,
    pub redis_url: String,
    pub signer_url: String,
    pub jupiter_api_url: String,
    pub paper_trading_mode: bool,
    pub trailing_stop_loss_percent: f64,
}

impl Config {
    fn load() -> Self {
        Self {
            database_path: env::var("DATABASE_PATH").expect("DATABASE_PATH must be set"),
            redis_url: env::var("REDIS_URL").expect("REDIS_URL must be set"),
            signer_url: env::var("SIGNER_URL").expect("SIGNER_URL must be set"),
            jupiter_api_url: env::var("JUPITER_API_URL").expect("JUPITER_API_URL must be set"),
            paper_trading_mode: env::var("PAPER_TRADING_MODE")
                .unwrap_or_else(|_| "true".to_string())
                == "true",
            trailing_stop_loss_percent: env::var("TRAILING_STOP_LOSS_PERCENT")
                .unwrap_or_else(|_| "15.0".to_string())
                .parse()
                .unwrap(),
        }
    }
}

lazy_static! {
    pub static ref CONFIG: Config = Config::load();
}
