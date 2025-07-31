use crate::{
    config::CONFIG,
    database::Database,
    jito_client::JitoClient,
    jupiter::JupiterClient,
    risk_manager::RiskManager,
    signer_client,
    strategies::{self, Strategy, EventType, MarketEvent, StrategyAction, OrderDetails},
};
use anyhow::{anyhow, Context, Result};
use redis::AsyncCommands;
use shared_models::{Side, StrategyAllocation, TradeMode};
use solana_sdk::pubkey::Pubkey;
use std::{collections::HashMap, str::FromStr, sync::Arc};
use tokio::sync::{
    mpsc::{self, Receiver, Sender},
    Mutex,
};
use tokio::task::JoinHandle;
use tracing::{debug, error, info, instrument, warn};

pub struct MasterExecutor {
    db: Arc<Database>,
    active_strategies: HashMap<String, (Sender<MarketEvent>, JoinHandle<()>, Arc<Mutex<StrategyAllocation>>)>,
    event_router_senders: HashMap<EventType, Vec<Sender<MarketEvent>>>,
    redis_client: redis::Client,
    jupiter_client: Arc<JupiterClient>,
    jito_client: Arc<JitoClient>,
    sol_usd_price: Arc<Mutex<f64>>,
    portfolio_paused: Arc<Mutex<bool>>,
}

impl MasterExecutor {
    pub async fn new(db: Arc<Database>) -> Result<Self> {
        let redis_client = redis::Client::open(CONFIG.redis_url.clone())
            .context("Failed to create Redis client")?;
        let jupiter_client = Arc::new(JupiterClient::new());
        let jito_client = Arc::new(JitoClient::new(&CONFIG.jito_rpc_url).await
            .context("Failed to create Jito client")?);

        Ok(Self {
            db,
            active_strategies: HashMap::new(),
            event_router_senders: HashMap::new(),
            redis_client,
            jupiter_client,
            jito_client,
            sol_usd_price: Arc::new(Mutex::new(0.0)),
            portfolio_paused: Arc::new(Mutex::new(false)),
        })
    }

    pub fn paused_flag(&self) -> Arc<Mutex<bool>> {
        self.portfolio_paused.clone()
    }

    pub async fn run(&mut self) -> Result<()> {
        info!("🚀 MasterExecutor started, monitoring Redis streams and allocations...");
        let mut conn = self.redis_client.get_async_connection().await?;

        let mut stream_ids: HashMap<String, String> = [
            ("allocations_channel", "0"),
            ("events:price", "0"),
            ("events:social", "0"),
            ("events:depth", "0"),
            ("events:bridge", "0"),
            ("events:funding", "0"),
            ("events:sol_price", "0"),
        ].iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();

        loop {
            tokio::select! {
                // Handle allocation updates
                alloc_result = conn.xread(&["allocations_channel"], &[stream_ids.get("allocations_channel").unwrap()]) => {
                    match alloc_result {
                        Ok(streams) => {
                            for stream in streams {
                                for (id, data) in stream.ids {
                                    stream_ids.insert("allocations_channel".to_string(), id.clone());
                                    
                                    if let Some(alloc_data) = data.get("allocation") {
                                        match serde_json::from_str::<StrategyAllocation>(alloc_data) {
                                            Ok(allocation) => {
                                                info!(strategy_id = %allocation.strategy_id, "Received allocation update");
                                                self.reconcile_strategies(vec![allocation]).await;
                                            }
                                            Err(e) => error!(error = %e, "Failed to parse allocation")
                                        }
                                    }
                                }
                            }
                        }
                        Err(e) => error!(error = %e, "Failed to read allocations stream")
                    }
                }
                
                // Handle market events
                events_result = conn.xread(&["events:price"], &[stream_ids.get("events:price").unwrap()]) => {
                    match events_result {
                        Ok(streams) => {
                            for stream in streams {
                                for (id, data) in stream.ids {
                                    stream_ids.insert("events:price".to_string(), id.clone());
                                    
                                    if let Some(event_data) = data.get("data") {
                                        match serde_json::from_str::<MarketEvent>(event_data) {
                                            Ok(event) => {
                                                self.dispatch_event(&event).await;
                                            }
                                            Err(e) => error!(error = %e, "Failed to parse market event")
                                        }
                                    }
                                }
                            }
                        }
                        Err(e) => error!(error = %e, "Failed to read events stream")
                    }
                }
            }
        }
    }

