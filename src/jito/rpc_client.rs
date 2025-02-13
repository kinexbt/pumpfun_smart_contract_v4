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
/// A client of a remote Jito block engine node.
///
/// `RpcClient` communicates with a block engine node over [JSON-RPC]
/// It is the primary Rust interface for querying and transacting with the network
/// from external programs.
///
/// This is modeled very closely with the solan RpcClient with similar error types.
/// You can treat the client similar to the Solana RpcClient with the difference being
/// the RpcClient supports the block engine apis.
///
/// The client can be used with jito block engine proxy server for authentication
/// The client can be used as is to send requests unauthenticated to the jito block engine as well
///
/// Please note that the commitment level is not used at the moment. Support will be added
/// later on to specify and use the commitment levels.
pub struct RpcClient {
    sender: Box<dyn RpcSender + Send + Sync + 'static>,
    config: RpcClientConfig,
}

impl RpcClient {
    /// Create an `RpcClient` from an [`RpcSender`] and an [`RpcClientConfig`].
    ///
    /// This is the basic constructor, allowing construction with any type of
    /// `RpcSender`. Most applications should use one of the other constructors,
    /// such as [`RpcClient::new`], [`RpcClient::new_with_commitment`] or
    /// [`RpcClient::new_with_timeout`].
    pub fn new_sender<T: RpcSender + Send + Sync + 'static>(
        sender: T,
        config: RpcClientConfig,
    ) -> Self {
        Self {
            sender: Box::new(sender),
            config,
        }
    }

    /// Create an HTTP `RpcClient`.
    ///
    /// The URL is an HTTP URL, usually for port 8899, as in
    /// "http://localhost:8899".
    ///
    /// The client has a default timeout of 30 seconds, and a default [commitment
    /// level][cl] of [`Finalized`](CommitmentLevel::Finalized).
    ///
    /// [cl]: https://docs.solana.com/developing/clients/jsonrpc-api#configuring-state-commitment
    ///
    /// # Examples
    ///
    /// ```
    /// # use jito_block_engine_json_rpc_client::jsonrpc_client::rpc_client::RpcClient;
    /// let url = "http://localhost:8899".to_string();
    /// let client = RpcClient::new(url);
    /// ```
    pub fn new(url: String) -> Self {
        Self::new_with_commitment(url, CommitmentConfig::default())
    }

    /// Create an HTTP `RpcClient` with specified [commitment level][cl].
    ///
    /// Please note the client is not currently implemented to support commitment level configs
    ///
    /// [cl]: https://docs.solana.com/developing/clients/jsonrpc-api#configuring-state-commitment
    ///
    /// The URL is an HTTP URL, usually for port 8899, as in
    /// "http://localhost:8899".
    ///
    /// The client has a default timeout of 30 seconds, and a user-specified
    /// [`CommitmentLevel`] via [`CommitmentConfig`].
    ///
    /// # Examples
    ///
    /// # use solana_sdk::commitment_config::CommitmentConfig;
    /// # use jito_block_engine_json_rpc_client::jsonrpc_client::rpc_client::RpcClient;
    /// let url = "http://localhost:8899".to_string();
    /// let commitment_config = CommitmentConfig::processed();
    /// let client = RpcClient::new_with_commitment(url, commitment_config);
    fn new_with_commitment(url: String, commitment_config: CommitmentConfig) -> Self {
        Self::new_sender(
            HttpSender::new(url),
            RpcClientConfig::with_commitment(commitment_config),
        )
    }

    /// Create an HTTP `RpcClient` with specified timeout.
    ///
    /// The URL is an HTTP URL, usually for port 8899, as in
    /// "http://localhost:8899".
    ///
    /// The client has and a default [commitment level][cl] of
    /// [`Finalized`](CommitmentLevel::Finalized).
    ///
    /// [cl]: https://docs.solana.com/developing/clients/jsonrpc-api#configuring-state-commitment
    ///
    /// # Examples
    ///
    /// ```
    /// # use std::time::Duration;
    /// # use jito_block_engine_json_rpc_client::jsonrpc_client::rpc_client::RpcClient;
    /// let url = "http://localhost::8899".to_string();
    /// let timeout = Duration::from_secs(1);
    /// let client = RpcClient::new_with_timeout(url, timeout);
    /// ```
    pub fn new_with_timeout(url: String, timeout: Duration) -> Self {
        Self::new_sender(
            HttpSender::new_with_timeout(url, timeout),
            RpcClientConfig::with_commitment(CommitmentConfig::default()),
        )
    }

    /// Get the configured url of the client's sender
    pub fn url(&self) -> String {
        self.sender.url()
    }

    /// Get the configured default [commitment level][cl].
    ///
    /// [cl]: https://docs.solana.com/developing/clients/jsonrpc-api#configuring-state-commitment
    ///
    /// The commitment config may be specified during construction, and
    /// determines how thoroughly committed a transaction must be when waiting
    /// for its confirmation or otherwise checking for confirmation. If not
    /// specified, the default commitment level is
    /// [`Finalized`](CommitmentLevel::Finalized).
    ///
    /// The default commitment level is overridden when calling methods that
    /// explicitly provide a [`CommitmentConfig`], like
    /// [`RpcClient::confirm_transaction_with_commitment`].
    pub fn commitment(&self) -> CommitmentConfig {
        self.config.commitment_config
    }

    /// Submits a bundle of signed transactions to the network.
    ///
    /// This returns a bundle_id on success and will return an error
    /// code on failure. The error code will only be on the basic
    /// validation of the bundle and publishing the bundle to the mempool
    /// For the bundle status regarding whether it landed or not, get_bundle_statuses
    /// should be used with the bundle id.
    ///
    /// Example excerpt can be as below
    ///
    /// use jito_block_engine_json_rpc_client::jsonrpc_client::rpc_client::RpcClient;
    /// use solana_program::hash::Hash;
    /// use solana_sdk::{pubkey::Pubkey, signer::keypair::Keypair};
    ///
    /// let base = 0;
    /// let MAX_BUNDLE_LEN = 5;
    /// let searcher_keypair = Keypair::new();
    /// let recent_blockhash = Hash::new_unique();
    ///
    /// let mut bundle: Vec<_> = (0..(MAX_BUNDLE_LEN) as u64)
    ///     .map(|amount| {
    ///         VersionedTransaction::from(system_transaction::transfer(
    ///             &searcher_keypair,
    ///             &searcher_keypair.pubkey(),
    ///             base + amount,
    ///             recent_blockhash,
    ///         ))
    ///     })
    ///     .collect();
    ///
    /// let rpc_client = RpcClient::new(SERVER_URL.to_owned());
    /// let response = rpc_client.send_bundle(&bundle).await;
    pub async fn send_bundle(
        &self,
        transactions: &[impl SerializableTransaction],
    ) -> ClientResult<String> {
        let mut serialized_encoded: Vec<String> = Vec::with_capacity(transactions.len());
        for transaction in transactions {
            let encoding = self.default_cluster_transaction_encoding().await?;
            serialized_encoded.push(serialize_and_encode(transaction, encoding)?);
        }
        //The bundle may or may
        // not have been submitted to the cluster, so callers should verify the success of
        // the correct transaction signature independently.
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

    /// Gets the statuses of a list of bundle ids.
    ///
    /// Returns the statuses of a list of signatures. Each signature must be a bundle_id.
    /// bundle ids are sha256 hashes of their tx signatures (we get it after a sendBundle)
    /// This method currently will provide information regarding whether the bundle
    /// landed or not.
    /// The behavior is similar to the solana rpc method getSignatureStatuses
    /// https://docs.solana.com/api/http#getsignaturestatuses
    ///
    /// If the bundle_id is not found or the all of the transactions in the bundle has not landed,
    /// we return null. If found and landed, we return the context information including the slot
    /// at which the request was made and result with the bundle_id(s) and the transactions with the
    /// slot and confirmation status. At this point, its assumed that all transactions within a bundle
    /// will have the same slot number and confirmation status.
    ///
    /// The confirmation status of a bundle is the confirmation status of the transactions.
    /// This api does not provide a commitment level to configure, but will return the commitment level
    /// as returned by the rpc. The rpc used to fetch bulk transaction status does not provide a commitment
    /// level configuration option either.
    ///
    /// Example excerpt can be as below
    ///
    /// use jito_block_engine_json_rpc_client::jsonrpc_client::rpc_client::RpcClient;
    ///
    /// let SERVER_URL = "http://localhost:8899";
    /// let bundle_id = "bundle_id".to_owned();
    ///
    /// let rpc_client = RpcClient::new(SERVER_URL.to_owned());
    /// let response = rpc_client.get_bundle_statuses(&[bundle_id.clone()]).await;
    pub async fn get_bundle_statuses(
        &self,
        signatures: &[String],
    ) -> RpcResult<Vec<serde_json::Value>> {
        self.send(RpcRequest::GetBundlesStatuses, json!([signatures]))
            .await
    }

    /// Returns the tip accounts to be used for tip payments.
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

// Sample tests. Can use these as a reference point on how to use the api and what to expect
#[cfg(test)]
mod rpc_client_tests {
    use solana_program::hash::Hash;
    use solana_sdk::{
        pubkey::Pubkey, signature::Signer, signer::keypair::Keypair, system_transaction,
        transaction::VersionedTransaction,
    };

    use crate::jsonrpc_client::rpc_client::RpcClient;

    // Use the proxy server url here.
    const SERVER_URL: &str = "http://0.0.0.0:8080/api/v1/bundles";

    #[tokio::test]
    pub async fn get_tip_accounts() {
        // Let's try the same with the rpc client
        let rpc_client = RpcClient::new(SERVER_URL.to_owned());

        let tip_accounts = rpc_client.get_tip_accounts().await;

        // Sample output. Pick only randomly to not have contention
        // ["9ttgPBBhRYFuQccdR1DSnb7hydsWANoDsV3P9kaGMCEh",
        //  "EoW3SUQap7ZeynXQ2QJ847aerhxbPVr843uMeTfc9dxM",
        //  "4xgEmT58RwTNsF5xm2RMYCnR1EVukdK8a1i2qFjnJFu3",
        //  "B1mrQSpdeMU9gCvkJ6VsXVVoYjRGkNA7TtjMyqxrhecH",
        //  "aTtUk2DHgLhKZRDjePq6eiHRKC1XXFMBiSUfQ2JNDbN",
        //  "9n3d1K5YD2vECAbRFhFFGYNNjiXtHXJWn9F31t89vsAV",
        //  "ARTtviJkLLt6cHGQDydfo1Wyk6M4VGZdKZ2ZhdnJL336",
        //  "E2eSqe33tuhAHKTrwky5uEjaVqnb2T9ns6nHHUrN8588"]
        println!("{:?}", tip_accounts);
    }

    #[tokio::test]
    pub async fn send_bundle() {
        let rpc_client = RpcClient::new(SERVER_URL.to_owned());

        // Use your own keypair to sign
        let signer_keypair = Keypair::new();

        // Get the latest blockhash from solana cluster. Can use https://docs.solana.com/api/http#getlatestblockhash
        let recent_blockhash = Hash::new_unique();

        // Use the get_tip_accounts to randomly select a tip account to send tips to
        let tip_account = Pubkey::try_from("DCN82qDxJAQuSqHhv2BJuAgi41SPeKZB5ioBCTMNDrCC").unwrap();

        let mut bundle: Vec<_> = vec![VersionedTransaction::from(system_transaction::transfer(
            &signer_keypair,
            &signer_keypair.pubkey(),
            10000,
            recent_blockhash,
        ))];

        // Add the tip
        bundle.push(VersionedTransaction::from(system_transaction::transfer(
            &signer_keypair,
            &tip_account,
            10000,
            recent_blockhash,
        )));
        let response = rpc_client.send_bundle(&bundle).await;

        // If successful, the bundle_id can be retrieved. Else, an error code will be provided
        println!("{:?}", response);
    }

    #[tokio::test]
    pub async fn get_bundle_statuses() {
        let rpc_client = RpcClient::new(SERVER_URL.to_owned());

        // Use the bundle id you got from send_bundle
        let bundle_id =
            "6e4b90284778a40633b56e4289202ea79e62d2296bb3d45398bb93f6c9ec083d".to_owned();

        let response = rpc_client.get_bundle_statuses(&[bundle_id]).await;

        // Sample success output:
        // Response {
        //    context: RpcResponseContext {
        //      slot: 0, api_version: None },
        //    value: [Object {
        //      "bundle_id": String("6e4b90284778a40633b56e4289202ea79e62d2296bb3d45398bb93f6c9ec083d"),
        //      "transactions": Array [String("4DGCuaKc2oue4Z8YC6mBwyg3oPAFG64BfxDtMbqDU3Du9zr26oVSuZcjSnJqTnHnKYFJ4AdPuq5kUrWKwTFLKtW6"),
        //                             String("srrgfKABYeaKazZjBmpuPKySJ8qgqezYaCdDnB9nhED5CFhviZ1wgcs5vEKnAK9L2ytRauWG9czGoKRxajpZ1YR")],
        //      "slot": Number(240632575),
        //      "confirmation_status": String("finalized"),
        //      "err": Object {"Ok": Null}}] }
        //
        // Sample retryable error output:
        // Response {
        //    context: RpcResponseContext {
        //      slot: 0, api_version: None },
        //    value: [Object {
        //      "bundle_id": String("6e4b90284778a40633b56e4289202ea79e62d2296bb3d45398bb93f6c9ec083d"),
        //      "transactions": Array [String("4DGCuaKc2oue4Z8YC6mBwyg3oPAFG64BfxDtMbqDU3Du9zr26oVSuZcjSnJqTnHnKYFJ4AdPuq5kUrWKwTFLKtW6"),
        //                             String("srrgfKABYeaKazZjBmpuPKySJ8qgqezYaCdDnB9nhED5CFhviZ1wgcs5vEKnAK9L2ytRauWG9czGoKRxajpZ1YR")],
        //      "slot": Number(612529),
        //      "confirmation_status": Null,
        //      "err": Object {"Err": Object {"Retryable": String("Failed to retrieve information from solana cluster")}}}] }
        // If unknown bundle, the response would be null
        println!("{:?}", response);
    }
}
