use crate::config::CONFIG;
use crate::database::Database;
use crate::jito_client::JitoClient;
use crate::jupiter::JupiterClient;
use crate::signer_client;
use crate::strategies::{self, create_strategy, Strategy};
use anyhow::{anyhow, Context, Result};
use base64::{Engine as _, engine::general_purpose};
use redis::AsyncCommands;
use shared_models::{
    MarketEvent, OrderDetails, PriceTick, Side, SolPriceEvent, SocialMention, StrategyAction,
    StrategyAllocation, TradeMode, EventType,
};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc::{self, Receiver, Sender};
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tracing::{debug, error, info, instrument, warn};

// Commented out for initial deployment - will re-enable for live trading
// use drift_sdk::{Client as DriftClient, types::Network as DriftNet};
use chrono::Utc;

pub struct MasterExecutor {
    strategies: HashMap<String, StrategyInfo>,
    strategy_senders: HashMap<EventType, Vec<Sender<MarketEvent>>>,
    db: Arc<Database>,
    redis_client: redis::Client,
    jupiter_client: Arc<JupiterClient>,
    jito_client: Arc<JitoClient>,
    sol_usd_price: Arc<Mutex<f64>>,
    portfolio_paused: Arc<Mutex<bool>>,
}

struct StrategyInfo {
    handle: JoinHandle<()>,
    subscriptions: HashSet<EventType>,
    mode: TradeMode,
}

impl MasterExecutor {
    pub async fn new(db: Arc<Database>) -> Result<Self> {
        let redis_client = redis::Client::open(CONFIG.redis_url.clone())?;
        let jupiter_client = Arc::new(JupiterClient::new());
        let jito_client = Arc::new(JitoClient::new(&CONFIG.jito_rpc_url).await?);

        Ok(Self {
            strategies: HashMap::new(),
            strategy_senders: HashMap::new(),
            db,
            redis_client,
            jupiter_client,
            jito_client,
            sol_usd_price: Arc::new(Mutex::new(0.0)),
            portfolio_paused: Arc::new(Mutex::new(false)),
        })
    }

