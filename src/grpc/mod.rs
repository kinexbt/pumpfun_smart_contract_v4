use std::{collections::HashMap, fmt, time::Duration};

use futures::{channel::mpsc, sink::Sink, Stream, StreamExt, SinkExt};
use rustls::crypto::{ring::default_provider, CryptoProvider};
use tonic::codec::CompressionEncoding;
use tonic::{transport::channel::ClientTlsConfig, Status};
use yellowstone_grpc_client::{GeyserGrpcClient, GeyserGrpcClientResult};
use yellowstone_grpc_proto::geyser::SubscribeUpdateSlot;
use yellowstone_grpc_proto::geyser::{
    CommitmentLevel, SubscribeRequest, SubscribeRequestFilterTransactions, SubscribeUpdate,
    SubscribeUpdateTransaction, subscribe_update::UpdateOneof, SubscribeRequestPing,
};
use log::{error, info};
use chrono::Local;
use solana_sdk::{pubkey, pubkey::Pubkey, signature::Signature};
use solana_transaction_status::{
    option_serializer::OptionSerializer, EncodedTransactionWithStatusMeta, UiTransactionEncoding,
};

use crate::common::logs_data::DexInstruction;
use crate::common::logs_events::PumpfunEvent;
use crate::common::logs_filters::LogFilter;
use crate::error::{ClientError, ClientResult};

type TransactionsFilterMap = HashMap<String, SubscribeRequestFilterTransactions>;

const PUMP_PROGRAM_ID: Pubkey = pubkey!("6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P");
const CONNECT_TIMEOUT: u64 = 10;
const REQUEST_TIMEOUT: u64 = 60;
const CHANNEL_SIZE: usize = 1000;

#[derive(Clone)]
pub struct TransactionPretty {
    pub slot: u64,
    pub signature: Signature,
    pub is_vote: bool,
    pub tx: EncodedTransactionWithStatusMeta,
    // pub transaction: Option<Transaction>,
}

impl fmt::Debug for TransactionPretty {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        struct TxWrap<'a>(&'a EncodedTransactionWithStatusMeta);
        impl<'a> fmt::Debug for TxWrap<'a> {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                let serialized = serde_json::to_string(self.0).expect("failed to serialize");
                fmt::Display::fmt(&serialized, f)
            }
        }

        f.debug_struct("TransactionPretty")
            .field("slot", &self.slot)
            .field("signature", &self.signature)
            .field("is_vote", &self.is_vote)
            .field("tx", &TxWrap(&self.tx))
            .finish()
    }
}

impl From<SubscribeUpdateTransaction> for TransactionPretty {
    fn from(SubscribeUpdateTransaction { transaction, slot }: SubscribeUpdateTransaction) -> Self {
        let tx = transaction.expect("should be defined");
        // let transaction_info = tx.transaction.clone().unwrap();
        Self {
            slot,
            signature: Signature::try_from(tx.signature.as_slice()).expect("valid signature"),
            is_vote: tx.is_vote,
            tx: yellowstone_grpc_proto::convert_from::create_tx_with_meta(tx)
                .expect("valid tx with meta")
                .encode(UiTransactionEncoding::Base64, Some(u8::MAX), true)
                .expect("failed to encode"),
            // transaction: Some(transaction_info),
        }
    }
}

#[derive(Clone)]
pub struct YellowstoneGrpc {
    endpoint: String,
}

impl YellowstoneGrpc {
    pub fn new(endpoint: String) -> Self {
        Self { endpoint }
    }

    pub async fn connect(
        &self,
        transactions: TransactionsFilterMap,
    ) -> ClientResult<
        GeyserGrpcClientResult<(
            impl Sink<SubscribeRequest, Error = mpsc::SendError>,
            impl Stream<Item = Result<SubscribeUpdate, Status>>,
        )>
    > {
        if CryptoProvider::get_default().is_none() {
            default_provider()
                .install_default()
                .map_err(|e| ClientError::Other(format!("Failed to install crypto provider: {:?}", e)))?;
        }

        let mut client = GeyserGrpcClient::build_from_shared(self.endpoint.clone())
            .map_err(|e| ClientError::Other(format!("Failed to build client: {:?}", e)))?
            .tls_config(ClientTlsConfig::new().with_native_roots())
            .map_err(|e| ClientError::Other(format!("Failed to build client: {:?}", e)))?
            .connect_timeout(Duration::from_secs(CONNECT_TIMEOUT))
            .timeout(Duration::from_secs(REQUEST_TIMEOUT))
            .connect()
            .await
            .map_err(|e| ClientError::Other(format!("Failed to connect: {:?}", e)))?;

        let subscribe_request = SubscribeRequest {
            transactions,
            commitment: Some(CommitmentLevel::Processed.into()),
            ..Default::default()
        };

        Ok(client.subscribe_with_request(Some(subscribe_request)).await)
    }

    pub fn get_subscribe_request_filter(
        &self,
        account_include: Vec<String>,
        account_exclude: Vec<String>,
        account_required: Vec<String>,
    ) -> TransactionsFilterMap {
        let mut transactions = HashMap::new();
        transactions.insert(
            "client".to_string(),
            SubscribeRequestFilterTransactions {
                vote: Some(false),
                failed: Some(false),
                signature: None,
                account_include,
                account_exclude,
                account_required,
            },
        );
        transactions
    }

    // pub fn get_subscribe_account_updater_request_filter(
    //     &self,
    //     account_include: Vec<String>,
    //     account_exclude: Vec<String>,
    //     account_required: Vec<String>,
    // ) -> TransactionsFilterMap {
    //     let mut transactions = HashMap::new();
    //     transactions.insert(
    //         "client".to_string(),
    //         SubscribeUpdateAccount {
    //             account: account_include,
    //             slot: None,
    //             is_startup: None,
    //         },
    //     );
    //     transactions
    // }

