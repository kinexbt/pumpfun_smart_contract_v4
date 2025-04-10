use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use crate::swqos::jito_grpc::{
    bundle::{
        Bundle, BundleResult,
    },
    convert::proto_packet_from_versioned_tx,
    searcher::{
        searcher_service_client::SearcherServiceClient, SendBundleRequest, SubscribeBundleResultsRequest,
    },
};
use solana_sdk::{
    signature::Signature,
    transaction::VersionedTransaction,
};
use thiserror::Error;
use tokio::sync::Mutex;
use tonic::{transport::{self, Channel, Endpoint}, Status};
use yellowstone_grpc_client::ClientTlsConfig;

use crate::swqos::common::poll_transaction_confirmation;
use crate::common::SolanaRpcClient;

#[derive(Debug, Error)]
pub enum BlockEngineConnectionError {
    #[error("transport error {0}")]
    TransportError(#[from] transport::Error),
    #[error("client error {0}")]
    ClientError(#[from] Status),
}

#[derive(Debug, Error)]
pub enum BundleRejectionError {
    #[error("bundle lost state auction, auction: {0}, tip {1} lamports")]
    StateAuctionBidRejected(String, u64),
    #[error("bundle won state auction but failed global auction, auction {0}, tip {1} lamports")]
    WinningBatchBidRejected(String, u64),
    #[error("bundle simulation failure on tx {0}, message: {1:?}")]
    SimulationFailure(String, Option<String>),
    #[error("internal error {0}")]
    InternalError(String),
}

pub type BlockEngineConnectionResult<T> = Result<T, BlockEngineConnectionError>;

pub async fn get_searcher_client_no_auth(
    block_engine_url: &str,
) -> BlockEngineConnectionResult<SearcherServiceClient<Channel>> {
    let searcher_channel = create_grpc_channel(block_engine_url).await?;
    let searcher_client = SearcherServiceClient::new(searcher_channel);
    Ok(searcher_client)
}

pub async fn create_grpc_channel(url: &str) -> BlockEngineConnectionResult<Channel> {
    let mut endpoint = Endpoint::from_shared(url.to_string()).expect("invalid url");
    if url.starts_with("https") {
        endpoint = endpoint.tls_config(ClientTlsConfig::new().with_native_roots())?;
    }

    endpoint = endpoint.tcp_nodelay(true);
    endpoint = endpoint.tcp_keepalive(Some(Duration::from_secs(10)));
    endpoint = endpoint.connect_timeout(Duration::from_secs(20));
    endpoint = endpoint.http2_keep_alive_interval(Duration::from_secs(10));

    Ok(endpoint.connect().await?)
}

pub async fn subscribe_bundle_results(
    searcher_client: Arc<Mutex<SearcherServiceClient<Channel>>>,
    request: impl tonic::IntoRequest<SubscribeBundleResultsRequest>,
) -> std::result::Result<
    tonic::Response<tonic::codec::Streaming<BundleResult>>,
    tonic::Status,
> {
    let mut searcher = searcher_client.lock().await;
    searcher.subscribe_bundle_results(request).await
}

pub async fn send_bundle_with_confirmation(
    rpc: Arc<SolanaRpcClient>,
    transactions: &Vec<VersionedTransaction>,
    searcher_client: Arc<Mutex<SearcherServiceClient<Channel>>>,
) -> Result<Vec<Signature>, anyhow::Error> {
    let mut signatures = send_bundle_no_wait(transactions, searcher_client).await?;

    let timeout: Duration = Duration::from_secs(10);
    let start_time: Instant = Instant::now();
    while Instant::now().duration_since(start_time) < timeout {
        for signature in signatures.clone() {
            match poll_transaction_confirmation(&rpc, signature).await {
                Ok(sig) => signatures.push(sig),
                Err(_) => continue,
            }
        }
    }

    Ok(signatures)
}

pub async fn send_bundle_no_wait(
    transactions: &Vec<VersionedTransaction>,
    searcher_client: Arc<Mutex<SearcherServiceClient<Channel>>>,
) -> Result<Vec<Signature>, anyhow::Error> {
    let mut packets = vec![];
    let mut signatures = vec![];
    for transaction in transactions {
        let packet = proto_packet_from_versioned_tx(transaction);
        packets.push(packet);
        signatures.push(transaction.signatures[0]);
    }

    let mut searcher = searcher_client.lock().await;
    searcher
        .send_bundle(SendBundleRequest {
            bundle: Some(Bundle {
                header: None,
                packets,
            }),
        })
        .await?;

    Ok(signatures)
}