    pub async fn run(&mut self) -> Result<()> {
        info!("Starting Master Executor run loop.");

        let mut conn = self.redis_client.get_multiplexed_async_connection().await?;

        // Start with empty stream IDs to read all messages
        let mut stream_ids = HashMap::new();
        stream_ids.insert("allocations_channel".to_string(), "0".to_string());
        stream_ids.insert("events:price".to_string(), "0".to_string());
        stream_ids.insert("events:social".to_string(), "0".to_string());
        stream_ids.insert("events:depth".to_string(), "0".to_string());
        stream_ids.insert("events:bridge".to_string(), "0".to_string());
        stream_ids.insert("events:funding".to_string(), "0".to_string());
        stream_ids.insert("events:sol_price".to_string(), "0".to_string());
        stream_ids.insert("events:onchain".to_string(), "0".to_string());

        loop {
            // Read from all streams
            let keys: Vec<&str> = stream_ids.keys().map(|k| k.as_str()).collect();
            let ids: Vec<&str> = stream_ids.values().map(|v| v.as_str()).collect();

            let result: redis::RedisResult<redis::streams::StreamReadReply> = conn
                .xread_options(
                    &keys,
                    &ids,
                    &redis::streams::StreamReadOptions::default()
                        .block(1000)
                        .count(100),
                )
                .await;

            if let Ok(reply) = result {
                for stream_key in reply.keys {
                    let stream_name = stream_key.key.clone();

                    for message in stream_key.ids {
                        // Update last seen ID
                        stream_ids.insert(stream_name.clone(), message.id.clone());

                        if stream_name == "allocations_channel" {
                            if let Some(allocations_value) = message.map.get("allocations") {
                                if let Ok(allocations_str) = redis::from_redis_value::<String>(allocations_value) {
                                    if let Ok(allocations) =
                                        serde_json::from_str::<Vec<StrategyAllocation>>(
                                            &allocations_str,
                                        )
                                    {
                                        self.reconcile_strategies(allocations).await;
                                    }
                                }
                            }
                        } else if stream_name.starts_with("events:") {
                            if let Some(event_value) = message.map.get("data") {
                                if let Ok(event_str) = redis::from_redis_value::<String>(event_value) {
                                if let Ok(event_data) =
                                    serde_json::from_str::<serde_json::Value>(&event_str)
                                {
                                    // Parse specific event types
                                    let event = match stream_name.as_str() {
                                        "events:price" => {
                                            if let Ok(tick) =
                                                serde_json::from_value::<PriceTick>(event_data)
                                            {
                                                Some(MarketEvent::Price(tick))
                                            } else {
                                                None
                                            }
                                        }
                                        "events:social" => {
                                            if let Ok(mention) =
                                                serde_json::from_value::<SocialMention>(event_data)
                                            {
                                                Some(MarketEvent::Social(mention))
                                            } else {
                                                None
                                            }
                                        }
                                        "events:sol_price" => {
                                            if let Ok(sol_event) =
                                                serde_json::from_value::<SolPriceEvent>(event_data)
                                            {
                                                *self.sol_usd_price.lock().await =
                                                    sol_event.price_usd;
                                                Some(MarketEvent::SolPrice(sol_event))
                                            } else {
                                                None
                                            }
                                        }
                                        // Add other event types...
                                        _ => None,
                                    };

                                    if let Some(event) = event {
                                        self.dispatch_event(&event).await;
                                    }
                                }
                            }
                        }
                    }
                    }
                }
            }

            // Small delay to prevent tight loop
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    async fn reconcile_strategies(&mut self, allocations: Vec<StrategyAllocation>) {
        let new_ids: HashMap<String, StrategyAllocation> = allocations.into_iter().map(|a| (a.id.clone(), a)).collect();
        let current_ids: Vec<String> = self.strategies.keys().cloned().collect();

        // Stop and remove deallocated strategies
        for id in current_ids.iter().filter(|id| !new_ids.contains_key(*id)) {
            if let Some(strategy_info) = self.strategies.remove(id) {
                strategy_info.handle.abort();
                // Also remove its senders from the dispatch map
                for event_type in strategy_info.subscriptions {
                    if let Some(senders) = self.strategy_senders.get_mut(&event_type) {
                        senders.retain(|s| !s.is_closed());
                    }
                }
                info!(strategy = id, "Stopped strategy due to deallocation.");
            }
        }

        // Start new strategies
        for (id, alloc) in new_ids {
            if !self.strategies.contains_key(&id) {
                info!(strategy = %id, weight = alloc.weight, mode = ?alloc.mode, "Starting new strategy.");
                if let Ok(mut strategy_instance) = create_strategy(&id) {
                    // Initialize strategy with parameters
                    if let Err(e) = strategy_instance.init(&alloc.params).await {
                        error!("Failed to initialize strategy {}: {}", id, e);
                        continue;
                    }
                    let (tx, rx) = mpsc::channel(100);
                    let strategy_id_clone = id.clone();
                    let jupiter_client_clone = self.jupiter_client.clone();
                    let jito_client_clone = self.jito_client.clone();
                    let sol_usd_price_clone = self.sol_usd_price.clone();
                    let portfolio_paused_clone = self.portfolio_paused.clone();
                    let redis_client_clone = self.redis_client.clone();
                    let mode = alloc.mode;
                    let subscriptions = strategy_instance.subscriptions();

                    let db_clone = self.db.clone();
                    let handle = tokio::spawn(async move {
                        strategy_task(
                            strategy_instance,
                            rx,
                            db_clone,
                            jupiter_client_clone,
                            jito_client_clone,
                            sol_usd_price_clone,
                            portfolio_paused_clone,
                            redis_client_clone,
                            strategy_id_clone,
                            mode,
                        ).await;
                    });

                    self.strategies.insert(
                        id.clone(),
                        StrategyInfo {
                            handle,
                            subscriptions: subscriptions.clone(),
                            mode: alloc.mode,
                        },
                    );
                    
                    for event_type in subscriptions {
                        self.strategy_senders.entry(event_type).or_default().push(tx.clone());
                    }

                } else {
                    warn!(strategy = id, "Strategy constructor not found. Skipping allocation.");
                }
            } else {
                // Here you could update the mode or other parameters if needed
                info!(strategy = id, weight = alloc.weight, "Strategy already active, weight updated.");
            }
        }
    }

    async fn dispatch_event(&self, event: &MarketEvent) {
        let event_type = event.get_type();
        if let Some(senders) = self.strategy_senders.get(&event_type) {
            for sender in senders {
                if let Err(e) = sender.send(event.clone()).await {
                    error!(?event_type, error = %e, "Failed to dispatch event to strategy channel.");
                }
            }
        }
    }
}

#[instrument(skip_all, fields(strategy_id))]
async fn strategy_task(
    mut strategy_instance: Box<dyn strategies::Strategy>,
    mut rx: Receiver<MarketEvent>,
    db: Arc<Database>,
    jupiter_client: Arc<JupiterClient>,
    jito_client: Arc<JitoClient>,
    sol_usd_price: Arc<tokio::sync::Mutex<f64>>,
    portfolio_paused: Arc<tokio::sync::Mutex<bool>>,
    redis_client: redis::Client,
    strategy_id: String,
    mode: TradeMode,
) {
    info!(
        "Strategy task started for instance: {}",
        strategy_instance.id()
    );
    while let Some(event) = rx.recv().await {
        if *portfolio_paused.lock().await {
            debug!("Portfolio paused. Skipping trade signal for {}.", strategy_id);
            continue;
        }

        match strategy_instance.on_event(&event).await {
            Ok(StrategyAction::Execute(details)) => {
                match mode {
                    TradeMode::Simulating => {
                        if let Err(e) = simulate_trade(redis_client.clone(), &strategy_id, &details).await {
                            error!(strategy=%strategy_id, error=%e, "Simulation failed.");
                        }
                    }
                    TradeMode::Paper | TradeMode::Live => {
                        if let Err(e) = execute_trade(
                            db.clone(),
                            jupiter_client.clone(),
                            jito_client.clone(),
                            sol_usd_price.clone(),
                            details,
                            &strategy_id,
                            matches!(mode, TradeMode::Paper),
                        ).await { 
                            error!(strategy=%strategy_id, error=%e, "Trade execution failed."); 
                        }
                    }
                }
            }
            Ok(StrategyAction::Hold) => {}
            Err(e) => {
                error!(strategy=%strategy_id, error=%e, "Strategy returned an error on event.");
            }
        }
    }
    info!("Strategy task finished.");
}

async fn simulate_trade(
    redis_client: redis::Client,
    strategy_id: &str,
    details: &OrderDetails,
) -> Result<()> {
    let mut conn = redis_client.get_multiplexed_async_connection().await?;
    let sim_pnl = details.suggested_size_usd * (rand::random::<f64>() * 0.1 - 0.05);
    
    let shadow_trade = serde_json::json!({
        "strategy_id": strategy_id,
        "token": details.token_address,
        "side": details.side.to_string(),
        "size_usd": details.suggested_size_usd,
        "confidence": details.confidence,
        "simulated_pnl": sim_pnl,
        "timestamp": Utc::now().timestamp(),
    });
    
    conn.xadd(
        &format!("shadow_ledgers:{}", strategy_id),
        "*",
        &[("trade", &serde_json::to_vec(&shadow_trade)?)],
    ).await?;
    
    info!(strategy=%strategy_id, token=%details.token_address, simulated_pnl=sim_pnl, "Simulated trade recorded.");
    Ok(())
}

#[instrument(skip_all, fields(strategy_id, token_address = %details.token_address, action = ?details.side))]
async fn execute_trade(
    db: Arc<Database>,
    jupiter: Arc<JupiterClient>,
    jito: Arc<JitoClient>,
    sol_price: Arc<tokio::sync::Mutex<f64>>,
    details: OrderDetails,
    strategy_id: &str,
    is_paper: bool,
) -> Result<()> {
    info!(
        "Executing trade for strategy {}: {:?}, Paper: {}",
        strategy_id, details, is_paper
    );

    let redis_client = redis::Client::open(CONFIG.redis_url.clone())?;
    let mut conn = redis_client.get_multiplexed_async_connection().await?;
    let global_max_pos_usd: f64 = conn.get("config:dynamic:global_max_position_usd").await.unwrap_or(CONFIG.global_max_position_usd);
    let final_size_usd = details.suggested_size_usd.min(global_max_pos_usd);

    let current_sol_usd_price = *sol_price.lock().await;
    if current_sol_usd_price <= 0.0 {
        return Err(anyhow!("SOL/USD price not available or zero. Cannot size trade."));
    }

    let current_token_price_usd = jupiter
        .get_price(&details.token_address)
        .await
        .context("Failed to get current token price from Jupiter")?;

    let trade_id = db.log_trade_attempt(&details, strategy_id, current_token_price_usd)?;

    if is_paper {
        info!(
            "PAPER TRADE: Simulating fill for trade_id: {}, size_usd: {}",
            trade_id, final_size_usd
        );
        simulate_fill(
            &db,
            trade_id,
            final_size_usd,
            matches!(details.side, Side::Short),
        )?;
        return Ok(());
    }

    // Live trading logic
    let amount_sol = final_size_usd / current_sol_usd_price;
    let quote_response = jupiter
        .get_quote(amount_sol, &details.token_address)
        .await
        .context("Failed to get quote from Jupiter")?;

    // Convert quote to transaction string for signing
    let tx_data = serde_json::to_string(&quote_response)
        .context("Failed to serialize quote response")?;
    
    let signed_tx = signer_client::sign_transaction(&tx_data).await?;
    
    // Parse the signed transaction for Jito submission
    let tx_bytes = general_purpose::STANDARD.decode(&signed_tx)?;
    let transaction: solana_sdk::transaction::VersionedTransaction = 
        bincode::deserialize(&tx_bytes)
            .context("Failed to deserialize signed transaction")?;
            
    let sig = jito.send_transaction(&transaction).await?;

    db.open_trade(trade_id, &sig.to_string())?;

    info!(
        "Successfully submitted live trade {} for strategy {}. Signature: {}",
        trade_id, strategy_id, sig
    );
    
    Ok(())
}

fn simulate_fill(db: &Arc<Database>, id: i64, size: f64, short: bool) -> Result<()> {
    // Simulate a fill with some slippage
    let slippage_percent = 0.005; // 0.5%
    let slippage_amount = size * slippage_percent;
    let final_pnl = if short {
        -slippage_amount
    } else {
        -slippage_amount
    };
    let status = "FILLED";
    db.open_trade(id, "paper")?;
    db.update_trade_pnl(id, status, 0.0, final_pnl)?;
    info!(
        "Simulated fill for trade {}, final PnL after slippage: {}",
        id, final_pnl
    );
    Ok(())
}
