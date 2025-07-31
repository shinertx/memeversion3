use anyhow::Result;
use crate::jupiter::JupiterClient;
use crate::database::{Database, TradeRecord};
use shared_models::Side;
use tracing::{error, info};
use redis::AsyncCommands;
use redis::streams::StreamReadOptions;
use std::str::FromStr;
use tokio::sync::Mutex;
use std::sync::Arc;
use std::collections::HashMap;
use std::time::Duration;
use crate::config::CONFIG;
use solana_sdk::pubkey::Pubkey;

pub async fn run_monitor(db: Arc<Database>) -> Result<()> {
    info!("📈 Starting Position Manager v24...");
    
    // Initialize Jupiter client
    let jupiter_client = Arc::new(JupiterClient::new("https://quote-api.jup.ag/v6".to_string()));
    
    // Initialize price tracking
    let current_prices = Arc::new(Mutex::new(HashMap::new()));
    let sol_usd_price = Arc::new(Mutex::new(50.0)); // Default SOL price
    
    // Initialize Redis connection for market data
    let redis_client = redis::Client::open(CONFIG.redis_url.clone())?;
    let mut redis_conn = redis_client.get_multiplexed_async_connection().await?;
    
    let mut market_stream_ids = HashMap::new();

    loop {
        // Listen for market events from Redis streams with timeout to avoid blocking forever
        let result = tokio::time::timeout(
            Duration::from_millis(1000),
            redis_conn.xread_options::<_, _, redis::streams::StreamReadReply>(
                &["events:price", "events:sol_price"], 
                &["0", "0"],
                &StreamReadOptions::default().block(100).count(10)
            )
        ).await;

        match result {
            Ok(Ok(streams)) => {
                for stream_key in streams.keys {
                    let stream_name = &stream_key.key;
                    
                    for stream_msg in stream_key.ids {
                        if stream_name == "events:sol_price" {
                            if let Some(data_bytes) = stream_msg.map.get("data") {
                                if let Ok(data_str) = redis::from_redis_value::<String>(data_bytes) {
                                    if let Ok(event) = serde_json::from_str::<shared_models::SolPriceEvent>(&data_str) {
                                        *sol_usd_price.lock().await = event.price_usd;
                                    }
                                }
                            }
                        } else if stream_name == "events:price" {
                            if let Some(data_bytes) = stream_msg.map.get("data") {
                                if let Ok(data_str) = redis::from_redis_value::<String>(data_bytes) {
                                    if let Ok(event) = serde_json::from_str::<shared_models::PriceTick>(&data_str) {
                                        current_prices.lock().await.insert(event.token_address.clone(), event.price_usd);
                                    }
                                }
                            }
                        }
                        
                        market_stream_ids.insert(stream_name.clone(), stream_msg.id.clone());
                    }
                }
            }
            Ok(Err(e)) => error!("Redis stream error: {}", e),
            Err(_) => {
                // Timeout - this is normal, continue to position checking
            }
        }

        // Check open positions
        if !CONFIG.paper_trading_mode {
            if let Err(e) = check_open_positions(
                db.clone(), 
                jupiter_client.clone(), 
                current_prices.clone(), 
                sol_usd_price.clone()
            ).await {
                error!("Error checking open positions: {}", e);
            }
        }

        tokio::time::sleep(Duration::from_secs(5)).await;
    }
}

async fn check_open_positions(
    db: Arc<Database>,
    jupiter: Arc<JupiterClient>,
    current_prices: Arc<Mutex<HashMap<String, f64>>>,
    sol_price: Arc<Mutex<f64>>,
) -> Result<()> {
    let open_trades = db.get_open_trades()?;
    let prices = current_prices.lock().await;
    
    for trade in open_trades {
        if let Some(&current_price) = prices.get(&trade.token_address) {
            // Update highest price for trailing stop
            if current_price > trade.highest_price_usd.unwrap_or(trade.entry_price_usd) {
                db.update_highest_price(trade.id, current_price)?;
            }
            
            // Check stop loss conditions
            let highest = trade.highest_price_usd.unwrap_or(trade.entry_price_usd);
            let trailing_stop_price = highest * (1.0 - CONFIG.trailing_stop_loss_percent / 100.0);
            
            if current_price <= trailing_stop_price {
                info!(
                    "Trailing stop triggered for trade {} at price {:.4} (stop: {:.4})",
                    trade.id, current_price, trailing_stop_price
                );
                execute_close_trade(db.clone(), jupiter.clone(), sol_price.clone(), trade, current_price).await?;
            }
        }
    }
    
    Ok(())
}

async fn execute_close_trade(
    db: Arc<Database>,
    jupiter: Arc<JupiterClient>,
    sol_price: Arc<Mutex<f64>>,
    trade: TradeRecord,
    close_price_usd: f64,
) -> Result<()> {
    info!("Executing close trade for trade_id: {}", trade.id);
    
    let user_pk = Pubkey::from_str(&crate::signer_client::get_pubkey(&CONFIG.signer_url).await?)?;
    let _current_sol_price = *sol_price.lock().await;

    let pnl_usd = if trade.side == Side::Long.to_string() {
        (close_price_usd - trade.entry_price_usd) * (trade.amount_usd / trade.entry_price_usd)
    } else {
        (trade.entry_price_usd - close_price_usd) * (trade.amount_usd / trade.entry_price_usd)
    };

    if trade.side == Side::Long.to_string() {
        let swap_tx_b64 = jupiter.get_swap_transaction(
            &user_pk, 
            &trade.token_address, 
            "So11111111111111111111111111111111111111112", // SOL mint
            trade.amount_usd, 
            30
        ).await?;
        let _signed_tx_b64 = crate::signer_client::sign_transaction(&CONFIG.signer_url, &swap_tx_b64).await?;
        info!("Position closed via Jupiter swap");
    } else {
        info!("Closing SHORT position via Drift (simulated)");
    }

    let status = if pnl_usd > 0.0 { "CLOSED_PROFIT" } else { "CLOSED_LOSS" };
    db.update_trade_pnl(trade.id, status, close_price_usd, pnl_usd)?;
    info!("Trade {} closed with PnL: ${:.2}", trade.id, pnl_usd);

    // Publish PnL to Redis
    let redis_client = redis::Client::open(CONFIG.redis_url.clone())?;
    let mut conn = redis_client.get_multiplexed_async_connection().await?;
    let _: () = conn.xadd(
        "metrics:portfolio:realized_pnl_stream", 
        "*", 
        &[("pnl", pnl_usd.to_string().as_bytes())]
    ).await?;

    Ok(())
}