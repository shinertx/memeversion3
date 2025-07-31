use crate::event_processor::EventMessage;
use crate::strategies::{create_strategy, Strategy};
use anyhow::Result;
use shared_models::{MarketEvent, StrategyAction, StrategyAllocation, TradeMode, OrderDetails};
use std::collections::HashMap;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tracing::{debug, error, info, warn};

pub struct StrategyManager {
    strategies: HashMap<String, ActiveStrategy>,
}

struct ActiveStrategy {
    handle: JoinHandle<()>,
    tx: mpsc::Sender<MarketEvent>,
    mode: TradeMode,
}

pub struct TradeRequest {
    pub strategy_id: String,
    pub details: OrderDetails,
    pub mode: TradeMode,
}

impl StrategyManager {
    pub fn new() -> Self {
        Self {
            strategies: HashMap::new(),
        }
    }

    pub async fn start(
        &mut self,
        mut event_rx: mpsc::Receiver<EventMessage>,
    ) -> Result<mpsc::Receiver<TradeRequest>> {
        let (trade_tx, trade_rx) = mpsc::channel(100);

        tokio::spawn(async move {
            let mut manager = Self::new();
            while let Some(msg) = event_rx.recv().await {
                match msg {
                    EventMessage::Allocation(allocations) => {
                        manager.reconcile_strategies(allocations, trade_tx.clone()).await;
                    }
                    EventMessage::Market(event) => {
                        manager.dispatch_event(&event).await;
                    }
                }
            }
        });

        Ok(trade_rx)
    }

    async fn reconcile_strategies(
        &mut self,
        allocations: Vec<StrategyAllocation>,
        trade_tx: mpsc::Sender<TradeRequest>,
    ) {
        let new_ids: HashMap<String, StrategyAllocation> = 
            allocations.into_iter().map(|a| (a.id.clone(), a)).collect();
        
        // Stop removed strategies
        let current_ids: Vec<String> = self.strategies.keys().cloned().collect();
        for id in current_ids {
            if !new_ids.contains_key(&id) {
                if let Some(strategy) = self.strategies.remove(&id) {
                    strategy.handle.abort();
                    info!("🛑 Stopped strategy: {}", id);
                }
            }
        }

        // Start new strategies
        for (id, alloc) in new_ids {
            if !self.strategies.contains_key(&id) {
                match create_strategy(&id) {
                    Ok(mut strategy) => {
                        if let Err(e) = strategy.init(&alloc.params).await {
                            error!("Failed to init strategy {}: {}", id, e);
                            continue;
                        }

                        let (event_tx, event_rx) = mpsc::channel(100);
                        let strategy_id = id.clone();
                        let mode = alloc.mode;
                        let trade_tx_clone = trade_tx.clone();

                        let handle = tokio::spawn(async move {
                            Self::run_strategy(
                                strategy,
                                strategy_id,
                                mode,
                                event_rx,
                                trade_tx_clone,
                            ).await;
                        });

                        self.strategies.insert(id.clone(), ActiveStrategy {
                            handle,
                            tx: event_tx,
                            mode,
                        });

                        info!("🚀 Started strategy: {} in mode {:?}", id, mode);
                    }
                    Err(e) => {
                        warn!("Failed to create strategy {}: {}", id, e);
                    }
                }
            }
        }
    }

    async fn dispatch_event(&self, event: &MarketEvent) {
        let _event_type = event.get_type();
        
        for (id, strategy) in &self.strategies {
            if let Err(e) = strategy.tx.send(event.clone()).await {
                debug!("Failed to send event to strategy {}: {}", id, e);
            }
        }
    }

    async fn run_strategy(
        mut strategy: Box<dyn Strategy>,
        strategy_id: String,
        mode: TradeMode,
        mut event_rx: mpsc::Receiver<MarketEvent>,
        trade_tx: mpsc::Sender<TradeRequest>,
    ) {
        info!("🎯 Strategy {} running in {:?} mode", strategy_id, mode);

        while let Some(event) = event_rx.recv().await {
            match strategy.on_event(&event).await {
                Ok(StrategyAction::Execute(details)) => {
                    let request = TradeRequest {
                        strategy_id: strategy_id.clone(),
                        details,
                        mode,
                    };
                    if let Err(e) = trade_tx.send(request).await {
                        error!("Failed to send trade request: {}", e);
                    }
                }
                Ok(StrategyAction::Hold) => {}
                Err(e) => {
                    error!("Strategy {} error: {}", strategy_id, e);
                }
            }
        }

        info!("Strategy {} stopped", strategy_id);
    }
}
