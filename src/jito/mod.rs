use api::api_client::ApiClient;
use common::{poll_transaction_confirmation, serialize_smart_transaction_and_encode};
use jito_protos::{searcher::searcher_service_client::SearcherServiceClient, shredstream::shredstream_client::ShredstreamClient};
use reqwest::Client;
use searcher_client::{get_searcher_client_no_auth, send_bundle_with_confirmation};
use serde_json::json;
use tonic::transport::Channel;
use tracing::instrument::WithSubscriber;
use yellowstone_grpc_client::Interceptor;
use std::{sync::Arc, time::Instant};
use tokio::sync::{Mutex, RwLock};

use solana_sdk::signature::Signature;

use std::str::FromStr;
use rustls::crypto::{ring::default_provider, CryptoProvider};

use tonic::{service::interceptor::InterceptedService, transport::Uri, Status};         
use std::time::Duration;
use solana_transaction_status::UiTransactionEncoding;
use tonic::transport::ClientTlsConfig;

use anyhow::{anyhow, Result};
use rand::{rng, seq::{IndexedRandom, IteratorRandom}};
use solana_sdk::transaction::VersionedTransaction;

use crate::{common::SolanaRpcClient, constants::accounts::{JITO_TIP_ACCOUNTS, NEXTBLOCK_TIP_ACCOUNTS, ZEROSLOT_TIP_ACCOUNTS}};

pub mod common;
pub mod searcher_client;
pub mod api;

lazy_static::lazy_static! {
    static ref TIP_ACCOUNT_CACHE: RwLock<Vec<String>> = RwLock::new(Vec::new());
}

#[derive(Debug, Clone, Copy)]
pub enum ClientType {
    Jito,
    NextBlock,
    ZeroSlot,
}

pub type FeeClient = dyn FeeClientTrait + Send + Sync + 'static;

#[async_trait::async_trait]
pub trait FeeClientTrait {
    async fn send_transaction(&self, transaction: &VersionedTransaction) -> Result<Signature>;
    async fn send_transactions(&self, transactions: &Vec<VersionedTransaction>) -> Result<Vec<Signature>>;
    async fn get_tip_account(&self) -> Result<String>;
    async fn get_client_type(&self) -> ClientType;
}

pub struct JitoClient {
    pub rpc_client: Arc<SolanaRpcClient>,
    pub searcher_client: Arc<Mutex<SearcherServiceClient<Channel>>>,
}

#[async_trait::async_trait]
impl FeeClientTrait for JitoClient {
    async fn send_transaction(&self, transaction: &VersionedTransaction) -> Result<Signature, anyhow::Error> {
        self.send_bundle_with_confirmation(&vec![transaction.clone()]).await?.first().cloned().ok_or(anyhow!("Failed to send transaction"))
    }

    async fn send_transactions(&self, transactions: &Vec<VersionedTransaction>) -> Result<Vec<Signature>, anyhow::Error> {
        self.send_bundle_with_confirmation(transactions).await
    }

    async fn get_tip_account(&self) -> Result<String, anyhow::Error> {
        if let Some(acc) = JITO_TIP_ACCOUNTS.iter().choose(&mut rng()) {
            Ok(acc.to_string())
        } else {
            Err(anyhow!("no valid tip accounts found"))
        }
    }

    async fn get_client_type(&self) -> ClientType {
        ClientType::Jito
    }
}

impl JitoClient {
    pub async fn new(rpc_url: String, block_engine_url: String) -> Result<Self> {
        let rpc_client = SolanaRpcClient::new(rpc_url);
        let searcher_client = get_searcher_client_no_auth(block_engine_url.as_str()).await?;
        Ok(Self { rpc_client: Arc::new(rpc_client), searcher_client: Arc::new(Mutex::new(searcher_client)) })
    }
    
    pub async fn send_bundle_with_confirmation(
        &self,
        transactions: &Vec<VersionedTransaction>,
    ) -> Result<Vec<Signature>, anyhow::Error> {
        send_bundle_with_confirmation(self.rpc_client.clone(), &transactions, self.searcher_client.clone()).await
    }

    pub async fn send_bundle_no_wait(
        &self,
        transactions: &Vec<VersionedTransaction>,
    ) -> Result<Vec<Signature>, anyhow::Error> {
        searcher_client::send_bundle_no_wait(&transactions, self.searcher_client.clone()).await
    }

