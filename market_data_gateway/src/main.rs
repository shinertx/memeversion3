use crate::providers::{self, farcaster_consumer, helius_consumer, pyth_consumer, twitter_consumer};
use anyhow::Result;
use std::sync::Arc;
use tokio::sync::Mutex;

mod providers;

#[tokio::main]
async fn main() -> Result<()> {
    let tx = Arc::new(Mutex::new(()));
    // Initialize and spawn data providers
    tokio::spawn(helius_consumer::run(tx.clone()));
    tokio::spawn(pyth_consumer::run(tx.clone()));
    tokio::spawn(twitter_consumer::run(tx.clone()));
    tokio::spawn(farcaster_consumer::run(tx.clone()));

    Ok(())
}