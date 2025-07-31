use anyhow::{anyhow, Result};
use reqwest::Client;
use shared_models::{SignRequest, SignResponse};
use std::time::Duration;

pub async fn get_pubkey(signer_url: &str) -> Result<String> {
    let client = Client::new();
    let url = format!("{}/pubkey", signer_url);
    let response = client.get(&url).timeout(Duration::from_secs(5)).send().await?
        .json::<serde_json::Value>().await?;
    
    response["pubkey"].as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| anyhow!("Pubkey not found in signer response"))
}

pub async fn sign_transaction(signer_url: &str, tx_b64: &str) -> Result<String> {
    let client = Client::new();
    let url = format!("{}/sign", signer_url);
    let request = SignRequest { transaction_b64: tx_b64.to_string() };
    
    let response: SignResponse = client.post(&url)
        .json(&request)
        .timeout(Duration::from_secs(5))
        .send().await?
        .json().await?;
    
    Ok(response.signed_transaction_b64)
}
