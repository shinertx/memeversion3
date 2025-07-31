use anyhow::{anyhow, Context, Result};
use base64::{Engine as _, engine::general_purpose};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use solana_sdk::{pubkey::Pubkey, transaction::VersionedTransaction};
use std::time::Duration;
use tracing::info;

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JupiterQuote {
    pub out_amount: String,
    #[serde(rename = "marketInfos")]
    pub market_infos: Vec<MarketInfo>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketInfo {
    pub lp_fee: LpFee,
    pub liquidity: f64,
}

#[derive(Debug, Deserialize, Serialize)]
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

#[derive(Debug, Serialize)]
pub struct QuoteResult {
    pub out_amount: u64,
    pub price_per_token: f64,
}

pub struct JupiterClient {
    client: Client,
}

impl JupiterClient {
    pub fn new() -> Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(15))
            .build()
            .context("Failed to create HTTP client for Jupiter")?;
        
        Ok(Self { client })
    }

    pub async fn get_quote(&self, amount_sol: f64, output_mint: &str) -> Result<JupiterQuote> {
        // Hardcoded for now - should be passed as parameter
        let slippage_bps = 30;
        
        let input_mint = "So11111111111111111111111111111111111111112"; // SOL mint
        let amount_lamports = (amount_sol * 1_000_000_000.0) as u64;

        let quote_url = format!(
            "https://quote-api.jup.ag/v6/quote?inputMint={}&outputMint={}&amount={}&slippageBps={}",
            input_mint, output_mint, amount_lamports, slippage_bps
        );

        let response: JupiterQuote = self.client.get(&quote_url).send().await?.json().await?;
        Ok(response)
    }

    pub async fn get_price(&self, token_mint: &str) -> Result<f64> {
        // Hardcoded jupiter URL for now
        let jupiter_url = "https://quote-api.jup.ag/v6";
        let url = format!("{}/price?ids={}", jupiter_url, token_mint);
        
        let response: serde_json::Value = self.client.get(&url).send().await?.json().await?;
        
        if let Some(price_data) = response.get("data").and_then(|d| d.get(token_mint)) {
            if let Some(price) = price_data.get("price").and_then(|p| p.as_f64()) {
                return Ok(price);
            }
        }
        
        Err(anyhow!("Failed to get price for token {}", token_mint))
    }

    pub async fn get_swap_transaction(&self, user_pubkey: &Pubkey, output_mint: &str, amount_usd_to_swap: f64, slippage_bps: u16) -> Result<String> {
        let amount_sol_approx = amount_usd_to_swap / 150.0;
        let amount_lamports = (amount_sol_approx * 1_000_000_000.0) as u64;

        let quote_url = format!(
            "https://quote-api.jup.ag/v6/quote?inputMint=So11111111111111111111111111111111111111112&outputMint={}&amount={}&slippageBps={}",
            output_mint, amount_lamports, slippage_bps
        );
        let quote_response: serde_json::Value = self.client.get(&quote_url).send().await?.json().await?;
        
        let swap_payload = serde_json::json!({
            "quoteResponse": quote_response,
            "userPublicKey": user_pubkey.to_string(),
            "wrapAndUnwrapSol": true,
        });

        let swap_url = "https://quote-api.jup.ag/v6/swap";
        let response: SwapResponse = self.client.post(swap_url).json(&swap_payload).send().await?.json().await?;
        info!("Generated Jupiter swap transaction for {} USD.", amount_usd_to_swap);
        Ok(response.swap_transaction)
    }
}

pub fn deserialize_transaction(tx_b64: &str) -> Result<VersionedTransaction> {
    let tx_bytes = general_purpose::STANDARD.decode(tx_b64)?;
    bincode::deserialize(&tx_bytes).context("Failed to deserialize transaction")
}
