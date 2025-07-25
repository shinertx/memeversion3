use lazy_static::lazy_static;
use std::env;

pub struct Config {
    pub paper_trading_mode: bool,
    pub wallet_keypair_filename: String,
    pub jito_auth_keypair_filename: String,
    pub solana_rpc_url: String,
    pub jito_rpc_url: String,
    pub signer_url: String,
    pub initial_capital_usd: f64,
    pub global_max_position_usd: f64,
    pub portfolio_stop_loss_percent: f64,
    pub trailing_stop_loss_percent: f64,
    pub jupiter_api_url: String,
    pub slippage_bps: u16,
    pub jito_tip_lamports: u64,
    pub database_path: String,
    pub historical_data_path: String,
    pub redis_url: String,
    pub helius_api_key: String,
    pub pyth_api_key: String,
    pub twitter_bearer_token: String,
    pub drift_api_url: String,
    pub farcaster_api_url: String,
}

impl Config {
    fn load() -> Self {
        Self {
            paper_trading_mode: env::var("PAPER_TRADING_MODE").unwrap_or_else(|_| "true".to_string()) == "true",
            wallet_keypair_filename: env::var("WALLET_KEYPAIR_FILENAME").expect("WALLET_KEYPAIR_FILENAME must be set"),
            jito_auth_keypair_filename: env::var("JITO_AUTH_KEYPAIR_FILENAME").expect("JITO_AUTH_KEYPAIR_FILENAME must be set"),
            solana_rpc_url: env::var("SOLANA_RPC_URL").expect("SOLANA_RPC_URL must be set"),
            jito_rpc_url: env::var("JITO_RPC_URL").expect("JITO_RPC_URL must be set"),
            signer_url: env::var("SIGNER_URL").expect("SIGNER_URL must be set"),
            initial_capital_usd: env::var("INITIAL_CAPITAL_USD").expect("INITIAL_CAPITAL_USD must be set").parse().unwrap(),
            global_max_position_usd: env::var("GLOBAL_MAX_POSITION_USD").expect("GLOBAL_MAX_POSITION_USD must be set").parse().unwrap(),
            portfolio_stop_loss_percent: env::var("PORTFOLIO_STOP_LOSS_PERCENT").expect("PORTFOLIO_STOP_LOSS_PERCENT must be set").parse().unwrap(),
            trailing_stop_loss_percent: env::var("TRAILING_STOP_LOSS_PERCENT").expect("TRAILING_STOP_LOSS_PERCENT must be set").parse().unwrap(),
            jupiter_api_url: env::var("JUPITER_API_URL").expect("JUPITER_API_URL must be set"),
            slippage_bps: env::var("SLIPPAGE_BPS").expect("SLIPPAGE_BPS must be set").parse().unwrap(),
            jito_tip_lamports: env::var("JITO_TIP_LAMPORTS").expect("JITO_TIP_LAMPORTS must be set").parse().unwrap(),
            database_path: env::var("DATABASE_PATH").expect("DATABASE_PATH must be set"),
            historical_data_path: env::var("HISTORICAL_DATA_PATH").expect("HISTORICAL_DATA_PATH must be set"),
            redis_url: env::var("REDIS_URL").expect("REDIS_URL must be set"),
            helius_api_key: env::var("HELIUS_API_KEY").expect("HELIUS_API_KEY must be set"),
            pyth_api_key: env::var("PYTH_API_KEY").expect("PYTH_API_KEY must be set"),
            twitter_bearer_token: env::var("TWITTER_BEARER_TOKEN").expect("TWITTER_BEARER_TOKEN must be set"),
            drift_api_url: env::var("DRIFT_API_URL").expect("DRIFT_API_URL must be set"),
            farcaster_api_url: env::var("FARCASATER_API_URL").expect("FARCASATER_API_URL must be set"),
        }
    }
}

lazy_static! {
    pub static ref CONFIG: Config = Config::load();
}
