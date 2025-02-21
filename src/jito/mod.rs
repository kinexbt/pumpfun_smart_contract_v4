use std::str::FromStr;

use anyhow::{anyhow, Result};
use api::TipAccountResult;
use rand::seq::IteratorRandom;
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
    client: RpcClient,
}

impl Clone for JitoClient {
    fn clone(&self) -> Self {
        Self {
            base_url: self.base_url.clone(),
            tip_accounts: RwLock::new(Vec::new()),
            client: RpcClient::new(self.base_url.clone()),
        }
    }
}

impl JitoClient {
    pub fn new(jito_url: &str, _uuid: Option<String>) -> Self {
        Self {
            base_url: jito_url.to_string(),
            tip_accounts: RwLock::new(vec![]),
            client: RpcClient::new(jito_url.to_string()),
        }
    }

    pub async fn get_tip_accounts(&self) -> Result<TipAccountResult> {
        let result = self.client.get_tip_accounts().await?;
        TipAccountResult::from(result).map_err(|e| anyhow!(e))
    }

    pub async fn init_tip_accounts(&self) -> Result<()> {
        let accounts = self.get_tip_accounts().await?;
        let mut tip_accounts = self.tip_accounts.write().await;
        *tip_accounts = accounts.accounts.iter().map(|a| a.to_string()).collect();
        Ok(())
    }

    pub async fn get_tip_account(&self) -> Result<Pubkey> {
        {
            let accounts = self.tip_accounts.read().await;
            if !accounts.is_empty() {
                if let Some(acc) = accounts.iter().choose(&mut rand::rng()) {
                    return Pubkey::from_str(acc)
                        .map_err(|err| {
                            error!("jito: failed to parse Pubkey: {:?}", err);
                            anyhow!("Invalid pubkey format")
                        });
                }
            }
        }

        self.init_tip_accounts().await?;

        let accounts = self.tip_accounts.read().await;
        accounts
            .iter()
            .choose(&mut rand::rng())
            .ok_or_else(|| anyhow!("jito: no tip accounts available"))
            .and_then(|acc| {
                Pubkey::from_str(acc).map_err(|err| {
                    error!("jito: failed to parse Pubkey: {:?}", err);
                    anyhow!("Invalid pubkey format")
                })
            })
    }

    pub async fn send_transaction(
        &self,
        transaction: &Transaction,
    ) -> Result<String, anyhow::Error> {
        let bundles = vec![VersionedTransaction::from(transaction.clone())];
        Ok(self.client.send_bundle(&bundles).await?)
    }

    pub async fn send_transactions(
        &self,
        transactions: &Vec<Transaction>,
    ) -> Result<String, anyhow::Error> {
        let bundles: Vec<VersionedTransaction> = transactions.iter()
        .map(|t| VersionedTransaction::from(t.clone()))
        .collect();  // 显式指定类型
        Ok(self.client.send_bundle(&bundles).await?)
    }
}
