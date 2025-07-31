use crate::providers::validate_simulated_event;
use anyhow::Result;
use redis::AsyncCommands;
use shared_models::MarketEvent;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use tracing::{error, info, warn, level_filters::LevelFilter};
use tracing_subscriber;
use std::time::Duration;
use prometheus::{Counter, Gauge, HistogramVec, Registry, Encoder, TextEncoder};
use axum::{extract::State, http::StatusCode, response::Response, routing::get, Router};

mod providers;

// Configuration - normally this would be in a separate config.rs file
pub struct Config {
    pub redis_url: String,
    pub helius_api_key: String,
    pub pyth_api_key: String,
    pub twitter_bearer_token: String,
    pub farcaster_api_url: String,
}

impl Config {
    pub fn from_env() -> Self {
        Self {
            redis_url: std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379/".to_string()),
            helius_api_key: std::env::var("HELIUS_API_KEY").unwrap_or_else(|_| "demo_key".to_string()),
            pyth_api_key: std::env::var("PYTH_API_KEY").unwrap_or_else(|_| "demo_key".to_string()),
            twitter_bearer_token: std::env::var("TWITTER_BEARER_TOKEN").unwrap_or_else(|_| "demo_token".to_string()),
            farcaster_api_url: std::env::var("FARCASTER_API_URL").unwrap_or_else(|_| "https://api.farcaster.xyz".to_string()),
        }
    }
}

pub mod config {
    use super::Config;
    use std::sync::OnceLock;
    
    static CONFIG_INSTANCE: OnceLock<Config> = OnceLock::new();
    
    pub static CONFIG: std::sync::LazyLock<Config> = std::sync::LazyLock::new(|| Config::from_env());
}

// Metrics for monitoring data validation (Red Team audit requirement)
#[derive(Clone)]
pub struct DataValidationMetrics {
    pub registry: Registry,
    pub events_total: Counter,
    pub events_invalid: Counter,
    pub circuit_breaker_active: Gauge,
    pub validation_latency: HistogramVec,
    pub provider_events: Counter,
}

impl DataValidationMetrics {
    pub fn new() -> anyhow::Result<Self> {
        let registry = Registry::new();
        
        let events_total = Counter::new("data_events_total", "Total number of data events processed")
            .context("Failed to create events_total counter")?;
        let events_invalid = Counter::new("data_events_invalid_total", "Total number of invalid data events")
            .context("Failed to create events_invalid counter")?;
        let circuit_breaker_active = Gauge::new("data_circuit_breaker_active", "Whether circuit breaker is active (1) or inactive (0)")
            .context("Failed to create circuit_breaker_active gauge")?;
        let validation_latency = HistogramVec::new(
            prometheus::HistogramOpts::new("data_validation_duration_ms", "Time spent validating data events"),
            &["event_type", "provider"]
        ).context("Failed to create validation_latency histogram")?;
        let provider_events = Counter::new("data_provider_events_total", "Events received per provider")
            .context("Failed to create provider_events counter")?;
        
        registry.register(Box::new(events_total.clone()))
            .context("Failed to register events_total metric")?;
        registry.register(Box::new(events_invalid.clone()))
            .context("Failed to register events_invalid metric")?;
        registry.register(Box::new(circuit_breaker_active.clone()))
            .context("Failed to register circuit_breaker_active metric")?;
        registry.register(Box::new(validation_latency.clone()))
            .context("Failed to register validation_latency metric")?;
        registry.register(Box::new(provider_events.clone()))
            .context("Failed to register provider_events metric")?;
        
        Ok(Self {
            registry,
            events_total,
            events_invalid,
            circuit_breaker_active,
            validation_latency,
            provider_events,
        })
    }
}

