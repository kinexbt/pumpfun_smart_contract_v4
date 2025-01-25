use solana_client::{
    nonblocking::pubsub_client::PubsubClient,
    rpc_config::{RpcTransactionLogsConfig, RpcTransactionLogsFilter}
};

use solana_sdk::{commitment_config::CommitmentConfig, pubkey::Pubkey};
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use futures::StreamExt;
use crate::{constants, common::{
    logs_data::DexInstruction, logs_events::DexEvent, logs_filters::LogFilter
}};

use super::logs_events::PumpfunEvent;

/// Subscription handle containing task and unsubscribe logic
pub struct SubscriptionHandle {
    pub task: JoinHandle<()>,
    pub unsub_fn: Box<dyn Fn() + Send>,
}

impl SubscriptionHandle {
    pub async fn shutdown(self) {
        (self.unsub_fn)();
        self.task.abort();
    }
}

pub async fn create_pubsub_client(ws_url: &str) -> PubsubClient {
    PubsubClient::new(ws_url).await.unwrap()
}

/// 启动订阅
pub async fn tokens_subscription<F>(
    ws_url: &str,
    commitment: CommitmentConfig,
    callback: F,
    bot_wallet: Option<Pubkey>,
) -> Result<SubscriptionHandle, Box<dyn std::error::Error>>
where
    F: Fn(PumpfunEvent) + Send + Sync + 'static,
{
    let program_address = constants::accounts::PUMPFUN.to_string();
    let logs_filter = RpcTransactionLogsFilter::Mentions(vec![program_address]);

    let logs_config = RpcTransactionLogsConfig {
        commitment: Some(commitment),
    };

    // Create PubsubClient
    let sub_client = Arc::new(PubsubClient::new(ws_url).await.unwrap());

    let sub_client_clone = Arc::clone(&sub_client);

    // Create channel for unsubscribe
    let (unsub_tx, _) = mpsc::channel(1);

    // Start subscription task
    let task = tokio::spawn(async move {
        let (mut stream, _) = sub_client_clone.logs_subscribe(logs_filter, logs_config).await.unwrap();

        loop {
            let msg = stream.next().await;
            match msg {
                Some(msg) => {
                    if let Some(_err) = msg.value.err {
                        continue;
                    }

                    let instructions = LogFilter::parse_instruction(&msg.value.logs, bot_wallet).unwrap();
                    for instruction in instructions {
                        match instruction {
                            DexInstruction::CreateToken(token_info) => {
                                callback(PumpfunEvent::NewToken(token_info));
                            }
                            DexInstruction::UserTrade(trade_info) => {
                                callback(PumpfunEvent::NewUserTrade(trade_info));
                            }
                            DexInstruction::BotTrade(trade_info) => {
                                callback(PumpfunEvent::NewBotTrade(trade_info));
                            }
                            _ => {}
                        }
                    }
                }
                None => {
                    println!("Token subscription stream ended");
                }
            }   
        }
    });

    // Return subscription handle and unsubscribe logic
    Ok(SubscriptionHandle {
        task,
        unsub_fn: Box::new(move || {
            let _ = unsub_tx.try_send(());
        }),
    })
}

pub async fn stop_subscription(handle: SubscriptionHandle) {
    handle.shutdown().await;
}
