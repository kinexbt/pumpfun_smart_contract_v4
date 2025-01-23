use std::{collections::HashMap, fmt, time::Duration};

use futures::{channel::mpsc, sink::Sink, Stream, StreamExt, SinkExt};
use rustls::crypto::{ring::default_provider, CryptoProvider};
use tonic::{transport::channel::ClientTlsConfig, Status};
use yellowstone_grpc_client::{GeyserGrpcClient, GeyserGrpcClientResult};
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
use anyhow::anyhow;

use crate::common::event::{PumpEvent, RaydiumEvent, SwapBaseInLog, TradeEvent};
use crate::error::AppError;
use crate::common::event::PumpfunEvent;

// 类型别名定义
type TransactionsFilterMap = HashMap<String, SubscribeRequestFilterTransactions>;
type GrpcStreamResult = GeyserGrpcClientResult<(
    Box<dyn Sink<SubscribeRequest, Error = mpsc::SendError> + Unpin + Send>,
    Box<dyn Stream<Item = Result<SubscribeUpdate, Status>> + Unpin + Send>,
)>;

// 常量定义
const AMM_V4: Pubkey = pubkey!("675kPX9MHTjS2zt1qfr1NYHuzeLXfQM9H24wFSUt1Mp8");
const PUMP_PROGRAM_ID: Pubkey = pubkey!("6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P");
const CONNECT_TIMEOUT: u64 = 10;
const REQUEST_TIMEOUT: u64 = 60;
const CHANNEL_SIZE: usize = 1000;

// 枚举定义
#[derive(Debug)]
pub enum SwapType {
    Pump,
    Raydium,
}