// Metrics endpoint handler
async fn metrics_handler(State(metrics): State<DataValidationMetrics>) -> Result<Response<String>, StatusCode> {
    let encoder = TextEncoder::new();
    let metric_families = metrics.registry.gather();
    let mut buffer = Vec::new();
    
    encoder.encode(&metric_families, &mut buffer)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    
    let body = String::from_utf8(buffer)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    
    Response::builder()
        .header("content-type", "text/plain; version=0.0.4")
        .body(body)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_max_level(LevelFilter::INFO)
        .init();

    info!("🚀 Starting MemeSnipe v25 Market Data Gateway with Data Validation Layer");

    // Initialize metrics for monitoring
    let metrics = DataValidationMetrics::new()
        .context("Failed to initialize data validation metrics")?;

    // Start metrics server for Prometheus scraping
    let metrics_clone = metrics.clone();
    tokio::spawn(async move {
        let app = Router::new()
            .route("/metrics", get(metrics_handler))
            .with_state(metrics_clone);
        
        match tokio::net::TcpListener::bind("0.0.0.0:9185").await {
            Ok(listener) => {
                info!("📊 Metrics server listening on port 9185");
                if let Err(e) = axum::serve(listener, app).await {
                    error!("Metrics server failed: {}", e);
                }
            }
            Err(e) => {
                error!("Failed to bind metrics server to port 9185: {}", e);
            }
        }
    });

    // Create Redis connection
    let redis_client = redis::Client::open(config::CONFIG.redis_url.as_str())?;
    let mut redis_conn = redis_client.get_async_connection().await?;

    // Create channel for receiving market events from simulated providers
    let (tx, mut rx) = mpsc::channel::<MarketEvent>(1000);

    // Spawn simulated data provider (as per README - market data is simulated but designed for easy replacement)
    let tx_sim = tx.clone();
    tokio::spawn(async move {
        if let Err(e) = run_simulated_data_provider(tx_sim).await {
            error!("Simulated data provider failed: {}", e);
        }
    });

    // Main event processing loop
    info!("🔍 Starting market data processing loop");
    while let Some(event) = rx.recv().await {
        let start_time = std::time::Instant::now();
        
        // Validate event (simple validation for simulation mode)
        if validate_simulated_event(&event) {
            // Publish to Redis
            match redis_conn.xadd::<&str, &str, &str, &str>(
                "events:price",
                "*",
                &[("data", &serde_json::to_string(&event).unwrap_or_default())],
            ).await {
                Ok(_) => {
                    metrics.events_total.inc();
                    info!("📡 Published market event: {:?}", event);
                }
                Err(e) => {
                    metrics.events_invalid.inc();
                    error!("Failed to publish event to Redis: {}", e);
                }
            }
        } else {
            metrics.events_invalid.inc();
            warn!("🚫 Invalid event dropped: {:?}", event);
        }
        
        // Update metrics
        let processing_duration = start_time.elapsed().as_millis() as f64;
        metrics.validation_latency
            .with_label_values(&[&event.get_type().to_string(), "simulated"])
            .observe(processing_duration);
    }

    Ok(())
}
}

/// Simulated data provider for development and testing
/// As per README: "Market data is currently simulated but designed for easy replacement with real feeds"
async fn run_simulated_data_provider(tx: mpsc::Sender<MarketEvent>) -> Result<()> {
    let mut interval = tokio::time::interval(Duration::from_secs(1));
    let mut sol_price = 100.0; // Starting SOL price
    
    info!("🎲 Starting simulated data provider");
    
    loop {
        interval.tick().await;
        
        // Simulate SOL price with random walk
        sol_price += (rand::random::<f64>() - 0.5) * 2.0; // +/- $1 volatility
        sol_price = sol_price.max(50.0).min(200.0); // Keep within reasonable bounds
        
        let sol_event = MarketEvent::SolPrice(shared_models::SolPriceEvent {
            price_usd: sol_price,
        });
        
        if tx.send(sol_event).await.is_err() {
            warn!("Receiver dropped, stopping simulated data provider");
            break;
        }
    }
    
    Ok(())
}