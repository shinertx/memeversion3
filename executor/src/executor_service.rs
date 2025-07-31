use crate::config::Config;
use crate::database::Database;
use crate::event_processor::EventProcessor;
use crate::strategy_manager::StrategyManager;
use crate::trade_executor::TradeExecutor;
use crate::risk_manager::RiskManager;
use anyhow::Result;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, error};

pub struct ExecutorService {
    config: Arc<Config>,
    event_processor: EventProcessor,
    strategy_manager: StrategyManager,
    trade_executor: TradeExecutor,
    risk_manager: RiskManager,
    sol_usd_price: Arc<RwLock<f64>>,
}

impl ExecutorService {
    pub async fn new(config: Arc<Config>, db: Arc<Database>) -> Result<Self> {
        info!("Initializing executor service components...");

        let redis_client = redis::Client::open(config.redis_url.clone())?;
        let sol_usd_price = Arc::new(RwLock::new(0.0));

        let event_processor = EventProcessor::new(
            redis_client.clone(),
            sol_usd_price.clone(),
        );

        let strategy_manager = StrategyManager::new();

        let trade_executor = TradeExecutor::new(
            config.clone(),
            db.clone(),
            redis_client.clone(),
            sol_usd_price.clone(),
        ).await?;

        let risk_manager = RiskManager::new(
            config.clone(),
            db.clone(),
        );

        Ok(Self {
            config,
            event_processor,
            strategy_manager,
            trade_executor,
            risk_manager,
            sol_usd_price,
        })
    }

    pub async fn run(mut self) -> Result<()> {
        info!("🚀 Executor service starting main event loop...");

        // Start risk monitoring in background
        let risk_handle = self.risk_manager.start_monitoring();

        // Start event processing
        info!("Starting event processor...");
        let event_rx = self.event_processor.start().await?;

        // Start strategy manager with event stream
        info!("Starting strategy manager...");
        let trade_rx = self.strategy_manager.start(event_rx).await?;

        // Process trades
        info!("Starting trade executor...");
        self.trade_executor.start(trade_rx, self.risk_manager.get_circuit_breaker()).await?;

        // Wait for risk monitor (runs forever)
        info!("All components started, waiting for risk monitor...");
        if let Err(e) = risk_handle.await {
            error!("Risk monitor task failed: {:?}", e);
        }

        Ok(())
    }
}
