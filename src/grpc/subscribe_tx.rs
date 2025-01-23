#[cfg(test)]
mod subscribe_tx_tests {
    use crate::common::{
        myerror::AppError,
        yellowstone_grpc::{TransactionPretty, YellowstoneGrpc},
    };
    use chrono::Local;
    use dotenvy::dotenv;
    use futures::{channel::mpsc, sink::SinkExt, stream::StreamExt};
    use log::{error, info};
    use std::env;
    use tokio::test;
    use yellowstone_grpc_proto::geyser::{
        subscribe_update::UpdateOneof, SubscribeRequest, SubscribeRequestPing,
    };

    #[test]
    async fn test_subscribe_tx() -> Result<(), AppError> {
        dotenv().ok();
        pretty_env_logger::init_custom_env("RUST_LOG");
        let yellowstone_url = env::var("YELLOWSTONE_URL")?;

        info!("::: {:?}", yellowstone_url);
        let yellowstone_grpc = YellowstoneGrpc::new(yellowstone_url);

        let addrs = vec!["6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P".to_string()];
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

        while let Some(event) = rx.next().await {
            info!("TransactionPretty {:#?}", event);
        }
        Ok(())
    }
}