use crate::config::CONFIG;
use anyhow::{anyhow, Context, Result};
use reqwest::Client;
use serde::Deserialize;
use solana_sdk::{pubkey::Pubkey, transaction::VersionedTransaction};
use std::time::Duration;
use tracing::info;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JupiterQuote {
    pub out_amount: String,
    #[serde(rename = "marketInfos")]
    pub market_infos: Vec<MarketInfo>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketInfo {
    pub lp_fee: LpFee,
    pub liquidity: f64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LpFee {
    pub amount: String,
    pub mint: String,
    pub pct: f64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JupiterQuoteResponse {
    pub data: Vec<JupiterQuote>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SwapResponse {
    pub swap_transaction: String,
}

pub struct QuoteResult {
    pub out_amount: u64,
    pub price_per_token: f64,
}

pub struct JupiterClient {
    client: Client,
}

impl JupiterClient {
    pub fn new() -> Self {
        Self {
            client: Client::builder().timeout(Duration::from_secs(15)).build().unwrap(),
        }
    }

    pub async fn get_quote(&self, amount_sol_to_swap: f64, output_mint: &str) -> Result<QuoteResult> {
        let amount_lamports = (amount_sol_to_swap * 1_000_000_000.0) as u64;
        let url = format!(
            "{}/quote?inputMint=So11111111111111111111111111111111111111112&outputMint={}&amount={}&slippageBps={}",
            CONFIG.jupiter_api_url, output_mint, amount_lamports, CONFIG.slippage_bps
        );

        let response: JupiterQuoteResponse = self.client.get(&url).send().await?.json().await?;
        let best_route = response.data.first().ok_or_else(|| anyhow!("No route found by Jupiter for {}", output_mint))?;
        
        let out_amount: u64 = best_route.out_amount.parse()?;
        let price_per_token = amount_sol_to_swap / (out_amount as f64 / 1_000_000_000.0);
        
        info!("Jupiter quote for {} SOL -> {}. Price per token: {:.8} USD", amount_sol_to_swap, output_mint, price_per_token);

        Ok(QuoteResult { out_amount, price_per_token })
    }

    pub async fn get_swap_transaction(&self, user_pubkey: &Pubkey, output_mint: &str, amount_usd_to_swap: f64) -> Result<String> {
        let amount_sol_approx = amount_usd_to_swap / 150.0;
        let amount_lamports = (amount_sol_approx * 1_000_000_000.0) as u64;

        let quote_url = format!(
            "{}/quote?inputMint=So11111111111111111111111111111111111111112&outputMint={}&amount={}&slippageBps={}",
            CONFIG.jupiter_api_url, output_mint, amount_lamports, CONFIG.slippage_bps
        );
        let quote_response: serde_json::Value = self.client.get(&quote_url).send().await?.json().await?;
        
        let swap_payload = serde_json::json!({
            "quoteResponse": quote_response,
            "userPublicKey": user_pubkey.to_string(),
            "wrapAndUnwrapSol": true,
        });

        let swap_url = format!("{}/swap", CONFIG.jupiter_api_url);
        let response: SwapResponse = self.client.post(swap_url).json(&swap_payload).send().await?.json().await?;
        info!("Generated Jupiter swap transaction for {} USD.", amount_usd_to_swap);
        Ok(response.swap_transaction)
    }
}

pub fn deserialize_transaction(tx_b64: &str) -> Result<VersionedTransaction> {
    let tx_bytes = base64::decode(tx_b64)?;
    bincode::deserialize(&tx_bytes).context("Failed to deserialize transaction")
}
