use anyhow::{anyhow, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use shared_models::StrategySpec;
use std::time::Duration;
use tracing::{debug, info, warn};

#[derive(Debug, Serialize)]
struct BacktestRequest {
    strategy_spec: StrategySpec,
    lookback_days: u32,
    initial_capital: f64,
}

#[derive(Debug, Deserialize)]
struct BacktestJobResponse {
    job_id: String,
    status: String,
    estimated_completion_seconds: u32,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct BacktestResult {
    pub job_id: String,
    pub strategy_id: String,
    pub status: String,
    pub sharpe_ratio: f64,
    pub total_return_pct: f64,
    pub max_drawdown_pct: f64,
    pub win_rate: f64,
    pub total_trades: u32,
    pub metadata: serde_json::Value,
}

pub struct BacktestClient {
    client: Client,
    api_key: String,
    base_url: String,
}

impl BacktestClient {
    pub fn new(api_key: String) -> Self {
        let base_url = std::env::var("BACKTESTING_PLATFORM_URL")
            .unwrap_or_else(|_| "https://api.heliosprime.com/v1".to_string());
        
        Self {
            client: Client::builder()
                .timeout(Duration::from_secs(30))
                .build()
                .expect("Failed to build HTTP client"),
            api_key,
            base_url,
        }
    }

    pub async fn submit_backtest(&self, spec: &StrategySpec) -> Result<String> {
        let request = BacktestRequest {
            strategy_spec: spec.clone(),
            lookback_days: 30,
            initial_capital: 10000.0,
        };

        let response = self.client
            .post(format!("{}/backtest", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&request)
            .send()
            .await?;

        if !response.status().is_success() {
            let error_text = response.text().await?;
            return Err(anyhow!("Backtest submission failed: {}", error_text));
        }

        let job_response: BacktestJobResponse = response.json().await?;
        info!(
            strategy_id = %spec.id,
            job_id = %job_response.job_id,
            "Backtest job submitted successfully"
        );

        Ok(job_response.job_id)
    }

    pub async fn get_backtest_result(&self, job_id: &str) -> Result<Option<BacktestResult>> {
        let response = self.client
            .get(format!("{}/backtest/{}", self.base_url, job_id))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .send()
            .await?;

        if response.status().as_u16() == 404 {
            return Ok(None);
        }

        if !response.status().is_success() {
            let error_text = response.text().await?;
            return Err(anyhow!("Failed to fetch backtest result: {}", error_text));
        }

        let result: BacktestResult = response.json().await?;
        
        match result.status.as_str() {
            "completed" => {
                info!(
                    job_id = %job_id,
                    sharpe_ratio = result.sharpe_ratio,
                    "Backtest completed"
                );
                Ok(Some(result))
            }
            "pending" | "running" => {
                debug!(job_id = %job_id, "Backtest still in progress");
                Ok(None)
            }
            "failed" => {
                warn!(job_id = %job_id, "Backtest failed");
                Ok(Some(result))
            }
            _ => Err(anyhow!("Unknown backtest status: {}", result.status))
        }
    }
}
