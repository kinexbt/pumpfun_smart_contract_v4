use {
    bincode, bs58, reqwest, serde::Deserialize, serde_json::{json, Value}, 
    std::{str::FromStr, time::Duration}
};

use anchor_client::solana_sdk::{
    commitment_config::CommitmentConfig, pubkey::Pubkey, signature::Signature, transaction::Transaction
};

use crate::error::ClientError::{self, *};

/// 常量定义
pub const MAX_RETRIES: u8 = 3;             
pub const RETRY_DELAY: Duration = Duration::from_millis(200);  

/// 交易配置
#[derive(Debug, Clone)]
pub struct TransactionConfig {
    pub skip_preflight: bool,
    pub preflight_commitment: CommitmentConfig,
    pub encoding: String,
    pub last_n_blocks: u64,
}

impl Default for TransactionConfig {
    fn default() -> Self {
        Self {
            skip_preflight: true,  // Jito 建议跳过预检
            preflight_commitment: CommitmentConfig::confirmed(),
            encoding: "base58".to_string(),
            last_n_blocks: 100,
        }
    }
}
#[derive(Clone)]
pub struct JitoClient {
    endpoint: String,
    client: reqwest::Client,
    config: TransactionConfig,
}

impl JitoClient {
    pub fn new(endpoint: &str) -> Self {
        Self {
            endpoint: endpoint.to_string(),
            client: reqwest::Client::new(),
            config: TransactionConfig::default(),
        }
    }

    /// 获取随机的 tip account
    pub async fn get_tip_account(&self) -> Result<Pubkey, ClientError> {
        let response = self.send_request("getTipAccounts", json!([])).await?;
        
        if let Some(accounts) = response["result"].as_array() {
            if accounts.is_empty() {
                return Err(ClientError::Other(
                    "No JITO tip accounts found".to_string()
                ));
            }
            
            let random_index = rand::random::<usize>() % accounts.len();
            if let Some(account) = accounts.get(random_index) {
                if let Some(address) = account.as_str() {
                    return Pubkey::from_str(address)
                        .map_err(|e| ClientError::Parse(
                            "Invalid tip account address".to_string(),
                            e.to_string()
                        ));
                }
            }
        }
        
        Err(ClientError::Other("Failed to get Tip Account".to_string()))
    }

    /// 估算优先费用
    pub async fn estimate_priority_fees(
        &self,
        account: &Pubkey,
    ) -> Result<PriorityFeeEstimate, ClientError> {
        let params = json!({
            "last_n_blocks": self.config.last_n_blocks,
            "account": account.to_string(),
            "api_version": 2
        });

        let response = self.send_request("qn_estimatePriorityFees", params).await?;
        
        // 解析响应
        if let Some(result) = response.get("result") {
            let estimate: PriorityFeeEstimate = serde_json::from_value(result.clone())
                .map_err(|e| ClientError::Parse(
                    "Failed to parse priority fee estimate".to_string(),
                    e.to_string()
                ))?;
            
            Ok(estimate)
        } else {
            Err(ClientError::Parse(
                "Invalid response format".to_string(),
                "Missing result field".to_string()
            ))
        }
    }

    /// 发送交易
    pub async fn send_transaction(
        &self,
        transaction: &Transaction,
    ) -> Result<Signature, ClientError> {
        let wire_transaction = bincode::serialize(transaction).map_err(|e| 
            ClientError::Parse(
                "Transaction serialization failed".to_string(),
                e.to_string()
            ))?;
        
        let encoded_tx = bs58::encode(&wire_transaction).into_string();

        for retry in 0..MAX_RETRIES {
            match self.try_send_transaction(&encoded_tx).await {
                Ok(signature) => {
                    return Ok(Signature::from_str(&signature).map_err(|e| 
                        ClientError::Parse(
                            "Invalid signature".to_string(),
                            e.to_string()
                        ))?);
                },
                Err(e) => {
                    println!("Retry {} failed: {:?}", retry, e);
                    if retry == MAX_RETRIES - 1 {
                        return Err(e);
                    }
                    tokio::time::sleep(RETRY_DELAY).await;
                }
            }
        }
        
        Err(ClientError::Other("Max retries exceeded".to_string()))
    }

    async fn try_send_transaction(&self, encoded_tx: &str) -> Result<String, ClientError> {
        let params = json!([
            encoded_tx,
            {
                "skipPreflight": self.config.skip_preflight,
                "preflightCommitment": self.config.preflight_commitment.commitment,
                "encoding": self.config.encoding,
                "maxRetries": MAX_RETRIES,
                "minContextSlot": null
            }
        ]);

        let response = self.send_request(
            "sendTransaction",
            params
        ).await?;

        response["result"]
            .as_str()
            .map(|s| s.to_string())
            .ok_or_else(|| ClientError::Parse(
                "Invalid response format".to_string(),
                "Missing result field".to_string()
            ))
    }

    async fn send_request(&self, method: &str, params: Value) -> Result<Value, ClientError> {
        let request_body = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": method,
            "params": params
        });

        let response = self.client
            .post(&self.endpoint)
            .header("Content-Type", "application/json")
            .json(&request_body)
            .send()
            .await
            .map_err(|e| ClientError::Solana(
                "Request failed".to_string(),
                e.to_string()
            ))?;

        let response_data: Value = response.json().await
            .map_err(|e| ClientError::Parse(
                "Invalid JSON response".to_string(),
                e.to_string()
            ))?;
        
        if let Some(error) = response_data.get("error") {
            return Err(ClientError::Solana(
                "RPC error".to_string(),
                error.to_string()
            ));
        }

        Ok(response_data)
    }
}

#[derive(Debug, Deserialize)]
pub struct PriorityFeeEstimate {
    pub recommended: u64,
    pub per_compute_unit: PriorityFeeLevel,
    pub per_transaction: PriorityFeeLevel,
}

#[derive(Debug, Deserialize)]
pub struct PriorityFeeLevel {
    pub extreme: u64,  // 95th percentile
    pub high: u64,     // 80th percentile
    pub medium: u64,   // 60th percentile
    pub low: u64,      // 40th percentile
}
