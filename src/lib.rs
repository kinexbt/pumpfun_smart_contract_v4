pub mod accounts;
pub mod constants;
pub mod error;
pub mod instruction;
pub mod jito;
pub mod grpc;
pub mod common;
pub mod ipfs;
pub mod trade;

use std::sync::Arc;

use anyhow::anyhow;
use solana_client::rpc_client::RpcClient;
use solana_sdk::{
    commitment_config::CommitmentConfig,
    pubkey::Pubkey,
    signature::{Keypair, Signer, Signature},
};

use common::{logs_data::TradeInfo, logs_events::PumpfunEvent, logs_subscribe};
use common::logs_subscribe::SubscriptionHandle;
use ipfs::TokenMetadataIPFS;

use crate::jito::JitoClient;
use crate::trade::common::PriorityFee;

pub struct PumpFun {
    pub payer: Arc<Keypair>,
    pub rpc: RpcClient,
    pub jito_client: Option<JitoClient>,
}

impl Clone for PumpFun {
    fn clone(&self) -> Self {
        Self {
            payer: self.payer.clone(),
            rpc: RpcClient::new_with_commitment(
                self.rpc.url().to_string(),
                self.rpc.commitment()
            ),
            jito_client: self.jito_client.clone(),
        }
    }
}

impl PumpFun {
    #[inline]
    pub fn new(
        payer: Arc<Keypair>,
        rpc_url: String,
        commitment: Option<CommitmentConfig>,
        jito_url: Option<String>,
    ) -> Self {
        let rpc = RpcClient::new_with_commitment(
            rpc_url,
            commitment.unwrap_or(CommitmentConfig::processed())
        );   

        let jito_client = jito_url.map(|url| JitoClient::new(&url, None));

        Self {
            payer,
            rpc,
            jito_client,
        }
    }

    /// Create a new token
    pub async fn create(
        &self,
        mint: &Keypair,
        ipfs: TokenMetadataIPFS,
        priority_fee: PriorityFee,
    ) -> Result<Signature, anyhow::Error> {
        trade::create::create(
            &self.rpc,
            &self.payer,
            mint,
            ipfs,
            priority_fee,
        ).await 
    }

    pub async fn create_and_buy(
        &self,
        mint: &Keypair,
        ipfs: TokenMetadataIPFS,
        amount_sol: u64,
        slippage_basis_points: Option<u64>,
        priority_fee: PriorityFee,
    ) -> Result<Signature, anyhow::Error> {
        trade::create::create_and_buy(
            &self.rpc,
            &self.payer,
            mint,
            ipfs,
            amount_sol,
            slippage_basis_points,
            priority_fee,
        ).await
    }

    pub async fn create_and_buy_list_with_jito(
        &self,
        payers: Vec<&Keypair>,
        mint: &Keypair,
        ipfs: TokenMetadataIPFS,
        amount_sols: Vec<u64>,
        slippage_basis_points: Option<u64>,
        priority_fee: PriorityFee,
    ) -> Result<String, anyhow::Error> { 
        trade::create::create_and_buy_list_with_jito(
            &self.rpc,
            &self.jito_client.as_ref().unwrap(),
            payers,
            mint,
            ipfs,
            amount_sols,
            slippage_basis_points,
            priority_fee,
        ).await
    }

    pub async fn create_and_buy_with_jito(
        &self,
        payer: &Keypair,
        mint: &Keypair,
        ipfs: TokenMetadataIPFS,
        amount_sol: u64,
        slippage_basis_points: Option<u64>,
        priority_fee: PriorityFee,
    ) -> Result<String, anyhow::Error> { 
        trade::create::create_and_buy_with_jito(
            &self.rpc,
            &self.jito_client.as_ref().unwrap(),
            payer,
            mint,
            ipfs,
            amount_sol,
            slippage_basis_points,
            priority_fee,
        ).await
    }
    /// Buy tokens
    pub async fn buy(
        &self,
        mint: &Pubkey,
        amount_sol: u64,
        slippage_basis_points: Option<u64>,
        priority_fee: PriorityFee,
    ) -> Result<Signature, anyhow::Error> {
        trade::buy::buy(
            &self.rpc,
            &self.payer,
            mint,
            amount_sol,
            slippage_basis_points,
            priority_fee,
        ).await
    }

    /// Buy tokens using Jito
    pub async fn buy_with_jito(
        &self,
        mint: &Pubkey,
        amount_sol: u64,
        slippage_basis_points: Option<u64>,
        priority_fee: PriorityFee,
    ) -> Result<String, anyhow::Error> {
        trade::buy::buy_with_jito(
            &self.rpc,
            &self.jito_client.as_ref().unwrap(),
            &self.payer,
            mint,
            amount_sol,
            slippage_basis_points,
            priority_fee,
        ).await
    }