    async fn reconcile_strategies(&mut self, allocations: Vec<StrategyAllocation>) {
        for allocation in allocations {
            let strategy_id = allocation.strategy_id.clone();
            
            if !self.active_strategies.contains_key(&strategy_id) {
                info!(strategy_id = %strategy_id, "Starting new strategy");
                
                let strategy_instance = match self.build_strategy(&allocation.strategy_family) {
                    Ok(strategy) => strategy,
                    Err(e) => {
                        error!(error = %e, strategy_id = %strategy_id, "Failed to build strategy");
                        continue;
                    }
                };

                let (tx, rx) = mpsc::channel(1000);
                let allocation_mutex = Arc::new(Mutex::new(allocation));
                
                let handle = tokio::spawn(strategy_task(
                    strategy_instance,
                    rx,
                    self.db.clone(),
                    self.jupiter_client.clone(),
                    self.jito_client.clone(),
                    self.sol_usd_price.clone(),
                    self.portfolio_paused.clone(),
                    allocation_mutex.clone(),
                    strategy_id.clone(),
                ));

                self.active_strategies.insert(strategy_id.clone(), (tx.clone(), handle, allocation_mutex));
                
                // Register for events
                self.event_router_senders
                    .entry(EventType::Price)
                    .or_insert_with(Vec::new)
                    .push(tx);
            }
        }
    }

    async fn dispatch_event(&self, event: &MarketEvent) {
        let event_type = match event {
            MarketEvent::Price(_) => EventType::Price,
            MarketEvent::Social(_) => EventType::Social,
            MarketEvent::Bridge(_) => EventType::Bridge,
            MarketEvent::Depth(_) => EventType::Depth,
            MarketEvent::Funding(_) => EventType::Funding,
            MarketEvent::SolPrice(_) => EventType::SolPrice,
        };

        if let Some(senders) = self.event_router_senders.get(&event_type) {
            for sender in senders {
                if let Err(e) = sender.try_send(event.clone()) {
                    warn!(error = %e, "Failed to send event to strategy");
                }
            }
        }
    }

    fn build_strategy(&self, family: &str) -> Result<Box<dyn Strategy + Send>> {
        strategies::create_strategy(family)
    }
}

// Placeholder strategy for testing
#[derive(Default)]
struct PlaceholderStrategy;

#[async_trait::async_trait]
impl Strategy for PlaceholderStrategy {
    fn id(&self) -> &'static str { "placeholder" }
    fn subscriptions(&self) -> std::collections::HashSet<EventType> { 
        std::iter::once(EventType::Price).collect()
    }
    async fn init(&mut self, _params: &serde_json::Value) -> Result<()> { Ok(()) }
    async fn on_event(&mut self, _event: &MarketEvent) -> Result<StrategyAction> { 
        Ok(StrategyAction::Hold) 
    }
}

