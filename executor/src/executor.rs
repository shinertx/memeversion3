use crate::{config::CONFIG, database::Database, jupiter::JupiterClient, signer_client, strategies, jito_client::JitoClient};
use anyhow::{anyhow, Result};
use shared_models::{MarketEvent, StrategyAction, StrategyAllocation, OrderDetails, EventType, Side, TradeMode};
use solana_sdk::pubkey::Pubkey;
use std::{collections::HashMap, str::FromStr, sync::Arc};
use tokio::sync::mpsc::{self, Sender, Receiver};
use tokio::task::JoinHandle;
use tracing::{debug, error, info, instrument, warn};
use redis::AsyncCommands;
use drift_sdk::{Client as DriftClient, types::Network as DriftNet};
use chrono::Utc;

pub struct MasterExecutor {
    db: Arc<Database>,
    active_strategies: HashMap<String, (Sender<MarketEvent>, JoinHandle<()>)>,
    event_router_senders: HashMap<EventType, Vec<Sender<MarketEvent>>>,
    redis_client: redis::Client,
    jupiter_client: Arc<JupiterClient>,
    sol_usd_price: Arc<tokio::sync::Mutex<f64>>,
    portfolio_paused: Arc<tokio::sync::Mutex<bool>>,
    jito_client: Arc<JitoClient>,
    drift_client: Arc<tokio::sync::Mutex<Option<DriftClient>>>,
}

impl MasterExecutor {
    pub async fn new(db: Arc<Database>) -> Result<Self> {
        let jito_client = Arc::new(JitoClient::new(&CONFIG.jito_rpc_url).await?);
        
        Ok(Self {
            db,
            active_strategies: HashMap::new(),
            event_router_senders: HashMap::new(),
            redis_client: redis::Client::open(CONFIG.redis_url.clone())?,
            jupiter_client: Arc::new(JupiterClient::new()),
            sol_usd_price: Arc::new(tokio::sync::Mutex::new(1.0)),
            portfolio_paused: Arc::new(tokio::sync::Mutex::new(false)),
            jito_client,
            drift_client: Arc::new(tokio::sync::Mutex::new(None)),
        })
    }

    pub async fn run(&mut self) -> Result<()> {
        info!("Starting Master Executor run loop.");
        
        let mut conn = self.redis_client.get_async_connection().await?;
        let mut allocation_stream_id = HashMap::new();
        allocation_stream_id.insert("allocations_channel".to_string(), "0".to_string());

        let mut market_stream_ids = HashMap::new();
        market_stream_ids.insert("events:price".to_string(), "0".to_string());
        market_stream_ids.insert("events:social".to_string(), "0".to_string());
        market_stream_ids.insert("events:depth".to_string(), "0".to_string());
        market_stream_ids.insert("events:bridge".to_string(), "0".to_string());
        market_stream_ids.insert("events:funding".to_string(), "0".to_string());
        market_stream_ids.insert("events:sol_price".to_string(), "0".to_string());
        market_stream_ids.insert("events:onchain".to_string(), "0".to_string());
        market_stream_ids.insert("events:twitter_raw".to_string(), "0".to_string());
        market_stream_ids.insert("events:farcaster_raw".to_string(), "0".to_string());

        let mut pubsub_conn = self.redis_client.get_async_connection().await?.into_pubsub();
        pubsub_conn.subscribe("kill_switch_channel").await?;

        loop {
            tokio::select! {
                result = conn.xread_map(&allocation_stream_id, &[("allocations_channel", ">")]).await => {
                    match result {
                        Ok(streams) => {
                            for (_, messages) in streams {
                                for (id, payload) in messages {
                                    if let Some(alloc_bytes) = payload.get("allocations") {
                                        if let Ok(allocations) = serde_json::from_slice::<Vec<StrategyAllocation>>(alloc_bytes) {
                                            self.reconcile_strategies(allocations).await;
                                        }
                                    }
                                    allocation_stream_id.insert("allocations_channel".to_string(), String::from_utf8_lossy(&id.id).to_string());
                                }
                            }
                        }
                        Err(e) => error!("Error reading from allocations_channel stream: {}", e),
                    }
                }
                result = conn.xread_map(&market_stream_ids, &[
                    ("events:price", ">"),
                    ("events:social", ">"),
                    ("events:depth", ">"),
                    ("events:bridge", ">"),
                    ("events:funding", ">"),
                    ("events:sol_price", ">"),
                    ("events:onchain", ">"),
                    ("events:twitter_raw", ">"),
                    ("events:farcaster_raw", ">"),
                ]).await => {
                    match result {
                        Ok(streams) => {
                            for (stream_name, messages) in streams {
                                for (id, payload) in messages {
                                    let event_result = match stream_name.as_str() {
                                        "events:price" => payload.get("event").and_then(|e| serde_json::from_slice::<shared_models::PriceTick>(e).ok()).map(MarketEvent::Price),
                                        "events:social" => payload.get("event").and_then(|e| serde_json::from_slice::<shared_models::SocialMention>(e).ok()).map(MarketEvent::Social),
                                        "events:depth" => payload.get("event").and_then(|e| serde_json::from_slice::<shared_models::DepthEvent>(e).ok()).map(MarketEvent::Depth),
                                        "events:bridge" => payload.get("event").and_then(|e| serde_json::from_slice::<shared_models::BridgeEvent>(e).ok()).map(MarketEvent::Bridge),
                                        "events:funding" => payload.get("event").and_then(|e| serde_json::from_slice::<shared_models::FundingEvent>(e).ok()).map(MarketEvent::Funding),
                                        "events:sol_price" => payload.get("event").and_then(|e| serde_json::from_slice::<shared_models::SolPriceEvent>(e).ok()).map(MarketEvent::SolPrice),
                                        "events:onchain" => payload.get("event").and_then(|e| serde_json::from_slice::<shared_models::OnChainEvent>(e).ok()).map(MarketEvent::OnChain),
                                        "events:twitter_raw" => payload.get("event").and_then(|e| serde_json::from_slice::<shared_models::TwitterRawEvent>(e).ok()).map(MarketEvent::TwitterRaw),
                                        "events:farcaster_raw" => payload.get("event").and_then(|e| serde_json::from_slice::<shared_models::FarcasterRawEvent>(e).ok()).map(MarketEvent::FarcasterRaw),
                                        _ => None,
                                    };

                                    if let Some(event) = event_result {
                                        if let MarketEvent::SolPrice(sol_price_event) = &event {
                                            *self.sol_usd_price.lock().await = sol_price_event.price_usd;
                                            info!("Updated SOL price to: {:.2} USD", sol_price_event.price_usd);
                                        }
                                        self.dispatch_event(event).await;
                                    }
                                    market_stream_ids.insert(stream_name, String::from_utf8_lossy(&id.id).to_string());
                                }
                            }
                        }
                        Err(e) => error!("Error reading from market event streams: {}", e),
                    }
                }
                Some(msg) = pubsub_conn.get_message() => {
                    if let Ok(payload) = msg.get_payload::<String>() {
                        let is_paused = payload == "PAUSE";
                        *self.portfolio_paused.lock().await = is_paused;
                        info!("Portfolio trading status: {}", if is_paused { "PAUSED" } else { "RESUMED" });
                    }
                }
            }
        }
    }

