use crate::error::Result;
use crate::jupiter_client::JupiterClient;
use crate::models::{TradeRecord, Side};
use crate::database::Database;
use log::{error, info};
use serde_json::json;
use std::str::FromStr;
use tokio::sync::Mutex;
use std::sync::Arc;
use std::collections::HashMap;
use std::time::Duration;
use crate::config::CONFIG;
use solana_sdk::pubkey::Pubkey;

pub async fn run_monitor(db: Arc<Database>) -> Result<()> {
    info!("📈 Starting Position Manager v24...");
    
    let mut market_stream_ids = HashMap::new();

    loop {
        // Fetch and process market events
        match jupiter_client.get_market_events().await {
            Ok(events) => {
                for event_bytes in events {
                    if let Ok(event) = serde_json::from_slice::<shared_models::SolPriceEvent>(event_bytes) {
                        *sol_usd_price.lock().await = event.price_usd;
                    }
                }
                market_stream_ids.insert(stream_name, String::from_utf8_lossy(&id.id).to_string());
            }
            Err(e) => error!("Error reading from market event stream: {}", e),
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
    
    let user_pk = Pubkey::from_str(&signer_client::get_pubkey(&CONFIG.signer_url).await?)?;
    let current_sol_price = *sol_price.lock().await;

    let pnl_usd = if trade.side == Side::Long.to_string() {
        (close_price_usd - trade.entry_price_usd) * (trade.amount_usd / trade.entry_price_usd)
    } else {
        (trade.entry_price_usd - close_price_usd) * (trade.amount_usd / trade.entry_price_usd)
    };

    if trade.side == Side::Long.to_string() {
        let swap_tx_b64 = jupiter.get_swap_transaction(
            &user_pk, 
            &trade.token_address, 
            "So11111111111111111111111111111111111111112", 
            trade.amount_usd, 
            30
        ).await?;
        let signed_tx_b64 = signer_client::sign_transaction(&CONFIG.signer_url, &swap_tx_b64).await?;
        info!("Position closed via Jupiter swap");
    } else {
        info!("Closing SHORT position via Drift (simulated)");
    }

    let status = if pnl_usd > 0.0 { "CLOSED_PROFIT" } else { "CLOSED_LOSS" };
    db.update_trade_pnl(trade.id, status, close_price_usd, pnl_usd)?;
    info!("Trade {} closed with PnL: ${:.2}", trade.id, pnl_usd);

    // Publish PnL to Redis
    let redis_client = redis::Client::open(CONFIG.redis_url.clone())?;
    let mut conn = redis_client.get_async_connection().await?;
    conn.xadd(
        "metrics:portfolio:realized_pnl_stream", 
        "*", 
        &[("pnl", pnl_usd.to_string().as_bytes())]
    ).await?;

    Ok(())
}