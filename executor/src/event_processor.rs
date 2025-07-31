use anyhow::Result;
use redis::AsyncCommands;
use shared_models::{MarketEvent, PriceTick, SolPriceEvent, SocialMention, StrategyAllocation};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, error, info, warn};

pub struct EventProcessor {
    redis_client: redis::Client,
    sol_usd_price: Arc<RwLock<f64>>,
}

impl EventProcessor {
    pub fn new(redis_client: redis::Client, sol_usd_price: Arc<RwLock<f64>>) -> Self {
        Self {
            redis_client,
            sol_usd_price,
        }
    }

    pub async fn start(&self) -> Result<mpsc::Receiver<EventMessage>> {
        let (tx, rx) = mpsc::channel(1000);
        
        let redis_client = self.redis_client.clone();
        let sol_usd_price = self.sol_usd_price.clone();

        tokio::spawn(async move {
            if let Err(e) = Self::process_events(redis_client, sol_usd_price, tx).await {
                error!("Event processor failed: {}", e);
            }
        });

        Ok(rx)
    }

    async fn process_events(
        redis_client: redis::Client,
        sol_usd_price: Arc<RwLock<f64>>,
        tx: mpsc::Sender<EventMessage>,
    ) -> Result<()> {
        let mut conn = redis_client.get_multiplexed_async_connection().await?;
        
        let mut stream_ids = HashMap::new();
        stream_ids.insert("allocations_channel".to_string(), "0".to_string());
        stream_ids.insert("events:price".to_string(), "0".to_string());
        stream_ids.insert("events:social".to_string(), "0".to_string());
        stream_ids.insert("events:sol_price".to_string(), "0".to_string());

        info!("📡 Event processor started, monitoring Redis streams...");

        loop {
            let keys: Vec<String> = stream_ids.keys().cloned().collect();
            let ids: Vec<String> = stream_ids.values().cloned().collect();

            match conn.xread_options::<String, String, redis::streams::StreamReadReply>(
                &keys,
                &ids,
                &redis::streams::StreamReadOptions::default()
                    .block(1000)
                    .count(100),
            ).await {
                Ok(reply) => {
                    for stream_key in reply.keys {
                        let stream_name = stream_key.key.clone();

                        for message in stream_key.ids {
                            stream_ids.insert(stream_name.clone(), message.id.clone());

                            if stream_name == "allocations_channel" {
                                if let Some(data) = message.map.get("allocations") {
                                    if let Ok(allocations_str) = redis::from_redis_value::<String>(data) {
                                        if let Ok(allocations) = serde_json::from_str::<Vec<StrategyAllocation>>(&allocations_str) {
                                            debug!("📋 Received {} strategy allocations", allocations.len());
                                            let _ = tx.send(EventMessage::Allocation(allocations)).await;
                                        }
                                    }
                                }
                            } else if let Some(data) = message.map.get("data") {
                                if let Ok(event_str) = redis::from_redis_value::<String>(data) {
                                    if let Ok(event_data) = serde_json::from_str::<serde_json::Value>(&event_str) {
                                        match stream_name.as_str() {
                                            "events:price" => {
                                                if let Ok(tick) = serde_json::from_value::<PriceTick>(event_data) {
                                                    let _ = tx.send(EventMessage::Market(MarketEvent::Price(tick))).await;
                                                }
                                            }
                                            "events:social" => {
                                                if let Ok(mention) = serde_json::from_value::<SocialMention>(event_data) {
                                                    let _ = tx.send(EventMessage::Market(MarketEvent::Social(mention))).await;
                                                }
                                            }
                                            "events:sol_price" => {
                                                if let Ok(sol_event) = serde_json::from_value::<SolPriceEvent>(event_data) {
                                                    *sol_usd_price.write().await = sol_event.price_usd;
                                                    let _ = tx.send(EventMessage::Market(MarketEvent::SolPrice(sol_event))).await;
                                                }
                                            }
                                            _ => {}
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    warn!("Redis read error: {}, retrying...", e);
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                }
            }

            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    }
}

#[derive(Debug, Clone)]
pub enum EventMessage {
    Market(MarketEvent),
    Allocation(Vec<StrategyAllocation>),
}
