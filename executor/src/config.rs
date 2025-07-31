use std::env;
use anyhow::{Context, Result};

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
    pub fn load() -> Result<Self> {
        println!("Loading configuration...");
        
        let config = Self {
            paper_trading_mode: env::var("PAPER_TRADING_MODE").unwrap_or_else(|_| "true".to_string()) == "true",
            wallet_keypair_filename: env::var("WALLET_KEYPAIR_FILENAME").context("WALLET_KEYPAIR_FILENAME must be set")?,
            jito_auth_keypair_filename: env::var("JITO_AUTH_KEYPAIR_FILENAME").context("JITO_AUTH_KEYPAIR_FILENAME must be set")?,
            solana_rpc_url: env::var("SOLANA_RPC_URL").context("SOLANA_RPC_URL must be set")?,
            jito_rpc_url: env::var("JITO_RPC_URL").context("JITO_RPC_URL must be set")?,
            signer_url: env::var("SIGNER_URL").context("SIGNER_URL must be set")?,
            initial_capital_usd: env::var("INITIAL_CAPITAL_USD")
                .context("INITIAL_CAPITAL_USD must be set")?
                .parse()
                .context("INITIAL_CAPITAL_USD must be a valid number")?,
            global_max_position_usd: env::var("GLOBAL_MAX_POSITION_USD")
                .context("GLOBAL_MAX_POSITION_USD must be set")?
                .parse()
                .context("GLOBAL_MAX_POSITION_USD must be a valid number")?,
            portfolio_stop_loss_percent: env::var("PORTFOLIO_STOP_LOSS_PERCENT")
                .context("PORTFOLIO_STOP_LOSS_PERCENT must be set")?
                .parse()
                .context("PORTFOLIO_STOP_LOSS_PERCENT must be a valid number")?,
            trailing_stop_loss_percent: env::var("TRAILING_STOP_LOSS_PERCENT")
                .context("TRAILING_STOP_LOSS_PERCENT must be set")?
                .parse()
                .context("TRAILING_STOP_LOSS_PERCENT must be a valid number")?,
            jupiter_api_url: env::var("JUPITER_API_URL").context("JUPITER_API_URL must be set")?,
            slippage_bps: env::var("SLIPPAGE_BPS")
                .context("SLIPPAGE_BPS must be set")?
                .parse()
                .context("SLIPPAGE_BPS must be a valid number")?,
            jito_tip_lamports: env::var("JITO_TIP_LAMPORTS")
                .context("JITO_TIP_LAMPORTS must be set")?
                .parse()
                .context("JITO_TIP_LAMPORTS must be a valid number")?,
            database_path: env::var("DATABASE_PATH").context("DATABASE_PATH must be set")?,
            historical_data_path: env::var("HISTORICAL_DATA_PATH").context("HISTORICAL_DATA_PATH must be set")?,
            redis_url: env::var("REDIS_URL").context("REDIS_URL must be set")?,
            helius_api_key: env::var("HELIUS_API_KEY").unwrap_or_else(|_| "demo_key".to_string()),
            pyth_api_key: env::var("PYTH_API_KEY").unwrap_or_else(|_| "demo_key".to_string()),
            twitter_bearer_token: env::var("TWITTER_BEARER_TOKEN").unwrap_or_else(|_| "demo_token".to_string()),
            drift_api_url: env::var("DRIFT_API_URL").unwrap_or_else(|_| "https://api.drift.trade".to_string()),
            farcaster_api_url: env::var("FARCASTER_API_URL").unwrap_or_else(|_| "https://api.neynar.com/v2".to_string()),
        };
        
        println!("Configuration loaded successfully");
        println!("Paper trading mode: {}", config.paper_trading_mode);
        println!("Redis URL: {}", config.redis_url);
        println!("Signer URL: {}", config.signer_url);
        
        Ok(config)
    }
}
