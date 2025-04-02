use bincode::serialize;
use serde_json::json;
use solana_client::rpc_client::SerializableTransaction;
use solana_sdk::signature::Signature;
use solana_sdk::transaction::Transaction;
use solana_transaction_status::{TransactionConfirmationStatus, UiTransactionEncoding};
use std::str::FromStr;
use std::time::{Duration, Instant};
use tokio::time::sleep;
use crate::common::types::SolanaRpcClient;
use anyhow::Result;
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use reqwest::Client;

pub async fn poll_transaction_confirmation(rpc: &SolanaRpcClient, txt_sig: Signature) -> Result<Signature> {
    // 15 second timeout
    let timeout: Duration = Duration::from_secs(15);
    // 5 second retry interval
    let interval: Duration = Duration::from_secs(5);
    let start: Instant = Instant::now();

    loop {
        if start.elapsed() >= timeout {
            return Err(anyhow::anyhow!("Transaction {}'s confirmation timed out", txt_sig));
        }

        let status = rpc.get_signature_statuses(&[txt_sig]).await?;

        match status.value[0].clone() {
            Some(status) => {
                if status.err.is_none()
                    && (status.confirmation_status == Some(TransactionConfirmationStatus::Confirmed)
                        || status.confirmation_status == Some(TransactionConfirmationStatus::Finalized))
                {
                    return Ok(txt_sig);
                }
                if status.err.is_some() {
                    return Err(anyhow::anyhow!(status.err.unwrap()));
                }
            }
            None => {
                sleep(interval).await;
            }
        }
    }
}

pub async fn send_nb_transaction(client: Client, endpoint: &str, auth_token: &str, transaction: &Transaction) -> Result<Signature, anyhow::Error> {
    // 序列化交易
    let serialized = bincode::serialize(transaction)
        .map_err(|e| anyhow::anyhow!("序列化交易失败: {}", e))?;
    
    // Base64编码
    let encoded = STANDARD.encode(serialized);

    let request_data = json!({
        "transaction": {
            "content": encoded
        },
        "frontRunningProtection": true
    });

    let url = format!("{}/api/v2/submit", endpoint);
    let response = client
        .post(url)
        .header("Authorization", auth_token)
        .header("Content-Type", "application/json")
        .json(&request_data)
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("请求失败: {}", e))?;

    let resp = response.json::<serde_json::Value>().await
        .map_err(|e| anyhow::anyhow!("解析响应失败: {}", e))?;

    if let Some(reason) = resp["reason"].as_str() {
        return Err(anyhow::anyhow!(reason.to_string()));
    }

    let signature = resp["signature"].as_str()
        .ok_or_else(|| anyhow::anyhow!("响应中缺少signature字段"))?;

    let signature = Signature::from_str(signature)
        .map_err(|e| anyhow::anyhow!("无效的签名: {}", e))?;

    Ok(signature)
}

pub async fn serialize_and_encode(
    transaction: &Vec<u8>,
    encoding: UiTransactionEncoding,
) -> Result<String> {
    let serialized = match encoding {
        UiTransactionEncoding::Base58 => bs58::encode(transaction).into_string(),
        UiTransactionEncoding::Base64 => STANDARD.encode(transaction),
        _ => return Err(anyhow::anyhow!("Unsupported encoding")),
    };
    Ok(serialized)
}

pub async fn serialize_transaction_and_encode(
    transaction: &impl SerializableTransaction,
    encoding: UiTransactionEncoding,
) -> Result<String> {
    let serialized_tx = serialize(transaction)?;
    let serialized = match encoding {
        UiTransactionEncoding::Base58 => bs58::encode(serialized_tx).into_string(),
        UiTransactionEncoding::Base64 => STANDARD.encode(serialized_tx),
        _ => return Err(anyhow::anyhow!("Unsupported encoding")),
    };
    Ok(serialized)
}

pub async fn serialize_smart_transaction_and_encode(
    transaction: &impl SerializableTransaction,
    encoding: UiTransactionEncoding,
) -> Result<(String, Signature)> {
    let signature = transaction.get_signature();
    let serialized_tx = serialize(transaction)?;
    let serialized = match encoding {
        UiTransactionEncoding::Base58 => bs58::encode(serialized_tx).into_string(),
        UiTransactionEncoding::Base64 => STANDARD.encode(serialized_tx),
        _ => return Err(anyhow::anyhow!("Unsupported encoding")),
    };
    Ok((serialized, *signature))
}