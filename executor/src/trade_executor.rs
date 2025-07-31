use crate::config::Config;
use crate::database::Database;
use crate::jupiter::JupiterClient;
use crate::jito_client::JitoClient;
use crate::signer_client;
use crate::strategy_manager::TradeRequest;
use anyhow::{anyhow, Context, Result};
use base64::{Engine as _, engine::general_purpose};
use redis::AsyncCommands;
use shared_models::{CircuitBreaker, Side};
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tracing::{error, info, warn};

pub struct TradeExecutor {
    config: Arc<Config>,
    db: Arc<Database>,
    redis_client: redis::Client,
    jupiter_client: Arc<JupiterClient>,
    jito_client: Arc<JitoClient>,
    sol_usd_price: Arc<RwLock<f64>>,
}

impl TradeExecutor {
    pub async fn new(
        config: Arc<Config>,
        db: Arc<Database>,
        redis_client: redis::Client,
        sol_usd_price: Arc<RwLock<f64>>,
    ) -> Result<Self> {
        let jupiter_client = Arc::new(JupiterClient::new()
            .context("Failed to create Jupiter client")?);
        let jito_client = Arc::new(JitoClient::new(&config.jito_rpc_url).await?);

        Ok(Self {
            config,
            db,
            redis_client,
            jupiter_client,
            jito_client,
            sol_usd_price,
        })
    }

    pub async fn start(
        &self,
        mut trade_rx: mpsc::Receiver<TradeRequest>,
        circuit_breaker: Arc<CircuitBreaker>,
    ) -> Result<()> {
        info!("💱 Trade executor started");

        while let Some(request) = trade_rx.recv().await {
            let executor = self.clone();
            let cb = circuit_breaker.clone();
            
            tokio::spawn(async move {
                if let Err(e) = executor.execute_trade(request, cb).await {
                    error!("Trade execution failed: {}", e);
                }
            });
        }

        Ok(())
    }

    async fn execute_trade(
        &self,
        request: TradeRequest,
        circuit_breaker: Arc<CircuitBreaker>,
    ) -> Result<()> {
        let TradeRequest { strategy_id, details, mode } = request;

        info!("💰 Executing trade for strategy {}: {:?} ${:.2}", 
              strategy_id, details.side, details.suggested_size_usd);

        // Circuit breaker checks
        match details.side {
            Side::Long => {
                if !circuit_breaker.can_open_new_positions() {
                    warn!("🚫 Circuit breaker blocking LONG for strategy {}", strategy_id);
                    return Ok(());
                }
            }
            Side::Short => {
                if !circuit_breaker.can_close_positions() {
                    error!("🚫 Circuit breaker blocking SHORT for strategy {}", strategy_id);
                    return Ok(());
                }
            }
        }

        // Get dynamic position limit
        let mut conn = self.redis_client.get_multiplexed_async_connection().await?;
        let global_max_pos_usd: f64 = conn.get("config:dynamic:global_max_position_usd")
            .await
            .unwrap_or(self.config.global_max_position_usd);

        // Apply circuit breaker multiplier
        let cb_multiplier = circuit_breaker.get_position_size_multiplier();
        let size_after_cb = details.suggested_size_usd * cb_multiplier;
        let final_size_usd = size_after_cb.min(global_max_pos_usd);

        if cb_multiplier < 1.0 {
            info!("📉 Circuit breaker reduced size by {:.1}% for {}", 
                  (1.0 - cb_multiplier) * 100.0, strategy_id);
        }

        // Get current prices
        let sol_price = *self.sol_usd_price.read().await;
        if sol_price <= 0.0 {
            return Err(anyhow!("SOL price not available"));
        }

        let token_price = self.jupiter_client
            .get_price(&details.token_address)
            .await
            .context("Failed to get token price")?;

        // Log trade attempt
        let trade_id = self.db.log_trade_attempt(&details, &strategy_id, token_price)?;

        // Execute based on mode
        match mode {
            shared_models::TradeMode::Simulating => {
                self.simulate_trade(&strategy_id, &details).await?;
            }
            shared_models::TradeMode::Paper => {
                self.execute_paper_trade(trade_id, final_size_usd, &details).await?;
            }
            shared_models::TradeMode::Live => {
                self.execute_live_trade(
                    trade_id,
                    final_size_usd,
                    sol_price,
                    &details,
                    &strategy_id,
                ).await?;
            }
        }

        Ok(())
    }

    async fn simulate_trade(
        &self,
        strategy_id: &str,
        details: &shared_models::OrderDetails,
    ) -> Result<()> {
        let mut conn = self.redis_client.get_multiplexed_async_connection().await?;
        let sim_pnl = details.suggested_size_usd * (rand::random::<f64>() * 0.1 - 0.05);
        
        let shadow_trade = serde_json::json!({
            "strategy_id": strategy_id,
            "token": details.token_address,
            "side": details.side.to_string(),
            "size_usd": details.suggested_size_usd,
            "confidence": details.confidence,
            "simulated_pnl": sim_pnl,
            "timestamp": chrono::Utc::now().timestamp(),
        });
        
        let _: () = conn.xadd(
            &format!("shadow_ledgers:{}", strategy_id),
            "*",
            &[("trade", serde_json::to_string(&shadow_trade)?)],
        ).await?;
        
        info!("📊 Simulated trade for {}: PnL ${:.2}", strategy_id, sim_pnl);
        Ok(())
    }

    async fn execute_paper_trade(
        &self,
        trade_id: i64,
        size_usd: f64,
        details: &shared_models::OrderDetails,
    ) -> Result<()> {
        info!("📄 Paper trade {}: ${:.2}", trade_id, size_usd);
        
        // Simulate fill with slippage
        let slippage_pct = 0.005;
        let slippage_amount = size_usd * slippage_pct;
        let final_pnl = -slippage_amount;
        
        self.db.open_trade(trade_id, "paper")?;
        self.db.update_trade_pnl(trade_id, "FILLED", 0.0, final_pnl)?;
        
        Ok(())
    }

    async fn execute_live_trade(
        &self,
        trade_id: i64,
        size_usd: f64,
        sol_price: f64,
        details: &shared_models::OrderDetails,
        strategy_id: &str,
    ) -> Result<()> {
        let amount_sol = size_usd / sol_price;
        
        let quote = self.jupiter_client
            .get_quote(amount_sol, &details.token_address)
            .await
            .context("Failed to get quote")?;

        let tx_data = serde_json::to_string(&quote)?;
        let signed_tx = signer_client::sign_transaction(&self.config.signer_url, &tx_data).await?;
        
        let tx_bytes = general_purpose::STANDARD.decode(&signed_tx)?;
        let transaction: solana_sdk::transaction::VersionedTransaction = 
            bincode::deserialize(&tx_bytes)?;
            
        let sig = self.jito_client.send_transaction(&transaction).await?;
        self.db.open_trade(trade_id, &sig.to_string())?;

        info!("🔴 Live trade {} submitted: {}", trade_id, sig);
        Ok(())
    }
}

impl Clone for TradeExecutor {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            db: self.db.clone(),
            redis_client: self.redis_client.clone(),
            jupiter_client: self.jupiter_client.clone(),
            jito_client: self.jito_client.clone(),
            sol_usd_price: self.sol_usd_price.clone(),
        }
    }
}
