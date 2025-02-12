use std::{convert::TryInto, future::Future, str::FromStr, time::Duration, fmt};

use anyhow::{anyhow, Result};
use api::TipAccountResult;
use rand::{rng, seq::IteratorRandom};
use reqwest::Client;
use serde::Deserialize;
use serde_json::{json, Value};
use solana_sdk::{pubkey::Pubkey, transaction::Transaction};
use tokio::{
    sync::RwLock,
    time::{sleep, Instant},
};
use tracing::{debug, error, info, warn};

use crate::error::{ClientError, ClientResult};

pub mod api;

#[derive(Debug)]
pub struct JitoClient {
    base_url: String,
    tip_accounts: RwLock<Vec<String>>,
    tips_percentile: RwLock<Option<TipPercentileData>>,
    tip_percentile: String,
    uuid: Option<String>,
    client: Client,
}

impl Clone for JitoClient {
    fn clone(&self) -> Self {
        Self {
            base_url: self.base_url.clone(),
            tip_accounts: RwLock::new(Vec::new()),
            tips_percentile: RwLock::new(None),
            tip_percentile: self.tip_percentile.clone(),
            uuid: self.uuid.clone(),
            client: Client::new(),
        }
    }
}

#[derive(Debug)]
pub struct PrettyJsonValue(pub Value);

impl fmt::Display for PrettyJsonValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", serde_json::to_string_pretty(&self.0).unwrap())
    }
}

impl From<Value> for PrettyJsonValue {
    fn from(value: Value) -> Self {
        PrettyJsonValue(value)
    }
}

const DEFAULT_TIP_PERCENTILE: &str = "25";

#[derive(Debug, Deserialize, Clone)]
pub struct TipPercentileData {
    pub time: String,
    pub landed_tips_25th_percentile: f64,
    pub landed_tips_50th_percentile: f64,
    pub landed_tips_75th_percentile: f64,
    pub landed_tips_95th_percentile: f64,
    pub landed_tips_99th_percentile: f64,
    pub ema_landed_tips_50th_percentile: f64,
}

#[derive(Deserialize, Debug)]
pub struct BundleStatus {
    pub bundle_id: String,
    pub transactions: Vec<String>,
    pub slot: u64,
    pub confirmation_status: String,
    pub err: ErrorStatus,
}

#[derive(Deserialize, Debug)]
pub struct ErrorStatus {
    #[serde(rename = "Ok")]
    pub ok: Option<()>,
}

impl JitoClient {
    pub fn new(base_url: &str, uuid: Option<String>) -> Self {
        Self {
            base_url: base_url.to_string(),
            tip_accounts: RwLock::new(vec![]),
            tips_percentile: RwLock::new(None),
            tip_percentile: DEFAULT_TIP_PERCENTILE.to_string(),
            uuid,
            client: Client::new(),
        }
    }

    pub async fn get_tip_accounts(&self) -> Result<TipAccountResult> {
        let endpoint = if let Some(uuid) = &self.uuid {
            format!("/bundles?uuid={}", uuid)
        } else {
            "/bundles".to_string()
        };

        let result = self.send_request(&endpoint, "getTipAccounts", None).await?;
        let tip_accounts = TipAccountResult::from(result).map_err(|e| anyhow!(e))?;
        Ok(tip_accounts)
    }

