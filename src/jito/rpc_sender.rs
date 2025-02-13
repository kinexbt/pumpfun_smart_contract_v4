use async_trait::async_trait;
use solana_rpc_client::rpc_sender::RpcTransportStats;

use crate::jito::{client_error::Result, request::RpcRequest};

/// A transport for RPC calls.
///
/// `RpcSender` implements the underlying transport of requests to, and
/// responses from, a Solana node, and is used primarily by [`RpcClient`].
///
/// [`RpcClient`]: crate::rpc_client::RpcClient
#[async_trait]
pub trait RpcSender {
    async fn send(
        &self,
        request: RpcRequest,
        params: serde_json::Value,
    ) -> Result<serde_json::Value>;
    fn get_transport_stats(&self) -> RpcTransportStats;
    fn url(&self) -> String;
}