    // pub fn get_subscribe_update_slot_request_filter(
    //     &self,
    //     account_include: Vec<String>,
    //     account_exclude: Vec<String>,
    //     account_required: Vec<String>,
    // ) -> TransactionsFilterMap {
    //     let mut transactions = HashMap::new();
    //     transactions.insert(
    //         "client".to_string(),
    //         SubscribeUpdateSlot {
    //             slot: 0,
    //             parent: None,
    //             status: None,
    //             dead_error: None,
    //         },
    //     );
    //     transactions
    // }

    async fn handle_stream_message(
        msg: SubscribeUpdate,
        tx: &mut mpsc::Sender<TransactionPretty>,
        subscribe_tx: &mut (impl Sink<SubscribeRequest, Error = mpsc::SendError> + Unpin),
    ) -> ClientResult<()> {
        match msg.update_oneof {
            Some(UpdateOneof::Transaction(sut)) => {
                let transaction_pretty = TransactionPretty::from(sut);
                tx.try_send(transaction_pretty).map_err(|e| ClientError::Other(format!("Send error: {:?}", e)))?;
            }
            Some(UpdateOneof::Ping(_)) => {
                subscribe_tx
                    .send(SubscribeRequest {
                        ping: Some(SubscribeRequestPing { id: 1 }),
                        ..Default::default()
                    })
                    .await
                    .map_err(|e| ClientError::Other(format!("Ping error: {:?}", e)))?;
                info!("service is ping: {}", Local::now());
            }
            Some(UpdateOneof::Pong(_)) => {
                info!("service is pong: {}", Local::now());
            }
            _ => {}
        }
        Ok(())
    }

    // pub async fn subscribe_account_updater<F>(&self, callback: F, bot_wallet: Option<Pubkey>) -> ClientResult<()> 
    // where
    //     F: Fn(PumpfunEvent) + Send + Sync + 'static,
    // {
    //     let addrs = vec![PUMP_PROGRAM_ID.to_string()];
    //     let transactions = self.get_subscribe_request_filter(addrs, vec![], vec![]);
    //     let (mut subscribe_tx, mut stream) = self.connect(transactions).await?
    //     .map_err(|e| ClientError::Other(format!("Failed to subscribe: {:?}", e)))?;
    //     let (mut tx, mut rx) = mpsc::channel::<TransactionPretty>(CHANNEL_SIZE);

    //     let callback = Box::new(callback);
        
    // }

    pub async fn subscribe_pumpfun<F>(&self, callback: F, bot_wallet: Option<Pubkey>) -> ClientResult<()> 
    where
        F: Fn(PumpfunEvent) + Send + Sync + 'static,
    {
        let addrs = vec![PUMP_PROGRAM_ID.to_string()];
        let transactions = self.get_subscribe_request_filter(addrs, vec![], vec![]);
        let (mut subscribe_tx, mut stream) = self.connect(transactions).await?
        .map_err(|e| ClientError::Other(format!("Failed to subscribe: {:?}", e)))?;
        let (mut tx, mut rx) = mpsc::channel::<TransactionPretty>(CHANNEL_SIZE);

        let callback = Box::new(callback);

        tokio::spawn(async move {
            while let Some(message) = stream.next().await {
                match message {
                    Ok(msg) => {
                        if let Err(e) = Self::handle_stream_message(msg, &mut tx, &mut subscribe_tx).await {
                            error!("Error handling message: {:?}", e);
                            break;
                        }
                    }
                    Err(error) => {
                        error!("Stream error: {error:?}");
                        break;
                    }
                }
            }
        });

        while let Some(transaction_pretty) = rx.next().await {
            if let Err(e) = Self::process_pumpfun_transaction(transaction_pretty, &*callback, bot_wallet).await {
                error!("Error processing transaction: {:?}", e);
            }
        }
        Ok(())
    }

    async fn process_pumpfun_transaction<F>(transaction_pretty: TransactionPretty, callback: &F, bot_wallet: Option<Pubkey>) -> ClientResult<()> 
    where
        F: Fn(PumpfunEvent) + Send + Sync,
    {
        let slot = transaction_pretty.slot;
        let trade_raw = transaction_pretty.tx;
        let meta = trade_raw.meta.as_ref()
            .ok_or_else(|| ClientError::Other("Missing transaction metadata".to_string()))?;
            
        if meta.err.is_some() {
            return Ok(());
        }

        let logs = if let OptionSerializer::Some(logs) = &meta.log_messages {
            logs
        } else {
            &vec![]
        };

        let mut dev_address: Option<Pubkey> = None;
        let instructions = LogFilter::parse_instruction(logs, bot_wallet).unwrap();
        for instruction in instructions {
            match instruction {
                DexInstruction::CreateToken(mut token_info) => {
                    token_info.slot = slot;
                    dev_address = Some(token_info.user);
                    callback(PumpfunEvent::NewToken(token_info));
                }
                DexInstruction::UserTrade(mut trade_info) => {
                    trade_info.slot = slot;
                    if Some(trade_info.user) == dev_address {
                        callback(PumpfunEvent::NewDevTrade(trade_info));
                    } else {
                        callback(PumpfunEvent::NewUserTrade(trade_info));
                    }
                }
                DexInstruction::BotTrade(mut trade_info) => {
                    trade_info.slot = slot;
                    callback(PumpfunEvent::NewBotTrade(trade_info));
                }
                _ => {}
            }
        }

        Ok(())
    }
}
