use crate::providers::{helius_consumer, pyth_consumer, twitter_consumer, farcaster_consumer, DataValidator};
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
    pub fn new() -> Self {
        let registry = Registry::new();
        
        let events_total = Counter::new("data_events_total", "Total number of data events processed").unwrap();
        let events_invalid = Counter::new("data_events_invalid_total", "Total number of invalid data events").unwrap();
        let circuit_breaker_active = Gauge::new("data_circuit_breaker_active", "Whether circuit breaker is active (1) or inactive (0)").unwrap();
        let validation_latency = HistogramVec::new(
            prometheus::HistogramOpts::new("data_validation_duration_ms", "Time spent validating data events"),
            &["event_type", "provider"]
        ).unwrap();
        let provider_events = Counter::new("data_provider_events_total", "Events received per provider").unwrap();
        
        registry.register(Box::new(events_total.clone())).unwrap();
        registry.register(Box::new(events_invalid.clone())).unwrap();
        registry.register(Box::new(circuit_breaker_active.clone())).unwrap();
        registry.register(Box::new(validation_latency.clone())).unwrap();
        registry.register(Box::new(provider_events.clone())).unwrap();
        
        Self {
            registry,
            events_total,
            events_invalid,
            circuit_breaker_active,
            validation_latency,
            provider_events,
        }
    }
}

// Metrics endpoint handler
async fn metrics_handler(State(metrics): State<DataValidationMetrics>) -> Result<Response<String>, StatusCode> {
    let encoder = TextEncoder::new();
    let metric_families = metrics.registry.gather();
    let mut buffer = Vec::new();
    
    encoder.encode(&metric_families, &mut buffer)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    
    String::from_utf8(buffer)
        .map(|body| Response::builder()
            .header("content-type", "text/plain; version=0.0.4")
            .body(body)
            .unwrap())
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
    let metrics = DataValidationMetrics::new();

    // Start metrics server for Prometheus scraping
    let metrics_clone = metrics.clone();
    tokio::spawn(async move {
        let app = Router::new()
            .route("/metrics", get(metrics_handler))
            .with_state(metrics_clone);
        
        let listener = tokio::net::TcpListener::bind("0.0.0.0:9185").await.unwrap();
        info!("📊 Metrics server listening on port 9185");
        axum::serve(listener, app).await.unwrap();
    });

    // Create Redis connection
    let redis_client = redis::Client::open(config::CONFIG.redis_url.as_str())?;
    let mut redis_conn = redis_client.get_async_connection().await?;

    // Create data validator
    let validator = Arc::new(Mutex::new(DataValidator::new()));

    // Create channel for receiving market events from providers
    let (tx, mut rx) = mpsc::channel::<MarketEvent>(1000);

    // Initialize and spawn data providers
    let tx_helius = tx.clone();
    tokio::spawn(async move {
        if let Err(e) = helius_consumer::run(tx_helius).await {
            error!("Helius consumer failed: {}", e);
        }
    });

    let tx_pyth = tx.clone();
    tokio::spawn(async move {
        if let Err(e) = pyth_consumer::run(tx_pyth).await {
            error!("Pyth consumer failed: {}", e);
        }
    });

    let tx_twitter = tx.clone();
    tokio::spawn(async move {
        if let Err(e) = twitter_consumer::run(tx_twitter).await {
            error!("Twitter consumer failed: {}", e);
        }
    });

    let tx_farcaster = tx.clone();
    tokio::spawn(async move {
        if let Err(e) = farcaster_consumer::run(tx_farcaster).await {
            error!("Farcaster consumer failed: {}", e);
        }
    });

    // Spawn data quality monitoring task
    let validator_monitor = validator.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(30));
        loop {
            interval.tick().await;
            let validator = validator_monitor.lock().await;
            let (invalid_count, total_count, invalid_ratio) = validator.get_data_quality_stats();
            
            if total_count > 0 {
                info!(
                    "📊 Data Quality Stats: {}/{} valid ({:.2}% invalid), Circuit Breaker: {}",
                    total_count - invalid_count,
                    total_count,
                    invalid_ratio * 100.0,
                    if validator.is_circuit_breaker_active() { "🔴 ACTIVE" } else { "🟢 INACTIVE" }
                );
            }
        }
    });

    // Main event processing loop with validation and metrics
    info!("🔍 Data validation and processing loop started");
    while let Some(event) = rx.recv().await {
        let start_time = std::time::Instant::now();
        metrics.events_total.inc();
        
        let mut validator_guard = validator.lock().await;
        
        // Validate the event
        let validated_event = validator_guard.validate_event(event.clone(), "provider").await;
        
        // Update metrics
        let validation_duration = start_time.elapsed().as_millis() as f64;
        metrics.validation_latency
            .with_label_values(&[event.get_type().to_string(), "combined"])
            .observe(validation_duration);
        
        if !validated_event.is_valid {
            metrics.events_invalid.inc();
        }
        
        // Update circuit breaker metric
        metrics.circuit_breaker_active.set(if validator_guard.is_circuit_breaker_active() { 1.0 } else { 0.0 });
        
        // Only process valid events unless circuit breaker is active
        if validated_event.is_valid && !validator_guard.is_circuit_breaker_active() {
            // Serialize and publish to Redis
            match serde_json::to_string(&validated_event.event) {
                Ok(serialized) => {
                    let stream_key = format!("events:{}", event.get_type().to_string());
                    
                    // Prepare fields for Redis
                    let timestamp_str = validated_event.timestamp_ms.to_string();
                    let fields = vec![
                        ("data", serialized.as_str()),
                        ("timestamp", timestamp_str.as_str()),
                        ("source", validated_event.source.as_str()),
                        ("token", event.token())
                    ];
                    
                    let result: redis::RedisResult<String> = redis_conn.xadd(&stream_key, "*", &fields).await;
                    
                    if let Err(e) = result {
                        error!("Failed to publish to Redis stream {}: {}", stream_key, e);
                    }
                }
                Err(e) => {
                    error!("Failed to serialize event: {}", e);
                }
            }
        } else if !validated_event.is_valid {
            warn!(
                "🚫 Dropping invalid event from {}: {:?}", 
                validated_event.source, 
                validated_event.validation_errors
            );
        } else {
            warn!("🔴 Circuit breaker active - dropping all events");
        }
        
        drop(validator_guard); // Release lock quickly
    }

    Ok(())
}