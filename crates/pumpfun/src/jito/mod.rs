use std::fmt;
use std::str::FromStr;
use rand::seq::SliceRandom;
use reqwest::Client;
use serde_json::{json, Value};

use anchor_client::solana_sdk::{
    pubkey::Pubkey,
    transaction::Transaction,
};

use crate::error::{ClientError, ClientResult};

#[derive(Clone, Debug)]
pub struct JitoClient {
    base_url: String,
    uuid: Option<String>,
    client: Client,
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

impl JitoClient {
    pub fn new(base_url: &str, uuid: Option<String>) -> Self {
        Self {
            base_url: base_url.to_string(),
            uuid,
            client: Client::new(),
        }
    }

    async fn send_request(&self, endpoint: &str, method: &str, params: Option<Value>) -> ClientResult<Value> {
        let url = format!("{}{}", self.base_url, endpoint);
        
        let data = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": method,
            "params": params.unwrap_or(json!([]))
        });

        // println!("Sending request to: {}", url);
        // println!("Request body: {}", serde_json::to_string_pretty(&data).unwrap());

        let response = self.client
            .post(&url)
            .header("Content-Type", "application/json")
            .json(&data)
            .send()
            .await
            .map_err(|e| ClientError::Other(format!("Request failed: {}", e)))?;

        let status = response.status();
        // println!("Response status: {}", status);

        let body = response.json::<Value>().await
            .map_err(|e| ClientError::Other(format!("Failed to parse response: {}", e)))?;
        // println!("Response body: {}", serde_json::to_string_pretty(&body).unwrap());

        Ok(body)
    }

    pub async fn get_tip_accounts(&self) -> ClientResult<Value> {
        let endpoint = if let Some(uuid) = &self.uuid {
            format!("/bundles?uuid={}", uuid)
        } else {
            "/bundles".to_string()
        };

        self.send_request(&endpoint, "getTipAccounts", None).await
    }

    // Get a random tip account
    pub async fn get_tip_account(&self) -> ClientResult<Pubkey> {
        let tip_accounts_response = self.get_tip_accounts().await?;
        
        let tip_accounts = tip_accounts_response["result"]
            .as_array()
            .ok_or_else(|| ClientError::Other("Failed to parse tip accounts as array".to_string()))?;

        if tip_accounts.is_empty() {
            return Err(ClientError::Other("No tip accounts available".to_string()));
        }

        let random_account = tip_accounts
            .choose(&mut rand::thread_rng())
            .ok_or_else(|| ClientError::Other("Failed to choose random tip account".to_string()))?;

        let address = random_account
            .as_str()
            .ok_or_else(|| ClientError::Other("Failed to parse tip account as string".to_string()))?;

        Pubkey::from_str(address)
            .map_err(|e| ClientError::Other(format!("Failed to parse pubkey: {}", e)))
    }

    pub async fn get_bundle_statuses(&self, bundle_uuids: Vec<String>) -> ClientResult<Value> {
        let endpoint = if let Some(uuid) = &self.uuid {
            format!("/bundles?uuid={}", uuid)
        } else {
            "/bundles".to_string()
        };

        // Construct the params as a list within a list
        let params = json!([bundle_uuids]);

        self.send_request(&endpoint, "getBundleStatuses", Some(params))
            .await
    }

    pub async fn send_transaction(
        &self,
        transaction: &Transaction,
    ) -> ClientResult<String> {
        let wire_transaction = bincode::serialize(transaction).map_err(|e| {
            ClientError::Parse(
                "Transaction serialization failed".to_string(),
                e.to_string(),
            )
        })?;

        let serialized_tx = bs58::encode(&wire_transaction).into_string();

        // Prepare bundle for submission (array of transactions)
        let bundle = json!([serialized_tx]);

        // UUID for the bundle
        let uuid = None;

        // Send bundle using Jito SDK
        // println!("Sending bundle with 1 transaction...");
        let response = self.send_bundle(Some(bundle), uuid).await?;

        response["result"]
            .as_str()
            .map(|s| s.to_string())
            .ok_or_else(|| ClientError::Parse(
                "Invalid response format".to_string(),
                "Missing result field".to_string(),
            ))
    }

    pub async fn send_bundle(&self, params: Option<Value>, uuid: Option<&str>) -> ClientResult<Value> {
        let mut endpoint = "/bundles".to_string();
        
        if let Some(uuid) = uuid {
            endpoint = format!("{}?uuid={}", endpoint, uuid);
        }
    
        // Ensure params is an array of transactions
        let transactions = match params {
            Some(Value::Array(transactions)) => {
                if transactions.is_empty() {
                    return Err(ClientError::Other("Bundle must contain at least one transaction".to_string()));
                }
                if transactions.len() > 5 {
                    return Err(ClientError::Other("Bundle can contain at most 5 transactions".to_string()));
                }
                transactions
            },
            _ => return Err(ClientError::Other("Invalid bundle format: expected an array of transactions".to_string())),
        };
    
        // Wrap the transactions array in another array
        let params = json!([transactions]);
    
        // Send the wrapped transactions array
        self.send_request(&endpoint, "sendBundle", Some(params))
            .await
    }

    pub async fn send_txn(&self, params: Option<Value>, bundle_only: bool) -> ClientResult<Value> {
        let mut query_params = Vec::new();

        if bundle_only {
            query_params.push("bundleOnly=true".to_string());
        }

        let endpoint = if query_params.is_empty() {
            "/transactions".to_string()
        } else {
            format!("/transactions?{}", query_params.join("&"))
        };

        // Construct params as an array instead of an object
        let params = match params {
            Some(Value::Object(map)) => {
                let tx = map.get("tx").and_then(Value::as_str).unwrap_or_default();
                let skip_preflight = map.get("skipPreflight").and_then(Value::as_bool).unwrap_or(false);
                json!([
                    tx,
                    {
                        "encoding": "base64",
                        "skipPreflight": skip_preflight
                    }
                ])
            },
            _ => json!([]),
        };

        self.send_request(&endpoint, "sendTransaction", Some(params)).await
    }

    pub async fn get_in_flight_bundle_statuses(&self, bundle_uuids: Vec<String>) -> ClientResult<Value> {
        let endpoint = if let Some(uuid) = &self.uuid {
            format!("/bundles?uuid={}", uuid)
        } else {
            "/bundles".to_string()
        };

        // Construct the params as a list within a list
        let params = json!([bundle_uuids]);

        self.send_request(&endpoint, "getInflightBundleStatuses", Some(params))
            .await
    }

    // Helper method to convert Value to PrettyJsonValue
    pub fn prettify(value: Value) -> PrettyJsonValue {
        PrettyJsonValue(value)
    }
}