// 结构体定义
#[allow(dead_code)]
pub struct TransactionPretty {
    pub slot: u64,
    pub signature: Signature,
    pub is_vote: bool,
    pub tx: EncodedTransactionWithStatusMeta,
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
        Self {
            slot,
            signature: Signature::try_from(tx.signature.as_slice()).expect("valid signature"),
            is_vote: tx.is_vote,
            tx: yellowstone_grpc_proto::convert_from::create_tx_with_meta(tx)
                .expect("valid tx with meta")
                .encode(UiTransactionEncoding::Base64, Some(u8::MAX), true)
                .expect("failed to encode"),
        }
    }
}

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
    ) -> Result<
        GeyserGrpcClientResult<(
            impl Sink<SubscribeRequest, Error = mpsc::SendError>,
            impl Stream<Item = Result<SubscribeUpdate, Status>>,
        )>,
        AppError,
    > {
        if CryptoProvider::get_default().is_none() {
            default_provider()
                .install_default()
                .map_err(|e| anyhow::anyhow!("Failed to install crypto provider: {:?}", e))?;
        }

        let mut client = GeyserGrpcClient::build_from_shared(self.endpoint.clone())?
            .tls_config(ClientTlsConfig::new().with_native_roots())?
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(60))
            .connect()
            .await?;

        let subscribe_request = SubscribeRequest {
            transactions,
            commitment: Some(CommitmentLevel::Processed.into()),
            ..Default::default()
        };

        Ok(client.subscribe_with_request(Some(subscribe_request)).await)
    }

    pub async fn subscribe_accounts(&self, accounts: Vec<String>) -> Result<(), AppError> {
        let transactions = self.get_subscribe_request_filter(accounts, vec![], vec![]);
        let (mut subscribe_tx, mut stream) = self.connect(transactions).await??;
        let (mut tx, mut rx) = mpsc::channel::<TransactionPretty>(CHANNEL_SIZE);

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
            if let Err(e) = Self::process_transaction(transaction_pretty, &|_| {}, None).await {
                error!("Error processing transaction: {:?}", e);
            }
        }
        Ok(())
    }

    pub async fn subscribe_pumpfun<F>(&self, callback: F, bot_wallet: Option<Pubkey>) -> Result<(), AppError> 
    where
        F: Fn(PumpfunEvent) + Send + Sync + 'static,
    {
        let addrs = vec![PUMP_PROGRAM_ID.to_string()];
        let transactions = self.get_subscribe_request_filter(addrs, vec![], vec![]);
        let (mut subscribe_tx, mut stream) = self.connect(transactions).await??;
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
            if let Err(e) = Self::process_transaction(transaction_pretty, &*callback, bot_wallet).await {
                error!("Error processing transaction: {:?}", e);
            }
        }
        Ok(())
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


    async fn handle_stream_message(
        msg: SubscribeUpdate,
        tx: &mut mpsc::Sender<TransactionPretty>,
        subscribe_tx: &mut (impl Sink<SubscribeRequest, Error = mpsc::SendError> + Unpin),
    ) -> Result<(), AppError> {
        match msg.update_oneof {
            Some(UpdateOneof::Transaction(sut)) => {
                let transaction_pretty = TransactionPretty::from(sut);
                tx.try_send(transaction_pretty).map_err(|e| AppError::from(anyhow!("Send error: {:?}", e)))?;
            }
            Some(UpdateOneof::Ping(_)) => {
                subscribe_tx
                    .send(SubscribeRequest {
                        ping: Some(SubscribeRequestPing { id: 1 }),
                        ..Default::default()
                    })
                    .await
                    .map_err(|e| AppError::from(anyhow!("Ping error: {:?}", e)))?;
                info!("service is ping: {}", Local::now());
            }
            Some(UpdateOneof::Pong(_)) => {
                info!("service is pong: {}", Local::now());
            }
            _ => {}
        }
        Ok(())
    }

    async fn process_transaction<F>(transaction_pretty: TransactionPretty, callback: &F, bot_wallet: Option<Pubkey>) -> Result<(), AppError> 
    where
        F: Fn(PumpfunEvent) + Send + Sync,
    {
        let trade_raw = transaction_pretty.tx;
        let meta = trade_raw.meta.as_ref()
            .ok_or_else(|| AppError::from(anyhow!("Missing transaction metadata")))?;
            
        if meta.err.is_some() {
            return Ok(());
        }

        let logs = if let OptionSerializer::Some(logs) = &meta.log_messages {
            logs
        } else {
            &vec![]
        };

        if let Ok(swap_type) = Self::get_swap_type(&trade_raw) {
            match swap_type {
                SwapType::Raydium => {
                    let event = RaydiumEvent::parse_logs::<SwapBaseInLog>(logs);
                    info!("RaydiumEvent {:#?}", event);
                }
                SwapType::Pump => {
                    let (create_event, trade_event) = PumpEvent::parse_logs(logs);
                    if let Some(create_event) = create_event {
                        callback(PumpfunEvent::NewToken(create_event));
                    }
                    if let Some(trade_event) = trade_event {
                        if let Some(bot_wallet_pubkey) = bot_wallet {
                            if trade_event.user == bot_wallet_pubkey {
                                callback(PumpfunEvent::NewBotTrade(trade_event));
                            } else {
                                callback(PumpfunEvent::NewUserTrade(trade_event));
                            }
                        } else {
                            callback(PumpfunEvent::NewUserTrade(trade_event));
                        }
                    }
                }
            }
        }
        Ok(())
    }

    pub fn get_swap_type(trade_raw: &EncodedTransactionWithStatusMeta) -> Result<SwapType, AppError> {
        let transaction = trade_raw.transaction.decode()
            .ok_or_else(|| AppError::from(anyhow!("Failed to decode transaction")))?;
        
        let account_keys = transaction.message.static_account_keys();
        let program_index = account_keys
            .iter()
            .position(|item| item == &AMM_V4 || item == &PUMP_PROGRAM_ID)
            .ok_or_else(|| AppError::from(anyhow!("swap type program_id not found")))?;

        match account_keys[program_index] {
            AMM_V4 => Ok(SwapType::Raydium),
            PUMP_PROGRAM_ID => Ok(SwapType::Pump),
            _ => Err(AppError::from(anyhow!("Invalid program_id")))
        }
    }
}