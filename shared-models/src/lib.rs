use serde::{Deserialize, Serialize};

// Event Types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EventType {
    Price,
    Social,
    Depth,
    Bridge,
    Funding,
    OnChain,
    SolPrice,
    TwitterRaw,
    FarcasterRaw,
}

impl EventType {
    pub fn to_string(&self) -> &'static str {
        match self {
            EventType::Price => "price",
            EventType::Social => "social",
            EventType::Depth => "depth",
            EventType::Bridge => "bridge",
            EventType::Funding => "funding",
            EventType::OnChain => "onchain",
            EventType::SolPrice => "sol_price",
            EventType::TwitterRaw => "twitter_raw",
            EventType::FarcasterRaw => "farcaster_raw",
        }
    }
}

// Market Events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MarketEvent {
    Price(PriceTick),
    Social(SocialMention),
    Depth(DepthEvent),
    Bridge(BridgeEvent),
    Funding(FundingEvent),
    OnChain(OnChainEvent),
    SolPrice(SolPriceEvent),
    TwitterRaw(TwitterRawEvent),
    FarcasterRaw(FarcasterRawEvent),
}

impl MarketEvent {
    pub fn get_type(&self) -> EventType {
        match self {
            MarketEvent::Price(_) => EventType::Price,
            MarketEvent::Social(_) => EventType::Social,
            MarketEvent::Depth(_) => EventType::Depth,
            MarketEvent::Bridge(_) => EventType::Bridge,
            MarketEvent::Funding(_) => EventType::Funding,
            MarketEvent::OnChain(_) => EventType::OnChain,
            MarketEvent::SolPrice(_) => EventType::SolPrice,
            MarketEvent::TwitterRaw(_) => EventType::TwitterRaw,
            MarketEvent::FarcasterRaw(_) => EventType::FarcasterRaw,
        }
    }
    
    pub fn token(&self) -> &str {
        match self {
            MarketEvent::Price(e) => &e.token_address,
            MarketEvent::Social(e) => &e.token_address,
            MarketEvent::Depth(e) => &e.token_address,
            MarketEvent::Bridge(e) => &e.token_address,
            MarketEvent::Funding(e) => &e.token_address,
            MarketEvent::OnChain(e) => &e.token_address,
            MarketEvent::SolPrice(_) => "SOL",
            MarketEvent::TwitterRaw(_) => "",
            MarketEvent::FarcasterRaw(_) => "",
        }
    }
}

// Event Structs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriceTick {
    pub token_address: String,
    pub price_usd: f64,
    pub volume_usd_1m: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SocialMention {
    pub token_address: String,
    pub source: String,
    pub sentiment: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DepthEvent {
    pub token_address: String,
    pub bid_price: f64,
    pub ask_price: f64,
    pub bid_size_usd: f64,
    pub ask_size_usd: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgeEvent {
    pub token_address: String,
    pub source_chain: String,
    pub destination_chain: String,
    pub volume_usd: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FundingEvent {
    pub token_address: String,
    pub funding_rate_pct: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OnChainEvent {
    pub token_address: String,
    pub event_type: String,
    pub details: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SolPriceEvent {
    pub price_usd: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TwitterRawEvent {
    pub tweet_id: String,
    pub text: String,
    pub author_id: String,
    pub timestamp: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FarcasterRawEvent {
    pub cast_hash: String,
    pub text: String,
    pub author_fid: String,
    pub timestamp: i64,
}

// Trading Types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Side {
    Long,
    Short,
}

impl std::fmt::Display for Side {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Side::Long => write!(f, "Long"),
            Side::Short => write!(f, "Short"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TradeMode {
    Simulating,
    Paper,
    Live,
}

// Strategy Types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderDetails {
    pub token_address: String,
    pub suggested_size_usd: f64,
    pub confidence: f64,
    pub side: Side,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StrategyAction {
    Execute(OrderDetails),
    Hold,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyAllocation {
    pub id: String,
    pub weight: f64,
    pub sharpe_ratio: f64,
    pub mode: TradeMode,
    pub params: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategySpec {
    pub id: String,
    pub family: String,
    pub params: serde_json::Value,
    #[serde(default = "default_fitness")]
    pub fitness: f64,
}

fn default_fitness() -> f64 {
    0.1 // Default fitness score for new strategies
}

// Signer Types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignRequest {
    pub transaction_b64: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignResponse {
    pub signed_transaction_b64: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BacktestResult {
    pub spec_id: String,
    pub strategy_id: String,
    pub sharpe_ratio: f64,
    pub total_pnl: f64,
    pub trade_count: u32,
    pub capital_tested_usd: f64,
    pub metadata: serde_json::Map<String, serde_json::Value>,
}

// Strategy trait for the execution engine
use async_trait::async_trait;
use std::collections::HashSet;

#[async_trait]
pub trait Strategy: Send + Sync {
    fn id(&self) -> &'static str;
    fn subscriptions(&self) -> HashSet<EventType>;
    async fn init(&mut self, params: &serde_json::Value) -> anyhow::Result<()>;
    async fn on_event(&mut self, event: &MarketEvent) -> anyhow::Result<StrategyAction>;
}
