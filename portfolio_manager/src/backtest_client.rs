use anyhow::Result;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use shared_models::StrategySpec;
use std::time::Duration;
use tracing::{error, info};

#[derive(Debug, Serialize)]
pub struct BacktestRequest {
    pub strategy_params: Value,
    pub symbol: String,
    pub start_date: String,
    pub end_date: String,
}

impl From<&StrategySpec> for BacktestRequest {
    fn from(spec: &StrategySpec) -> Self {
        Self {
            strategy_params: spec.params.clone(),
            symbol: "SOL".to_string(), // Default symbol for meme trading
            start_date: "2024-01-01".to_string(),
            end_date: "2024-12-31".to_string(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct BacktestResponse {
    pub sharpe_ratio: f64,
    pub total_return: f64,
    pub max_drawdown: f64,
    pub trade_count: i32,
    pub win_rate: f64,
}

impl std::fmt::Display for BacktestResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Sharpe: {:.2}, Return: {:.2}%, Drawdown: {:.2}%, Trades: {}, Win Rate: {:.2}%",
            self.sharpe_ratio,
            self.total_return * 100.0,
            self.max_drawdown * 100.0,
            self.trade_count,
            self.win_rate * 100.0
        )
    }
}

pub struct BacktestClient {
    client: Client,
    base_url: String,
}

impl BacktestClient {
    pub fn new(base_url: String) -> Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .context("Failed to create HTTP client for backtest API")?;

        Ok(Self { client, base_url })
    }

    pub async fn submit_backtest(&self, spec: &StrategySpec) -> Result<BacktestResponse> {
        info!("📊 Submitting backtest for strategy: {}", spec.id);

        let request: BacktestRequest = spec.into();
        
        // For now, simulate a backtest result based on strategy family
        // In production, this would call an external API
        let simulated_result = self.simulate_backtest(&request).await?;

        info!(
            "✅ Backtest completed for {}: Sharpe={:.2}, Return={:.2}%",
            spec.id,
            simulated_result.sharpe_ratio,
            simulated_result.total_return * 100.0
        );

        Ok(simulated_result)
    }

    pub async fn get_backtest_result(&self, job_id: &str) -> Result<BacktestResponse> {
        info!("📋 Fetching backtest result for job: {}", job_id);
        
        // For now, return a simulated result
        // In production, this would poll the external API
        let result = BacktestResponse {
            sharpe_ratio: 1.2,
            total_return: 0.15,
            max_drawdown: 0.08,
            trade_count: 42,
            win_rate: 0.65,
        };

        Ok(result)
    }

    async fn simulate_backtest(&self, request: &BacktestRequest) -> Result<BacktestResponse> {
        // Simulate different performance based on strategy parameters
        let base_sharpe = 0.8;
        let param_boost = if request.strategy_params.is_object() { 0.3 } else { 0.0 };
        
        let result = BacktestResponse {
            sharpe_ratio: base_sharpe + param_boost,
            total_return: 0.12 + (param_boost * 0.1),
            max_drawdown: 0.06 + (param_boost * 0.02),
            trade_count: 25 + (param_boost * 10.0) as i32,
            win_rate: 0.62 + (param_boost * 0.05),
        };

        // Simulate processing time
        tokio::time::sleep(Duration::from_millis(100)).await;
        
        Ok(result)
    }

    pub async fn health_check(&self) -> Result<bool> {
        let url = format!("{}/health", self.base_url);
        
        match self.client.get(&url).send().await {
            Ok(response) => Ok(response.status().is_success()),
            Err(e) => {
                error!("Backtest service health check failed: {}", e);
                Ok(false)
            }
        }
    }
}