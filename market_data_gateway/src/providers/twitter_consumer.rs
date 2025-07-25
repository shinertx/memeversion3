use crate::config::CONFIG;
use anyhow::Result;
use reqwest::Client;
use shared_models::{MarketEvent, SocialMention, TwitterRawEvent};
use tokio::sync::mpsc;
use tracing::{error, info};
use std::time::Duration;

pub async fn run(tx: mpsc::Sender<MarketEvent>) -> Result<()> {
    info!("Starting Twitter Data Consumer...");
    let client = Client::new();
    let bearer_token = CONFIG.twitter_bearer_token.clone();

    loop {
        // Simulate Twitter data
        let tweet_id = format!("tweet_{}", rand::random::<u64>());
        let tokens = vec!["BONK", "WIF", "POPCAT", "MEW"];
        let token = tokens[rand::random::<usize>() % tokens.len()];
        
        if let Err(e) = tx.send(MarketEvent::TwitterRaw(TwitterRawEvent {
            tweet_id: tweet_id.clone(),
            text: format!("Just bought some ${} to the moon! 🚀", token),
            author_id: format!("user_{}", rand::random::<u32>() % 1000),
            timestamp: chrono::Utc::now().timestamp(),
        })).await { error!("Failed to send TwitterRawEvent: {}", e); }

        if let Err(e) = tx.send(MarketEvent::Social(SocialMention {
            token_address: token.to_string(),
            source: "twitter".to_string(),
            sentiment: rand::random::<f64>() * 2.0 - 1.0,
        })).await { error!("Failed to send SocialMention: {}", e); }

        tokio::time::sleep(Duration::from_secs(3)).await;
    }
}
