use anyhow::{anyhow, Result};
use reqwest::Client;
use serde::Deserialize;
use solana_sdk::{pubkey::Pubkey, transaction::VersionedTransaction};
use std::time::Duration;
use tracing::info;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JupiterQuoteResponse {
    pub data: Vec<JupiterQuote>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JupiterQuote {
    pub out_amount: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SwapResponse {
    pub swap_transaction: String,
}

pub struct JupiterClient {
    client: Client,
    api_url: String,
}

impl JupiterClient {
    pub fn new(api_url: String) -> Self {
        Self {
            client: Client::builder().timeout(Duration::from_secs(15)).build().unwrap(),
            api_url,
        }
    }

    pub async fn get_price_usd(&self, token_mint: &str) -> Result<f64> {
        let input_mint = "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v"; // USDC
        let amount_in_smallest_unit = 1_000_000; // 1 USDC
        
        let url = format!(
            "{}/quote?inputMint={}&outputMint={}&amount={}",
            self.api_url, input_mint, token_mint, amount_in_smallest_unit
        );

        let response: JupiterQuoteResponse = self.client.get(&url).send().await?.json().await?;
        let best_route = response.data.first().ok_or_else(|| anyhow!("No route found"))?;
        
        let out_amount: u64 = best_route.out_amount.parse()?;
        let price_per_token = (amount_in_smallest_unit as f64 / 1_000_000.0) / (out_amount as f64 / 1_000_000_000.0);
        
        Ok(price_per_token)
    }

    pub async fn get_swap_transaction(
        &self, 
        user_pubkey: &Pubkey, 
        input_mint: &str, 
        output_mint: &str, 
        amount_in_usd: f64, 
        slippage_bps: u16
    ) -> Result<String> {
        let amount_sol_approx = amount_in_usd / 150.0;
        let amount_lamports = (amount_sol_approx * 1_000_000_000.0) as u64;

        let quote_url = format!(
            "{}/quote?inputMint={}&outputMint={}&amount={}&slippageBps={}",
            self.api_url, input_mint, output_mint, amount_lamports, slippage_bps
        );
        let quote_response: serde_json::Value = self.client.get(&quote_url).send().await?.json().await?;
        
        let swap_payload = serde_json::json!({
            "quoteResponse": quote_response,
            "userPublicKey": user_pubkey.to_string(),
            "wrapAndUnwrapSol": true,
        });

        let swap_url = format!("{}/swap", self.api_url);
        let response: SwapResponse = self.client.post(swap_url).json(&swap_payload).send().await?.json().await?;
        
        Ok(response.swap_transaction)
    }
}

pub fn deserialize_transaction(tx_b64: &str) -> Result<VersionedTransaction> {
    let tx_bytes = base64::decode(tx_b64)?;
    bincode::deserialize(&tx_bytes).map_err(|e| anyhow!("Failed to deserialize: {}", e))
}
