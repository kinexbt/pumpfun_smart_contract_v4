use std::time::Duration;

use bincode::serialize;
use log::*;
use serde_json::{json, Value};
use solana_rpc_client::{
    rpc_client::{RpcClientConfig, SerializableTransaction},
    rpc_sender::RpcTransportStats,
};
use solana_rpc_client_api::{
    client_error::ErrorKind as ClientErrorKind, request::RpcError, response::Response,
};
use solana_sdk::{bs58, commitment_config::CommitmentConfig};
use solana_transaction_status::UiTransactionEncoding;

use crate::jito::{
    client_error,
    client_error::{Error as ClientError, Result as ClientResult},
    http_sender::HttpSender,
    request::RpcRequest,
    rpc_sender::*,
};

pub type RpcResult<T> = client_error::Result<Response<T>>;

pub struct RpcClient {
    sender: Box<dyn RpcSender + Send + Sync + 'static>,
    config: RpcClientConfig,
}

impl RpcClient {
    pub fn new_sender<T: RpcSender + Send + Sync + 'static>(
        sender: T,
        config: RpcClientConfig,
    ) -> Self {
        Self {
            sender: Box::new(sender),
            config,
        }
    }

    pub fn new(url: String) -> Self {
        Self::new_with_commitment(url, CommitmentConfig::default())
    }

    fn new_with_commitment(url: String, commitment_config: CommitmentConfig) -> Self {
        Self::new_sender(
            HttpSender::new(url),
            RpcClientConfig::with_commitment(commitment_config),
        )
    }

    pub fn new_with_timeout(url: String, timeout: Duration) -> Self {
        Self::new_sender(
            HttpSender::new_with_timeout(url, timeout),
            RpcClientConfig::with_commitment(CommitmentConfig::default()),
        )
    }

    pub fn url(&self) -> String {
        self.sender.url()
    }

    pub fn commitment(&self) -> CommitmentConfig {
        self.config.commitment_config
    }

    pub async fn send_bundle(
        &self,
        transactions: &[impl SerializableTransaction],
    ) -> ClientResult<String> {
        let mut serialized_encoded: Vec<String> = Vec::with_capacity(transactions.len());
        for transaction in transactions {
            let encoding = self.default_cluster_transaction_encoding().await?;
            serialized_encoded.push(serialize_and_encode(transaction, encoding)?);
        }
        match self
            .send(RpcRequest::SendBundle, json!([serialized_encoded]))
            .await
        {
            Ok(signature_base58_str) => ClientResult::Ok(signature_base58_str),
            Err(err) => {
                if let ClientErrorKind::RpcError(RpcError::RpcResponseError {
                    code, message, ..
                }) = &err.kind
                {
                    debug!("{} {}", code, message);
                }
                Err(err)
            }
        }
    }

    async fn default_cluster_transaction_encoding(
        &self,
    ) -> Result<UiTransactionEncoding, RpcError> {
        Ok(UiTransactionEncoding::Base58)
    }

    pub async fn get_bundle_statuses(
        &self,
        signatures: &[String],
    ) -> RpcResult<Vec<serde_json::Value>> {
        self.send(RpcRequest::GetBundlesStatuses, json!([signatures]))
            .await
    }

    pub async fn get_tip_accounts(&self) -> ClientResult<Vec<String>> {
        self.send(RpcRequest::GetTipAccounts, Value::Null).await
    }

    pub async fn send<T>(&self, request: RpcRequest, params: Value) -> ClientResult<T>
    where
        T: serde::de::DeserializeOwned,
    {
        assert!(params.is_array() || params.is_null());

        let response = self
            .sender
            .send(request, params)
            .await
            .map_err(|err| err.into_with_request(request))?;
        serde_json::from_value(response)
            .map_err(|err| ClientError::new_with_request(err.into(), request))
    }

    pub fn get_transport_stats(&self) -> RpcTransportStats {
        self.sender.get_transport_stats()
    }
}

fn serialize_and_encode<T>(input: &T, encoding: UiTransactionEncoding) -> ClientResult<String>
where
    T: serde::ser::Serialize,
{
    let serialized = serialize(input)
        .map_err(|e| ClientErrorKind::Custom(format!("Serialization failed: {e}")))?;
    let encoded = match encoding {
        UiTransactionEncoding::Base58 => bs58::encode(serialized).into_string(),
        _ => {
            return Err(ClientErrorKind::Custom(format!(
                "unsupported encoding: {encoding}. Supported encodings: base58"
            ))
            .into())
        }
    };
    Ok(encoded)
}

#[cfg(test)]
mod rpc_client_tests {
    use solana_program::hash::Hash;
    use solana_sdk::{
        pubkey::Pubkey, signature::Signer, signer::keypair::Keypair, system_transaction,
        transaction::VersionedTransaction,
    };

    use crate::jito::rpc_client::RpcClient;

    const SERVER_URL: &str = "http://0.0.0.0:8080/api/v1/bundles";

    #[tokio::test]
    pub async fn get_tip_accounts() {
        let rpc_client = RpcClient::new(SERVER_URL.to_owned());
        let tip_accounts = rpc_client.get_tip_accounts().await;
        println!("{:?}", tip_accounts);
    }

    #[tokio::test]
    pub async fn send_bundle() {
        let rpc_client = RpcClient::new(SERVER_URL.to_owned());
        let signer_keypair = Keypair::new();
        let recent_blockhash = Hash::new_unique();
        let tip_account = Pubkey::try_from("DCN82qDxJAQuSqHhv2BJuAgi41SPeKZB5ioBCTMNDrCC").unwrap();

        let mut bundle: Vec<_> = vec![VersionedTransaction::from(system_transaction::transfer(
            &signer_keypair,
            &signer_keypair.pubkey(),
            10000,
            recent_blockhash,
        ))];

        bundle.push(VersionedTransaction::from(system_transaction::transfer(
            &signer_keypair,
            &tip_account,
            10000,
            recent_blockhash,
        )));
        let response = rpc_client.send_bundle(&bundle).await;
        println!("{:?}", response);
    }

    #[tokio::test]
    pub async fn get_bundle_statuses() {
        let rpc_client = RpcClient::new(SERVER_URL.to_owned());
        let bundle_id =
            "6e4b90284778a40633b56e4289202ea79e62d2296bb3d45398bb93f6c9ec083d".to_owned();
        let response = rpc_client.get_bundle_statuses(&[bundle_id]).await;
        println!("{:?}", response);
    }
}