#[instrument(skip_all, fields(strategy_id))]
async fn strategy_task(
    mut strategy_instance: Box<dyn Strategy>,
    mut rx: Receiver<MarketEvent>,
    db: Arc<Database>,
    jupiter_client: Arc<JupiterClient>,
    jito_client: Arc<JitoClient>,
    sol_usd_price: Arc<Mutex<f64>>,
    portfolio_paused: Arc<Mutex<bool>>,
    allocation: Arc<Mutex<StrategyAllocation>>,
    strategy_id: String,
) {
    info!("Strategy task started.");
    while let Some(event) = rx.recv().await {
        if *portfolio_paused.lock().await {
            debug!("Portfolio paused. Skipping trade signal.");
            continue;
        }

        match strategy_instance.on_event(&event).await {
            Ok(StrategyAction::Execute(details)) => {
                let alloc = allocation.lock().await;
                if let Err(e) = execute_trade(
                    db.clone(),
                    jupiter_client.clone(),
                    jito_client.clone(),
                    sol_usd_price.clone(),
                    details,
                    &strategy_id,
                    alloc.mode,
                ).await {
                    error!(error = %e, "Trade execution failed.");
                }
            }
            Ok(StrategyAction::Hold) => {}
            Err(e) => {
                error!(error = %e, "Strategy returned an error on event.");
            }
        }
    }
}

#[instrument(skip_all, fields(strategy_id, token_address = %details.token_address, action = ?details.side))]
async fn execute_trade(
    db: Arc<Database>,
    jupiter: Arc<JupiterClient>,
    jito: Arc<JitoClient>,
    sol_price: Arc<Mutex<f64>>,
    details: OrderDetails,
    strategy_id: &str,
    mode: TradeMode,
) -> Result<()> {
    let risk_manager = RiskManager::new();
    let redis_client = redis::Client::open(CONFIG.redis_url.clone())?;
    if let Err(e) = risk_manager.validate_order(&details, &redis_client).await {
        warn!(error = %e, "Pre-trade risk check failed. Order rejected.");
        return Ok(());
    }

    let current_sol_usd_price = *sol_price.lock().await;
    if current_sol_usd_price <= 0.0 {
        return Err(anyhow!("SOL/USD price not available or zero."));
    }
    
    let price_quote = jupiter.get_quote(details.suggested_size_usd / current_sol_usd_price, &details.token_address).await?;
    let trade_id = db.log_trade_attempt(&details, strategy_id, price_quote.price_per_token)?;

    match mode {
        TradeMode::Simulating => {
            simulate_trade(&redis_client, strategy_id, &details, price_quote.price_per_token).await?;
        }
        TradeMode::Paper => {
            info!(trade_id, "PAPER TRADING MODE: Simulating fill.");
            db.open_trade(trade_id, "paper-trade-signature")?;
        }
        TradeMode::Live => {
            info!(trade_id, "🔥 LIVE TRADING MODE: Executing real trade.");
            let user_pk = Pubkey::from_str(&signer_client::get_pubkey().await?)?;
            let swap_tx_b64 = jupiter.get_swap_transaction(&user_pk, &details.token_address, details.suggested_size_usd).await?;
            let signed_tx_b64 = signer_client::sign_transaction(&swap_tx_b64).await?;
            let mut tx = crate::jupiter::deserialize_transaction(&signed_tx_b64)?;

            let bh = jito.get_recent_blockhash().await?;
            tx.message.set_recent_blockhash(bh);
            jito.attach_tip(&mut tx, CONFIG.jito_tip_lamports).await?;
            let sig = jito.send_transaction(&tx).await?;
            db.open_trade(trade_id, &sig.to_string())?;
        }
    }
    Ok(())
}

async fn simulate_trade(
    redis_client: &redis::Client,
    strategy_id: &str,
    details: &OrderDetails,
    price: f64,
) -> Result<()> {
    let mut conn = redis_client.get_async_connection().await?;
    let sim_pnl = details.suggested_size_usd * (rand::random::<f64>() * 0.02 - 0.01); // +/- 1% PnL
    
    let shadow_trade = serde_json::json!({
        "pnl": sim_pnl,
        "price": price,
    });
    
    conn.xadd(format!("shadow_ledger:{}", strategy_id), "*", &[("trade", &serde_json::to_string(&shadow_trade)?)])
        .await?;
    
    debug!(strategy_id, "Simulated trade recorded to shadow ledger.");
    Ok(())
}