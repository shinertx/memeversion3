use std::time::Duration;
use tokio::time::sleep;
use tracing::info;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();
    
    info!("Position Manager v25 starting up...");
    
    // Keep service running
    loop {
        info!("Position manager heartbeat");
        sleep(Duration::from_secs(30)).await;
    }
}