    // pub async fn get_tip_accounts(&self) -> Result<Vec<String>, anyhow::Error> {
    //     let client = ShredstreamClient::connect("dst").await?;
    //     // let subscriber = Dispatch::new(tracing_subscriber::fmt::Subscriber::builder().finish());
    //     let subscriber = tracing::subscriber::set_global_default(tracing_subscriber::fmt::Subscriber::builder().finish()).unwrap();
    //     let aaa = client.with_subscriber(subscriber);
       
    //     let mut stream = client.subscribe_accounts_of_interest(tonic::Request::new(()));
    //     let mut accounts = Vec::new();
    //     while let Some(Ok(response)) = stream.next().await {
    //         accounts.extend(response.accounts);
    //     }
    //     Ok(accounts)
    // }
}

#[derive(Clone)]
pub struct MyInterceptor {
    auth_token: String,
}

impl MyInterceptor {
    pub fn new(auth_token: String) -> Self {
        Self { auth_token }
    }
}

impl Interceptor for MyInterceptor {
    fn call(&mut self, mut request: tonic::Request<()>) -> Result<tonic::Request<()>, Status> {
        request.metadata_mut().insert(
            "authorization", 
            tonic::metadata::MetadataValue::from_str(&self.auth_token)
                .map_err(|_| Status::invalid_argument("Invalid auth token"))?
        );
        Ok(request)
    }
}

#[derive(Clone)]
pub struct NextBlockClient {
    pub rpc_client: Arc<SolanaRpcClient>,
    pub client: ApiClient<InterceptedService<Channel, MyInterceptor>>,
}

#[async_trait::async_trait]
impl FeeClientTrait for NextBlockClient {
    async fn send_transaction(&self, transaction: &VersionedTransaction) -> Result<Signature, anyhow::Error> {
        self.send_transaction(transaction).await
    }

    async fn send_transactions(&self, transactions: &Vec<VersionedTransaction>) -> Result<Vec<Signature>, anyhow::Error> {
        self.send_transactions(transactions).await
    }

    async fn get_tip_account(&self) -> Result<String> {
        let tip_account = self.get_tip_account().await?;
        Ok(tip_account)
    }

    async fn get_client_type(&self) -> ClientType {
        ClientType::NextBlock
    }
}

impl NextBlockClient {
    pub fn new(rpc_url: String, endpoint: String, auth_token: String) -> Self {
        if CryptoProvider::get_default().is_none() {
            let _ = default_provider()
                .install_default()
                .map_err(|e| anyhow::anyhow!("Failed to install crypto provider: {:?}", e));
        }

        let endpoint = endpoint.parse::<Uri>().unwrap();
        let tls = ClientTlsConfig::new().with_native_roots();
        let channel = Channel::builder(endpoint)
            .tls_config(tls).expect("Failed to create TLS config")
            .tcp_keepalive(Some(Duration::from_secs(60)))
            .http2_keep_alive_interval(Duration::from_secs(30))
            .keep_alive_while_idle(true)
            .timeout(Duration::from_secs(30))
            .connect_timeout(Duration::from_secs(10))
            .connect_lazy();

        let client = ApiClient::with_interceptor(channel, MyInterceptor::new(auth_token));
        let rpc_client = SolanaRpcClient::new(rpc_url);
        Self { rpc_client: Arc::new(rpc_client), client }
    }

    pub async fn send_transaction(&self, transaction: &VersionedTransaction) -> Result<Signature, anyhow::Error> {
        let (content, signature) = serialize_smart_transaction_and_encode(transaction, UiTransactionEncoding::Base64).await?;
        
        self.client.clone().post_submit_v2(api::PostSubmitRequest {
            transaction: Some(api::TransactionMessage {
                content,
                is_cleanup: false,
            }),
            skip_pre_flight: true,
            front_running_protection: Some(true),
            experimental_front_running_protection: Some(true),
            snipe_transaction: Some(true),
        }).await?;

        let timeout: Duration = Duration::from_secs(10);
        let start_time: Instant = Instant::now();
        while Instant::now().duration_since(start_time) < timeout {
            match poll_transaction_confirmation(&self.rpc_client, signature).await {
                Ok(sig) => return Ok(sig),
                Err(_) => continue,
            }
        }

        Ok(signature)
    }

