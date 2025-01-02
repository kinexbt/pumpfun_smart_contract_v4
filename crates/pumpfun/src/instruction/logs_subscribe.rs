use anchor_client::solana_client::{
    nonblocking::pubsub_client::PubsubClient,
    rpc_config::{RpcTransactionLogsConfig, RpcTransactionLogsFilter}
};

use anchor_client::solana_sdk::commitment_config::CommitmentConfig;

use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use futures::{future::BoxFuture, Future, StreamExt};

/// 订阅结果，包含订阅任务和取消订阅逻辑
pub struct SubscriptionHandle {
    pub task: JoinHandle<()>,
    pub unsub_fn: Box<dyn Fn() + Send>,
}

pub async fn create_pubsub_client(ws_url: &str) -> PubsubClient {
    PubsubClient::new(ws_url).await.unwrap()
}

/// 启动订阅
pub async fn start_subscription<F>(
    ws_url: &str,
    program_address: &str,
    commitment: CommitmentConfig,
    process_logs_callback: F,
) -> Result<SubscriptionHandle, Box<dyn std::error::Error>>
where
    F: Fn(String, Vec<String>) + Send + 'static,
{
    // 配置日志订阅
    let logs_config = RpcTransactionLogsConfig {
        commitment: Some(commitment),
    };
    let logs_filter = RpcTransactionLogsFilter::Mentions(vec![program_address.to_string()]);

    // 创建 PubsubClient
    let sub_client = Arc::new(create_pubsub_client(ws_url).await);

    let sub_client_clone = Arc::clone(&sub_client);

    // 创建一个通道用于取消订阅
    let (unsub_tx, mut unsub_rx) = mpsc::channel(1);

    // 启动订阅任务
    let task = tokio::spawn(async move {
        // let subs_client = Arc::clone(&sub_client);
        let (mut stream, unsub) = sub_client_clone.logs_subscribe(logs_filter, logs_config).await.unwrap();
        loop {
            tokio::select! {
                _ = unsub_rx.recv() => {
                    eprintln!("Received shutdown signal. Unsubscribing...");
                    unsub().await;
                    break;
                }
                msg = stream.next() => {
                    match msg {
                        println!("msg: {}", msg);
                        Some(msg) => {
                            if let Some(_err) = msg.value.err {
                                continue;
                            }
                            
                            println!("logs: {}", msg.value.logs);
                            process_logs_callback(msg.value.signature, msg.value.logs);
                        }
                        None => {
                            println!("Token subscription stream ended");
                            break;
                        }
                    }
                }
            }
        }
    });

    // 返回订阅句柄和取消逻辑
    Ok(SubscriptionHandle {
        task,
        unsub_fn: Box::new(move || {
            let _ = unsub_tx.try_send(()); // 发送取消信号
        }),
    })
}

pub async fn stop_subscription(handle: SubscriptionHandle) {
    (handle.unsub_fn)();
    handle.task.abort();
}
