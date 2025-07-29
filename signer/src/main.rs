use anyhow::{anyhow, Result};
use axum::{extract::State, http::StatusCode, routing::{get, post}, Json, Router};
use shared_models::{SignRequest, SignResponse};
use solana_sdk::{
    signature::{read_keypair_file, Keypair, Signer},
    transaction::VersionedTransaction,
    message::VersionedMessage,
};
use std::{env, net::SocketAddr, sync::Arc};
use tracing::{error, info, instrument, level_filters::LevelFilter};
use tracing_subscriber::EnvFilter;
use base64::{Engine as _, engine::general_purpose};

struct AppState {
    keypair: Keypair,
}

#[tokio::main]
async fn main() -> Result<()> {
    let filter = EnvFilter::builder()
        .with_default_directive(LevelFilter::INFO.into())
        .from_env_lossy();
    tracing_subscriber::fmt().with_env_filter(filter).init();

    info!("🔒 Starting Signer Service v24...");

    let keypair_filename = env::var("WALLET_KEYPAIR_FILENAME").expect("WALLET_KEYPAIR_FILENAME must be set");
    let keypair_path = format!("/app/wallet/{}", keypair_filename);
    let keypair = read_keypair_file(&keypair_path)
        .map_err(|e| anyhow!("Failed to read keypair at {}: {}", keypair_path, e))?;
    
    let pubkey = keypair.pubkey();
    info!(%pubkey, "Wallet loaded successfully.");

    let state = Arc::new(AppState { keypair });

    let app = Router::new()
        .route("/pubkey", get(get_pubkey))
        .route("/sign", post(sign_transaction))
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], 8989));
    info!("Listening on http://{}", addr);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

#[instrument(skip(state), name="get_pubkey_handler")]
async fn get_pubkey(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    Json(serde_json::json!({ "pubkey": state.keypair.pubkey().to_string() }))
}

#[instrument(skip(state, request), name="sign_transaction_handler")]
async fn sign_transaction(
    State(state): State<Arc<AppState>>,
    Json(request): Json<SignRequest>,
) -> Result<Json<SignResponse>, StatusCode> {
    let tx_bytes = match general_purpose::STANDARD.decode(&request.transaction_b64) {
        Ok(bytes) => bytes,
        Err(e) => {
            error!(error = %e, "Failed to decode base64 transaction");
            return Err(StatusCode::BAD_REQUEST);
        }
    };

    let mut tx: VersionedTransaction = match bincode::deserialize(&tx_bytes) {
        Ok(tx) => tx,
        Err(e) => {
            error!(error = %e, "Failed to deserialize transaction");
            return Err(StatusCode::BAD_REQUEST);
        }
    };

    // Get the message to sign
    let message = match &tx.message {
        VersionedMessage::Legacy(legacy) => legacy.serialize(),
        VersionedMessage::V0(v0) => v0.serialize(),
    };

    // Sign the transaction with partial signatures  
    tx.signatures[0] = state.keypair.sign_message(&message);

    let signed_tx_bytes = match bincode::serialize(&tx) {
        Ok(bytes) => bytes,
        Err(e) => {
            error!(error = %e, "Failed to serialize signed transaction");
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    info!("Transaction signed successfully.");
    Ok(Json(SignResponse {
        signed_transaction_b64: general_purpose::STANDARD.encode(&signed_tx_bytes),
    }))
}
