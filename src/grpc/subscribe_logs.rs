#[cfg(test)]
mod subscribe_tx_tests {
    use crate::common::{
        event::{PumpEvent, RaydiumEvent, SwapBaseInLog, TradeEvent},
        myerror::AppError,
        yellowstone_grpc::{TransactionPretty, YellowstoneGrpc},
    };
    use anyhow::anyhow;
    use chrono::Local;
    use dotenvy::dotenv;
    use futures::{channel::mpsc, sink::SinkExt, stream::StreamExt};
    use log::{error, info};
    use solana_sdk::pubkey;
    use solana_sdk::pubkey::Pubkey;
    use solana_transaction_status::{
        option_serializer::OptionSerializer, EncodedTransactionWithStatusMeta,
    };
    use std::env;
    use tokio::test;
    use yellowstone_grpc_proto::geyser::{
        subscribe_update::UpdateOneof, SubscribeRequest, SubscribeRequestPing,
    };
    const AMM_V4: Pubkey = pubkey!("675kPX9MHTjS2zt1qfr1NYHuzeLXfQM9H24wFSUt1Mp8");
    const PUMP_PROGRAM_ID: Pubkey = pubkey!("6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P");
    pub enum SwapType {
        Pump,
        Raydium,
    }

    #[test]
    async fn test_subscribe_tx() -> Result<(), AppError> {
        dotenv().ok();
        pretty_env_logger::init_custom_env("RUST_LOG");
        let yellowstone_url = env::var("YELLOWSTONE_URL")?;

        info!("::: {:?}", yellowstone_url);
        let yellowstone_grpc = YellowstoneGrpc::new(yellowstone_url);

        let addrs = vec![
            "Aa4QWNkS3RLUv7DA9BM1a2Hzm4HDQo5PyRefqDJnpump".to_string(),
            "BnDssYyGDF9aj5j2N5BwsJFk9YMneQ8P7LQkoYkrpump".to_string(),
        ];
        let transactions = yellowstone_grpc.subscribe_transaction(addrs, vec![], vec![]);

        let (mut subscribe_tx, mut stream) = yellowstone_grpc.connect(transactions).await??;

        let (mut tx, mut rx) = mpsc::channel::<TransactionPretty>(1000);

        tokio::spawn(async move {
            while let Some(message) = stream.next().await {
                match message {
                    Ok(msg) => {
                        match msg.update_oneof {
                            Some(UpdateOneof::Transaction(sut)) => {
                                let transaction_pretty: TransactionPretty = sut.into();
                                let _ = tx.try_send(transaction_pretty);
                            }
                            Some(UpdateOneof::Ping(_)) => {
                                // This is necessary to keep load balancers that expect client pings alive. If your load balancer doesn't
                                // require periodic client pings then this is unnecessary
                                let _ = subscribe_tx
                                    .send(SubscribeRequest {
                                        ping: Some(SubscribeRequestPing { id: 1 }),
                                        ..Default::default()
                                    })
                                    .await;
                                info!("service is ping: {}", Local::now());
                            }
                            Some(UpdateOneof::Pong(_)) => {
                                info!("service is pong: {}", Local::now());
                            }
                            _ => {}
                        }
                        continue;
                    }
                    Err(error) => {
                        error!("error: {error:?}");
                        break;
                    }
                }
            }
        });

        while let Some(transaction_pretty) = rx.next().await {
            let trade_raw = transaction_pretty.tx.clone();
            let meta = &trade_raw.meta.clone().unwrap();
            if meta.err.is_some() {
                continue;
            }

            let logs = if let OptionSerializer::Some(logs) = &meta.log_messages {
                logs
            } else {
                &vec![]
            };

            if let Ok(swap_type) = get_swap_type(&trade_raw) {
                match swap_type {
                    SwapType::Raydium => {
                        let event = RaydiumEvent::parse_logs::<SwapBaseInLog>(logs);
                        info!("RaydiumEvent {:#?}", event);
                    }
                    SwapType::Pump => {
                        let event = PumpEvent::parse_logs::<TradeEvent>(logs);
                        info!("PumpEvent {:#?}", event);
                    }
                }
            };
        }
        Ok(())
    }

    #[allow(dead_code)]
    pub fn get_swap_type(
        trade_raw: &EncodedTransactionWithStatusMeta,
    ) -> Result<SwapType, AppError> {
        let transaction = &trade_raw.transaction.decode();
        if let Some(transaction) = transaction {
            let account_keys = transaction.message.static_account_keys();

            let program_index = account_keys
                .iter()
                .position(|item| item == &AMM_V4 || item == &PUMP_PROGRAM_ID)
                .ok_or(anyhow!("swap type program_id not found"))?;

            let program_id = account_keys[program_index];
            let _type = match program_id {
                AMM_V4 => Ok(SwapType::Raydium),
                PUMP_PROGRAM_ID => Ok(SwapType::Pump),
                _ => Err(AppError::from(anyhow!("program_id ix not found"))),
            };
            return _type;
        }
        Err(AppError::from(anyhow!("program_id ix not found")))
    }
}