    async fn send_request(&self, endpoint: &str, method: &str, params: Option<Value>) -> Result<Value> {
        let url = format!("{}{}", self.base_url, endpoint);
        
        let data = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": method,
            "params": params.unwrap_or(json!([]))
        });

        let response = self.client
            .post(&url)
            .header("Content-Type", "application/json")
            .json(&data)
            .send()
            .await
            .map_err(|e| anyhow!(format!("Request failed: {}", e)))?;

        let body = response.json::<Value>().await
            .map_err(|e| anyhow!(format!("Failed to parse response: {}", e)))?;

        Ok(body)
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
        // 第一次尝试获取读锁
        {
            let accounts = self.tip_accounts.read().await;
            if !accounts.is_empty() {
                let mut rng = rng();
                if let Some(acc) = accounts.iter().choose(&mut rng) {
                    return Ok(Pubkey::from_str(acc).inspect_err(|err| {
                        error!("jito: failed to parse Pubkey: {:?}", err);
                    })?);
                }
            }
        } // 这里释放读锁

        // 如果账户列表为空，初始化账户列表
        self.init_tip_accounts().await?;

        // 重新获取读锁
        let accounts = self.tip_accounts.read().await;
        let mut rng = rng();
        match accounts.iter().choose(&mut rng) {
            Some(acc) => Ok(Pubkey::from_str(acc).inspect_err(|err| {
                error!("jito: failed to parse Pubkey: {:?}", err);
            })?),
            None => Err(anyhow!("jito: no tip accounts available")),
        }
    }

    pub async fn init_tip_amounts(&self) -> Result<()> {
        let tip_percentiles = api::get_tip_amounts().await?;
        *self.tips_percentile.write().await = tip_percentiles.first().cloned();

        Ok(())
    }

    // unit sol
    pub async fn get_tip_value(&self) -> Result<f64> {
        let tips = self.tips_percentile.read().await;

        if let Some(ref data) = *tips {
            match self.tip_percentile.as_str() {
                "25" => Ok(data.landed_tips_25th_percentile),
                "50" => Ok(data.landed_tips_50th_percentile),
                "75" => Ok(data.landed_tips_75th_percentile),
                "95" => Ok(data.landed_tips_95th_percentile),
                "99" => Ok(data.landed_tips_99th_percentile),
                _ => Err(anyhow!("jito: invalid TIP_PERCENTILE value")),
            }
        } else {
            Err(anyhow!("jito: failed get tip"))
        }
    }

    pub async fn send_transaction(
        &self,
        transaction: &Transaction,
    ) -> Result<String, anyhow::Error> {
        let wire_transaction = bincode::serialize(transaction).map_err(|e| {
            anyhow!(
                "Transaction serialization failed: {}",
                e.to_string(),
            )
        })?;

        let serialized_tx = bs58::encode(&wire_transaction).into_string();

        // Prepare bundle for submission (array of transactions)
        let bundle = json!([serialized_tx]);

        // UUID for the bundle
        let uuid = self.uuid.clone();

        // Send bundle using Jito SDK
        let response = self.send_bundle(Some(bundle), uuid).await?;

        response["result"]
            .as_str()
            .map(|s| s.to_string())
            .ok_or_else(|| anyhow!("Invalid response format: missing result field"))
    }

    pub async fn send_bundle(&self, params: Option<Value>, uuid: Option<String>) -> Result<Value> {
        let mut endpoint = "/bundles".to_string();
        
        if let Some(uuid) = uuid {
            endpoint = format!("{}?uuid={}", endpoint, uuid);
        }
    
        // Ensure params is an array of transactions
        let transactions = match params {
            Some(Value::Array(transactions)) => {
                if transactions.is_empty() {
                    return Err(anyhow!("Bundle must contain at least one transaction"));
                }
                if transactions.len() > 5 {
                    return Err(anyhow!("Bundle can contain at most 5 transactions"));
                }
                transactions
            },
            _ => return Err(anyhow!("Invalid bundle format: expected an array of transactions")),
        };
    
        // Wrap the transactions array in another array
        let params = json!([transactions]);
    
        // Send the wrapped transactions array
        self.send_request(&endpoint, "sendBundle", Some(params))
            .await
            .map_err(|e| anyhow!(e))
    }

    pub async fn wait_for_bundle_confirmation<F, Fut>(
        &self,
        fetch_statuses: F,
        bundle_id: String,
        interval: Duration,
        timeout: Duration,
    ) -> Result<Vec<String>>
    where
        F: Fn(String) -> Fut,
        Fut: Future<Output = Result<Vec<Value>>>,
    {
        let start_time = Instant::now();

        loop {
            let statuses = fetch_statuses(bundle_id.clone()).await?;

            if let Some(status) = statuses.first() {
                let bundle_status: BundleStatus =
                    serde_json::from_value(status.clone()).inspect_err(|err| {
                        error!(
                            "Failed to parse JSON when get_bundle_statuses, err: {}",
                            err,
                        );
                    })?;

                debug!("{:?}", bundle_status);
                match bundle_status.confirmation_status.as_str() {
                    "finalized" | "confirmed" => {
                        info!(
                            "Finalized bundle {}: {}",
                            bundle_id, bundle_status.confirmation_status
                        );
                        bundle_status
                            .transactions
                            .iter()
                            .for_each(|tx| info!("https://solscan.io/tx/{}", tx));
                        return Ok(bundle_status.transactions);
                    }
                    _ => {
                        debug!("bundle_status: {:?}", bundle_status);
                    }
                }
            } else {
                debug!("Finalizing bundle {}: {}", bundle_id, "None");
            }

            if start_time.elapsed() > timeout {
                warn!("Loop exceeded {:?}, breaking out.", timeout);
                return Err(anyhow!("Bundle status get timeout"));
            }

            sleep(interval).await;
        }
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