    async fn reconcile_strategies(&mut self, allocations: Vec<StrategyAllocation>) {
        let new_ids: HashMap<String, StrategyAllocation> = allocations.into_iter().map(|a| (a.id.clone(), a)).collect();
        let current_ids: Vec<String> = self.active_strategies.keys().cloned().collect();

        for id in current_ids.iter().filter(|id| !new_ids.contains_key(*id)) {
            if let Some((_, handle)) = self.active_strategies.remove(id) {
                handle.abort();
                info!(strategy = id, "Stopped strategy due to deallocation.");
            }
            for (_, senders) in self.event_router_senders.iter_mut() {
                senders.retain(|s| !s.is_closed());
            }
        }

        for (id, alloc) in new_ids {
            if !self.active_strategies.contains_key(&id) {
                info!(strategy = id, weight = alloc.weight, mode = ?alloc.mode, "Starting new strategy.");
                if let Some(mut strategy_instance) = self.build_strategy(&id) {
                    if let Err(e) = strategy_instance.init(&alloc.params).await {
                        error!(strategy = id, error = %e, "Failed to initialize strategy, skipping.");
                        continue;
                    }

                    let (tx, rx) = mpsc::channel(100);
                    let strategy_id_clone = id.clone();
                    let db_clone = self.db.clone();
                    let jupiter_client_clone = self.jupiter_client.clone();
                    let sol_usd_price_clone = self.sol_usd_price.clone();
                    let portfolio_paused_clone = self.portfolio_paused.clone();
                    let drift_client_clone = self.drift_client.clone();
                    let jito_client_clone = self.jito_client.clone();
                    let redis_client_clone = self.redis_client.clone();
                    let mode = alloc.mode.clone();

                    for sub_type in strategy_instance.subscriptions() {
                        self.event_router_senders.entry(sub_type).or_default().push(tx.clone());
                    }

                    let handle = tokio::spawn(async move {
                        strategy_task(
                            strategy_instance,
                            rx,
                            db_clone,
                            jupiter_client_clone,
                            drift_client_clone,
                            jito_client_clone,
                            sol_usd_price_clone,
                            portfolio_paused_clone,
                            redis_client_clone,
                            strategy_id_clone,
                            mode,
                        ).await;
                    });
                    self.active_strategies.insert(id, (tx, handle));
                } else {
                    warn!(strategy = id, "Strategy constructor not found. Skipping allocation.");
                }
            } else {
                info!(strategy = id, weight = alloc.weight, "Strategy already active, weight updated.");
            }
        }
    }

    async fn dispatch_event(&self, event: MarketEvent) {
        let event_type = event.get_type();
        if let Some(senders) = self.event_router_senders.get(&event_type) {
            for sender in senders {
                if let Err(e) = sender.send(event.clone()).await {
                    error!(event_type = ?event_type, error = %e, "Failed to dispatch event to strategy channel.");
                }
            }
        }
    }