    pub async fn send_transactions(&self, transactions: &Vec<VersionedTransaction>) -> Result<Vec<Signature>, anyhow::Error> {
        let mut entries = Vec::new();
        let encoding = UiTransactionEncoding::Base64;
        
        let mut signatures = Vec::new();
        for transaction in transactions {
            let (content, signature) = serialize_smart_transaction_and_encode(transaction, encoding).await?;
            entries.push(api::PostSubmitRequestEntry {
                transaction: Some(api::TransactionMessage {
                    content,
                    is_cleanup: false,
                }),
                skip_pre_flight: true,
            });
            signatures.push(signature);
        }

        self.client.clone().post_submit_batch_v2(api::PostSubmitBatchRequest {
            entries,
            submit_strategy: api::SubmitStrategy::PSubmitAll as i32,
            use_bundle: Some(true),
            front_running_protection: Some(true),
        }).await?;

        let timeout: Duration = Duration::from_secs(10);
        let start_time: Instant = Instant::now();
        while Instant::now().duration_since(start_time) < timeout {
            for signature in signatures.clone() {
                match poll_transaction_confirmation(&self.rpc_client, signature).await {
                    Ok(sig) => signatures.push(sig),
                    Err(_) => continue,
                }
            }
        }

        Ok(signatures)
    }

    async fn get_tip_account(&self) -> Result<String> {
        let tip_account = *NEXTBLOCK_TIP_ACCOUNTS.choose(&mut rand::rng()).or_else(|| NEXTBLOCK_TIP_ACCOUNTS.first()).unwrap();
        Ok(tip_account.to_string())
    }
}

#[derive(Clone)]
pub struct ZeroSlotClient {
    pub endpoint: String,
    pub auth_token: String,
    pub rpc_client: Arc<SolanaRpcClient>,
}

#[async_trait::async_trait]
impl FeeClientTrait for ZeroSlotClient {
    async fn send_transaction(&self, transaction: &VersionedTransaction) -> Result<Signature, anyhow::Error> {
        self.send_transaction(transaction).await
    }

    async fn send_transactions(&self, transactions: &Vec<VersionedTransaction>) -> Result<Vec<Signature>, anyhow::Error> {
        self.send_transactions(transactions).await
    }

    async fn get_tip_account(&self) -> Result<String> {
        let tip_account = self.get_tip_account().await?;
        Ok(tip_account)
    }

    async fn get_client_type(&self) -> ClientType {
        ClientType::ZeroSlot
    }
}

impl ZeroSlotClient {
    pub fn new(rpc_url: String, endpoint: String, auth_token: String) -> Self {
        let rpc_client = SolanaRpcClient::new(rpc_url);
        Self { rpc_client: Arc::new(rpc_client), endpoint, auth_token }
    }

    pub async fn send_transaction(&self, transaction: &VersionedTransaction) -> Result<Signature, anyhow::Error> {
        let (content, signature) = serialize_smart_transaction_and_encode(transaction, UiTransactionEncoding::Base64).await?;
        
        let client = Client::new();
        let request_body = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "sendTransaction",
            "params": [
                content,
                {
                    "encoding": "base64",
                    "skipPreflight": true,
                }
            ]
        });

        // Send the request
        let response = client.post(format!("{}/?api-key={}", self.endpoint, self.auth_token))
            .json(&request_body)
            .send()
            .await?;

        // Parse the response
        let response_json: serde_json::Value = response.json().await?;
        if let Some(result) = response_json.get("result") {
            println!("Transaction sent successfully: {}", result);
        } else if let Some(error) = response_json.get("error") {
            eprintln!("Failed to send transaction: {}", error);
        }

        let timeout: Duration = Duration::from_secs(10);
        let start_time: Instant = Instant::now();
        while Instant::now().duration_since(start_time) < timeout {
            match poll_transaction_confirmation(&self.rpc_client, signature).await {
                Ok(sig) => return Ok(sig),
                Err(_) => continue,
            }
        }

        Ok(signature)
    }

    pub async fn send_transactions(&self, transactions: &Vec<VersionedTransaction>) -> Result<Vec<Signature>, anyhow::Error> {
        let mut signatures = Vec::new();
        for transaction in transactions {
            let signature = self.send_transaction(transaction).await?;
            signatures.push(signature);
        }
        Ok(signatures)
    }

    async fn get_tip_account(&self) -> Result<String> {
        let tip_account = *ZEROSLOT_TIP_ACCOUNTS.choose(&mut rand::rng()).or_else(|| NEXTBLOCK_TIP_ACCOUNTS.first()).unwrap();
        Ok(tip_account.to_string())
    }
}