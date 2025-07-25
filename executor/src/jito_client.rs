use anyhow::{anyhow, Context, Result};
use solana_sdk::{
    commitment_config::CommitmentConfig,
    hash::Hash,
    signature::{read_keypair_file, Signature, Signer},
    transaction::VersionedTransaction,
};
use std::sync::Arc;
use tracing::info;

pub struct JitoClient {
    auth_keypair: Arc<solana_sdk::signature::Keypair>,
    rpc_client: solana_client::nonblocking::rpc_client::RpcClient,
}

impl JitoClient {
    pub async fn new(jito_rpc_url: &str) -> Result<Self> {
        let auth_keypair_path = crate::config::CONFIG.jito_auth_keypair_filename.clone();
        let auth_keypair = Arc::new(
            read_keypair_file(&format!("/app/wallet/{}", auth_keypair_path))
                .map_err(|e| anyhow!("Failed to read Jito auth keypair from {}: {}", auth_keypair_path, e))?
        );
        
        let rpc_client = solana_client::nonblocking::rpc_client::RpcClient::new_with_commitment(
            crate::config::CONFIG.solana_rpc_url.clone(),
            CommitmentConfig::confirmed(),
        );

        info!("Jito client initialized successfully.");
        Ok(Self { auth_keypair, rpc_client })
    }

    pub async fn get_recent_blockhash(&self) -> Result<Hash> {
        self.rpc_client.get_latest_blockhash().await.context("Failed to get recent blockhash from RPC")
    }

    pub async fn attach_tip(&self, tx: &mut VersionedTransaction, tip_lamports: u64) -> Result<()> {
        // In a real implementation, you would modify the transaction to include a tip
        // This is a simplified placeholder
        info!("Jito tip attachment of {} lamports simulated.", tip_lamports);
        Ok(())
    }

    pub async fn send_transaction(&self, tx: &VersionedTransaction) -> Result<Signature> {
        // In a real implementation, this would send to Jito's block engine
        // For now, sending to regular RPC
        let sig = self.rpc_client.send_transaction(tx).await?;
        info!("Transaction sent. Signature: {}", sig);
        Ok(sig)
    }
}
