use std::{str::FromStr, time::Duration, fmt};

use anyhow::{anyhow, Result};
use api::TipAccountResult;
use rand::{seq::IteratorRandom};
use serde::Deserialize;
use serde_json::{json, Value};
use solana_sdk::{
    pubkey::Pubkey,
    transaction::{Transaction, VersionedTransaction},
};
use tokio::sync::RwLock;
use tracing::error;

pub mod api;
pub mod client_error;
pub mod http_sender;
pub mod request;
pub mod rpc_client;
pub mod rpc_sender;

use crate::jito::rpc_client::RpcClient;

pub struct JitoClient {
    base_url: String,
    tip_accounts: RwLock<Vec<String>>,
    tips_percentile: RwLock<Option<TipPercentileData>>,
    tip_percentile: String,
    client: RpcClient,
}

impl Clone for JitoClient {
    fn clone(&self) -> Self {
        Self {
            base_url: self.base_url.clone(),
            tip_accounts: RwLock::new(Vec::new()),
            tips_percentile: RwLock::new(None),
            tip_percentile: self.tip_percentile.clone(),
            client: RpcClient::new(self.base_url.clone()),
        }
    }
}

const DEFAULT_TIP_PERCENTILE: &str = "25";
pub struct TipPercentileData {
    pub time: String,
    pub landed_tips_25th_percentile: f64,
    pub landed_tips_50th_percentile: f64,
    pub landed_tips_75th_percentile: f64,
    pub landed_tips_95th_percentile: f64,
    pub landed_tips_99th_percentile: f64,
    pub ema_landed_tips_50th_percentile: f64,
}

impl JitoClient {
    pub fn new(jito_url: &str, uuid: Option<String>) -> Self {
        Self {
            base_url: jito_url.to_string(),
            tip_accounts: RwLock::new(vec![]),
            tips_percentile: RwLock::new(None),
            tip_percentile: DEFAULT_TIP_PERCENTILE.to_string(),
            client: RpcClient::new(jito_url.to_string()),
        }
    }

    pub async fn get_tip_accounts(&self) -> Result<TipAccountResult> {
        let result = self.client.get_tip_accounts().await?;
        let tip_accounts = TipAccountResult::from(result).map_err(|e| anyhow!(e))?;
        Ok(tip_accounts)
    }

    pub async fn init_tip_accounts(&self) -> Result<()> {
        let accounts = self.get_tip_accounts().await?;
        let mut tip_accounts = self.tip_accounts.write().await;

        accounts
            .accounts
            .iter()
            .for_each(|account| tip_accounts.push(account.to_string()));
        Ok(())
    }

    pub async fn get_tip_account(&self) -> Result<Pubkey> {
        {
            let accounts = self.tip_accounts.read().await;
            if !accounts.is_empty() {
                if let Some(acc) = accounts.iter().choose(&mut rand::thread_rng()) {
                    return Ok(Pubkey::from_str(acc).inspect_err(|err| {
                        error!("jito: failed to parse Pubkey: {:?}", err);
                    })?);
                }
            }
        }

        self.init_tip_accounts().await?;

        let accounts = self.tip_accounts.read().await;
        match accounts.iter().choose(&mut rand::thread_rng()) {
            Some(acc) => Ok(Pubkey::from_str(acc).inspect_err(|err| {
                error!("jito: failed to parse Pubkey: {:?}", err);
            })?),
            None => Err(anyhow!("jito: no tip accounts available")),
        }
    }

    pub async fn send_transaction(
        &self,
        transaction: &Transaction,
    ) -> Result<String, anyhow::Error> {
        let bundles = vec![VersionedTransaction::from(transaction.clone())];
        Ok(self.client.send_bundle(&bundles).await?)
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;
    use serde_json::{json, Value};
    use super::*;

    fn generate_statuses(bundle_id: String, confirmation_status: &str) -> Vec<Value> {
        vec![json!({
            "bundle_id": bundle_id,
            "transactions": ["tx1", "tx2"],
            "slot": 12345,
            "confirmation_status": confirmation_status,
            "err": {"Ok": null}
        })]
    }

    #[tokio::test]
    async fn test_success_confirmation() {
        let client = JitoClient::new("http://localhost:8899", None);
        for &status in &["finalized", "confirmed"] {
            let wait_result = client.wait_for_bundle_confirmation(
                |id| async { Ok(generate_statuses(id, status)) },
                "6e4b90284778a40633b56e4289202ea79e62d2296bb3d45398bb93f6c9ec083d".to_string(),
                Duration::from_secs(1),
                Duration::from_secs(1),
            )
            .await;
            assert!(wait_result.is_ok());
        }
    }

    #[tokio::test]
    async fn test_error_confirmation() {
        let client = JitoClient::new("http://localhost:8899", None);
        let wait_result = client.wait_for_bundle_confirmation(
            |id| async { Ok(generate_statuses(id, "processed")) },
            "6e4b90284778a40633b56e4289202ea79e62d2296bb3d45398bb93f6c9ec083d".to_string(),
            Duration::from_secs(1),
            Duration::from_secs(2),
        )
        .await;
        assert!(wait_result.is_err());
    }
}