    fn build_strategy(&self, id: &str) -> Option<Box<dyn strategies::Strategy>> {
        for constructor in inventory::iter::<strategies::StrategyConstructor> {
            if id.starts_with(constructor.0) {
                return Some((constructor.1)());
            }
        }
        None
    }
}

#[instrument(skip_all, fields(strategy_id))]
async fn strategy_task(
    mut strategy_instance: Box<dyn strategies::Strategy>,
    mut rx: Receiver<MarketEvent>,
    db: Arc<Database>,
    jupiter_client: Arc<JupiterClient>,
    drift_client: Arc<tokio::sync::Mutex<Option<DriftClient>>>,
    jito_client: Arc<JitoClient>,
    sol_usd_price: Arc<tokio::sync::Mutex<f64>>,
    portfolio_paused: Arc<tokio::sync::Mutex<bool>>,
    redis_client: redis::Client,
    strategy_id: String,
    mode: TradeMode,
) {
    info!("Strategy task started in {:?} mode.", mode);
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
                            drift_client.clone(),
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
    let mut conn = redis_client.get_async_connection().await?;
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
        &[("trade", serde_json::to_vec(&shadow_trade)?)]
    ).await?;
    
    info!(strategy=%strategy_id, token=%details.token_address, simulated_pnl=sim_pnl, "Simulated trade recorded.");
    Ok(())
}

#[instrument(skip_all, fields(strategy_id, token_address = %details.token_address, action = ?details.side))]
async fn execute_trade(
    db: Arc<Database>,
    jupiter: Arc<JupiterClient>,
    drift: Arc<tokio::sync::Mutex<Option<DriftClient>>>,
    jito: Arc<JitoClient>,
    sol_price: Arc<tokio::sync::Mutex<f64>>,
    details: OrderDetails,
    strategy_id: &str,
    is_paper: bool,
) -> Result<()> {
    info!("Attempting {} trade.", if is_paper { "PAPER" } else { "LIVE" });

    let redis_client = redis::Client::open(CONFIG.redis_url.clone())?;
    let mut conn = redis_client.get_async_connection().await?;
    let global_max_pos_usd: f64 = conn.get("config:dynamic:global_max_position_usd").await.unwrap_or(CONFIG.global_max_position_usd);
    let final_size_usd = details.suggested_size_usd.min(global_max_pos_usd);

    let current_sol_usd_price = *sol_price.lock().await;
    if current_sol_usd_price <= 0.0 {
        return Err(anyhow!("SOL/USD price not available or zero. Cannot size trade."));
    }

    let price_quote = jupiter.get_quote(final_size_usd / current_sol_usd_price, &details.token_address).await?;
    let current_token_price_usd = price_quote.price_per_token;

    let trade_id = db.log_trade_attempt(&details, strategy_id, current_token_price_usd)?;
    info!(trade_id, size_usd = final_size_usd, price_usd = current_token_price_usd, "Trade attempt logged.");

    if is_paper {
        info!("🧻 PAPER TRADING MODE: Simulating trade.");
        simulate_fill(&db, trade_id, final_size_usd, matches!(details.side, Side::Short))?;
    } else {
        info!("🔥 LIVE TRADING MODE: Executing real trade.");
        let user_pk = Pubkey::from_str(&signer_client::get_pubkey().await?)?;

        if matches!(details.side, Side::Short) {
            return Err(anyhow!("SHORT trades not yet implemented for live mode"));
        } else {
            let swap_tx_b64 = jupiter.get_swap_transaction(&user_pk, &details.token_address, final_size_usd).await?;
            let signed_tx_b64 = signer_client::sign_transaction(&swap_tx_b64).await?;
            let mut tx = crate::jupiter::deserialize_transaction(&signed_tx_b64)?;

            let bh = jito.get_recent_blockhash().await?;
            tx.message.set_recent_blockhash(bh);
            jito.attach_tip(&mut tx, CONFIG.jito_tip_lamports).await?;

            let sig = jito.send_transaction(&tx).await?;
            info!(signature = %sig, "✅ Spot trade submitted via Jito.");
            db.open_trade(trade_id, &sig.to_string())?;
        }
    }
    
    let fill_event = serde_json::json!({
        "trade_id": trade_id,
        "strategy_id": strategy_id,
        "side": details.side.to_string(),
        "fill_price_usd": current_token_price_usd,
        "size_usd": final_size_usd,
        "timestamp": Utc::now().timestamp(),
        "pnl": 0.0,
    });
    conn.xadd("fills_channel", "*", &[("fill", serde_json::to_vec(&fill_event)?)])?;
    
    Ok(())
}

fn simulate_fill(db: &Database, id: i64, size: f64, short: bool) -> Result<()> {
    let pnl = size * (rand::random::<f64>() * 0.1 - 0.05) * if short { -1.0 } else { 1.0 };
    let status = if pnl > 0.0 { "CLOSED_PROFIT" } else { "CLOSED_LOSS" };
    db.open_trade(id, "paper")?;
    db.update_trade_pnl(id, status, 0.0, pnl)?;
    info!(trade_id = id, status, pnl, "Paper trade finalized.");
    Ok(())
}