    pub async fn buy_list_with_jito(
        &self,
        payers: Vec<&Keypair>,
        mint: &Pubkey,
        amount_sols: Vec<u64>,
        slippage_basis_points: Option<u64>,
        priority_fee: PriorityFee,
    ) -> Result<String, anyhow::Error> {
        trade::buy::buy_list_with_jito(
            &self.rpc,
            &self.jito_client.as_ref().unwrap(),
            payers,
            mint,
            amount_sols,
            slippage_basis_points,
            priority_fee,
        ).await
    }

    /// Sell tokens
    pub async fn sell(
        &self,
        mint: &Pubkey,
        amount_token: Option<u64>,
        slippage_basis_points: Option<u64>,
        priority_fee: PriorityFee,
    ) -> Result<Signature, anyhow::Error> {
        trade::sell::sell(
            &self.rpc,
            &self.payer,
            mint,
            amount_token,
            slippage_basis_points,
            priority_fee,
        ).await
    }

    /// Sell tokens by percentage
    pub async fn sell_by_percent(
        &self,
        mint: &Pubkey,
        percent: u64,
        slippage_basis_points: Option<u64>,
        priority_fee: PriorityFee,
    ) -> Result<Signature, anyhow::Error> {
        trade::sell::sell_by_percent(
            &self.rpc,
            &self.payer,
            mint,
            percent,
            slippage_basis_points,
            priority_fee,
        ).await
    }

    pub async fn sell_by_percent_with_jito(
        &self,
        mint: &Pubkey,
        percent: u64,
        slippage_basis_points: Option<u64>,
        priority_fee: PriorityFee,
    ) -> Result<String, anyhow::Error> {
        trade::sell::sell_by_percent_with_jito(
            &self.rpc,
            &self.payer,
            self.jito_client.as_ref().unwrap(),
            mint,
            percent,
            slippage_basis_points,
            priority_fee,
        ).await
    }

    /// Sell tokens using Jito
    pub async fn sell_with_jito(
        &self,
        mint: &Pubkey,
        amount_token: Option<u64>,
        slippage_basis_points: Option<u64>,
        priority_fee: PriorityFee,
    ) -> Result<String, anyhow::Error> {
        let jito_client = self.jito_client.as_ref()
            .ok_or_else(|| anyhow!("Jito client not found"))?;

        trade::sell::sell_with_jito(
            &self.rpc,
            &self.payer,
            jito_client,
            mint,
            amount_token,
            slippage_basis_points,
            priority_fee,
        ).await
    }

    #[inline]
    pub async fn tokens_subscription<F>(
        &self,
        ws_url: &str,
        commitment: CommitmentConfig,
        callback: F,
        bot_wallet: Option<Pubkey>,
    ) -> Result<SubscriptionHandle, Box<dyn std::error::Error>>
    where
        F: Fn(PumpfunEvent) + Send + Sync + 'static,
    {
        logs_subscribe::tokens_subscription(ws_url, commitment, callback, bot_wallet).await
    }

    #[inline]
    pub async fn stop_subscription(&self, subscription_handle: SubscriptionHandle) {
        subscription_handle.shutdown().await;
    }

    #[inline]
    pub fn get_sol_balance(&self, payer: &Pubkey) -> Result<u64, anyhow::Error> {
        trade::common::get_sol_balance(&self.rpc, payer)
    }

    #[inline]
    pub fn get_payer_sol_balance(&self) -> Result<u64, anyhow::Error> {
        trade::common::get_sol_balance(&self.rpc, &self.payer.pubkey())
    }

    #[inline]
    pub fn get_token_balance(&self, payer: &Pubkey, mint: &Pubkey) -> Result<u64, anyhow::Error> {
        trade::common::get_token_balance(&self.rpc, payer, mint)
    }

    #[inline]
    pub fn get_payer_token_balance(&self, mint: &Pubkey) -> Result<u64, anyhow::Error> {
        trade::common::get_token_balance(&self.rpc, &self.payer.pubkey(), mint)
    }

    #[inline]
    pub fn get_payer_pubkey(&self) -> Pubkey {
        self.payer.pubkey()
    }

    #[inline]
    pub fn get_payer(&self) -> &Keypair {
        self.payer.as_ref()
    }

    #[inline]
    pub fn get_token_price(&self,virtual_sol_reserves: u64, virtual_token_reserves: u64) -> f64 {
        trade::common::get_token_price(virtual_sol_reserves, virtual_token_reserves)
    }

    #[inline]
    pub fn get_buy_price(&self, amount: u64, trade_info: &TradeInfo) -> u64 {
        trade::common::get_buy_price(amount, trade_info)
    }

    #[inline]
    pub async fn transfer_sol(&self, payer: &Keypair, receive_wallet: &Pubkey, amount: u64) -> Result<(), anyhow::Error> {
        trade::common::transfer_sol(&self.rpc, payer, receive_wallet, amount).await
    }
}
