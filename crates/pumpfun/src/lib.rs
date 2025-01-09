#![doc = include_str!("../RUSTDOC.md")]

pub mod accounts;
pub mod constants;
pub mod error;
pub mod instruction;
pub mod utils;
pub mod jito;

use anchor_client::{
    solana_client::rpc_client::RpcClient,
    solana_sdk::{
        commitment_config::CommitmentConfig,
        pubkey::Pubkey,
        signature::{Keypair, Signature},
        signer::Signer,
        instruction::Instruction,
        system_instruction,
        compute_budget::ComputeBudgetInstruction,
        transaction::Transaction,
    },
};
use anchor_client::Cluster;
use anchor_spl::associated_token::{
    get_associated_token_address,
    spl_associated_token_account::instruction::create_associated_token_account,
};
use instruction::logs_subscribe;
use instruction::logs_subscribe::SubscriptionHandle;
use instruction::logs_events::DexEvent;

use std::sync::Arc;
use borsh::BorshDeserialize;
use std::time::Instant;
pub use pumpfun_cpi as cpi;

use crate::jito::JitoClient;
use crate::error::ClientError;

// Constants
const DEFAULT_SLIPPAGE: u64 = 500; // 10%
const DEFAULT_COMPUTE_UNIT_LIMIT: u32 = 68_000;
const DEFAULT_COMPUTE_UNIT_PRICE: u64 = 400_000;

/// Priority fee configuration
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PriorityFee {
    pub limit: Option<u32>,
    pub price: Option<u64>,
}

pub struct PumpFun {
    pub rpc: RpcClient,
    pub payer: Arc<Keypair>,
    pub jito_client: Option<JitoClient>,
}

impl Clone for PumpFun {
    fn clone(&self) -> Self {
        Self {
            rpc: RpcClient::new_with_commitment(
                self.rpc.url().to_string(),
                self.rpc.commitment()
            ),
            payer: self.payer.clone(),
            jito_client: self.jito_client.clone(),
        }
    }
}

impl PumpFun {
    /// Create a new PumpFun client instance
    pub fn new(
        cluster: Cluster,
        commitment: Option<CommitmentConfig>,
        payer: Arc<Keypair>,
        jito_url: Option<String>,
    ) -> Self {
        let rpc = RpcClient::new_with_commitment(
            cluster.url(),
            commitment.unwrap_or(CommitmentConfig::confirmed())
        );   

        let jito_client = jito_url.map(|url| JitoClient::new(&url));

        Self {
            rpc,
            payer,
            jito_client,
        }
    }

    /// Create a new token
    pub async fn create(
        &self,
        mint: &Keypair,
        metadata: utils::CreateTokenMetadata,
        priority_fee: Option<PriorityFee>,
    ) -> Result<Signature, ClientError> {
        let ipfs = utils::create_token_metadata(metadata)
            .await
            .map_err(ClientError::UploadMetadataError)?;

        let mut instructions = self.create_priority_fee_instructions(priority_fee);

        instructions.push(instruction::create(
            &self.payer.clone(),
            mint,
            cpi::instruction::Create {
                _name: ipfs.metadata.name,
                _symbol: ipfs.metadata.symbol,
                _uri: ipfs.metadata.image,
            },
        ));

        let recent_blockhash = self.rpc.get_latest_blockhash()?;
        let transaction = Transaction::new_signed_with_payer(
            &instructions,
            Some(&self.payer.pubkey()),
            &[&self.payer.clone(), mint],
            recent_blockhash,
        );

        let signature = self.rpc.send_and_confirm_transaction(&transaction)?;

        Ok(signature)
    }

    /// Create and buy tokens in one transaction
    pub async fn create_and_buy(
        &self,
        mint: &Keypair,
        metadata: utils::CreateTokenMetadata,
        amount_sol: u64,
        slippage_basis_points: Option<u64>,
        priority_fee: Option<PriorityFee>,
    ) -> Result<Signature, ClientError> {
        let ipfs = utils::create_token_metadata(metadata)
            .await
            .map_err(ClientError::UploadMetadataError)?;

        let global_account = self.get_global_account()?;
        let buy_amount = global_account.get_initial_buy_price(amount_sol);
        let buy_amount_with_slippage =
            utils::calculate_with_slippage_buy(amount_sol, slippage_basis_points.unwrap_or(DEFAULT_SLIPPAGE));

        let mut instructions = self.create_priority_fee_instructions(priority_fee);

        instructions.push(instruction::create(
            &self.payer.clone(),
            mint,
            cpi::instruction::Create {
                _name: ipfs.metadata.name,
                _symbol: ipfs.metadata.symbol,
                _uri: ipfs.metadata.image,
            },
        ));

        let ata = get_associated_token_address(&self.payer.pubkey(), &mint.pubkey());
        if self.rpc.get_account(&ata).is_err() {
            instructions.push(create_associated_token_account(
                &self.payer.pubkey(),
                &self.payer.pubkey(),
                &mint.pubkey(),
                &constants::accounts::TOKEN_PROGRAM,
            ));
        }

        instructions.push(instruction::buy(
            &self.payer.clone(),
            &mint.pubkey(),
            &global_account.fee_recipient,
            cpi::instruction::Buy {
                _amount: buy_amount,
                _max_sol_cost: buy_amount_with_slippage,
            },
        ));

        let recent_blockhash = self.rpc.get_latest_blockhash()?;
        let transaction = Transaction::new_signed_with_payer(
            &instructions,
            Some(&self.payer.pubkey()),
            &[&self.payer.clone(), mint],
            recent_blockhash,
        );

        let signature = self.rpc.send_and_confirm_transaction(&transaction)?;

        Ok(signature)
    }

    /// Buy tokens
    pub async fn buy(
        &self,
        mint: &Pubkey,
        amount_sol: u64,
        slippage_basis_points: Option<u64>,
        priority_fee: Option<PriorityFee>,
    ) -> Result<Signature, ClientError> {
        let global_account = self.get_global_account()?;
        let bonding_curve_account = self.get_bonding_curve_account(mint)?;
        let buy_amount = bonding_curve_account
            .get_buy_price(amount_sol)
            .map_err(ClientError::BondingCurveError)?;
        let buy_amount_with_slippage =
            utils::calculate_with_slippage_buy(amount_sol, slippage_basis_points.unwrap_or(DEFAULT_SLIPPAGE));

        let mut instructions = self.create_priority_fee_instructions(priority_fee);

        let ata = get_associated_token_address(&self.payer.pubkey(), mint);
        if self.rpc.get_account(&ata).is_err() {
            instructions.push(create_associated_token_account(
                &self.payer.pubkey(),
                &self.payer.pubkey(),
                mint,
                &constants::accounts::TOKEN_PROGRAM,
            ));
        }

        instructions.push(instruction::buy(
            &self.payer.clone(),
            mint,
            &global_account.fee_recipient,
            cpi::instruction::Buy {
                _amount: buy_amount,
                _max_sol_cost: buy_amount_with_slippage,
            },
        ));

        let recent_blockhash = self.rpc.get_latest_blockhash()?;
        let transaction = Transaction::new_signed_with_payer(
            &instructions,
            Some(&self.payer.pubkey()),
            &[&self.payer.clone()],
            recent_blockhash,
        );

        let signature = self.rpc.send_transaction(&transaction)?;
        Ok(signature)
    }

    /// Buy tokens using Jito
    pub async fn buy_with_jito(
        &self,
        mint: &Pubkey,
        amount_sol: u64,
        slippage_basis_points: Option<u64>,
        priority_fee: Option<PriorityFee>,
    ) -> Result<Signature, ClientError> {
        let start_time = Instant::now();

        let jito_client = self.jito_client.as_ref()
            .ok_or_else(|| ClientError::Other("Jito client not found".to_string()))?;

        let global_account = self.get_global_account()?;
        let bonding_curve_pda = Self::get_bonding_curve_pda(mint)
            .ok_or(ClientError::BondingCurveNotFound)?;
        let bonding_curve_account = self.get_bonding_curve_account(mint)?;
        let buy_amount = bonding_curve_account
            .get_buy_price(amount_sol)
            .map_err(ClientError::BondingCurveError)?;
        let buy_amount_with_slippage =
            utils::calculate_with_slippage_buy(amount_sol, slippage_basis_points.unwrap_or(DEFAULT_SLIPPAGE));

        let (unit_limit, _unit_price) = self.get_compute_units(priority_fee);
        let mut instructions = self.create_priority_fee_instructions(priority_fee);

        let priority_fees = jito_client.estimate_priority_fees(&bonding_curve_pda).await?;
        let tip_account = jito_client.get_tip_account().await?;

        let ata = get_associated_token_address(&self.payer.pubkey(), mint);
        if self.rpc.get_account(&ata).is_err() {
            instructions.push(create_associated_token_account(
                &self.payer.pubkey(),
                &self.payer.pubkey(),
                mint,
                &constants::accounts::TOKEN_PROGRAM,
            ));
        }

        instructions.push(instruction::buy(
            self.payer.as_ref(),
            mint,
            &global_account.fee_recipient,
            cpi::instruction::Buy {
                _amount: buy_amount,
                _max_sol_cost: buy_amount_with_slippage,
            },
        ));

        let total_priority_fee = self.calculate_priority_fee(priority_fees.per_compute_unit.extreme, unit_limit);
        
        instructions.push(
            system_instruction::transfer(
                &self.payer.pubkey(),
                &tip_account,
                total_priority_fee,
            ),
        );

        let recent_blockhash = self.rpc.get_latest_blockhash()?;
        let transaction = Transaction::new_signed_with_payer(
            &instructions,
            Some(&self.payer.pubkey()),
            &[&self.payer.clone()],
            recent_blockhash,
        );

        let signature = jito_client.send_transaction(&transaction).await?;
        println!("Total Jito buy operation time: {:?}ms", start_time.elapsed().as_millis());

        Ok(signature)
    }

    /// Sell tokens
    pub async fn sell(
        &self,
        mint: &Pubkey,
        amount_token: Option<u64>,
        slippage_basis_points: Option<u64>,
        priority_fee: Option<PriorityFee>,
    ) -> Result<Signature, ClientError> {
        let ata = get_associated_token_address(&self.payer.pubkey(), mint);
        let balance = self.rpc.get_token_account_balance(&ata)?;
        let balance_u64 = balance.amount.parse::<u64>()
            .map_err(|_| ClientError::Other("Failed to parse token balance".to_string()))?;
        let amount = amount_token.unwrap_or(balance_u64);
        
        if amount == 0 {
            return Err(ClientError::Other("Balance is 0".to_string()));
        }

        let global_account = self.get_global_account()?;
        let bonding_curve_account = self.get_bonding_curve_account(mint)?;
        let min_sol_output = bonding_curve_account
            .get_sell_price(amount, global_account.fee_basis_points)
            .map_err(ClientError::BondingCurveError)?;
        let min_sol_output_with_slippage = utils::calculate_with_slippage_sell(
            min_sol_output,
            slippage_basis_points.unwrap_or(DEFAULT_SLIPPAGE),
        );

        let mut instructions = self.create_priority_fee_instructions(priority_fee);

        instructions.push(instruction::sell(
            &self.payer.clone(),
            mint,
            &global_account.fee_recipient,
            cpi::instruction::Sell {
                _amount: amount,
                _min_sol_output: min_sol_output_with_slippage,
            },
        ));

        let recent_blockhash = self.rpc.get_latest_blockhash()?;
        let transaction = Transaction::new_signed_with_payer(
            &instructions,
            Some(&self.payer.pubkey()),
            &[&self.payer.clone()],
            recent_blockhash,
        );

        let signature = self.rpc.send_and_confirm_transaction(&transaction)?;

        Ok(signature)
    }

    /// Sell tokens by percentage
    pub async fn sell_by_percent(
        &self,
        mint: &Pubkey,
        percent: u64,
        slippage_basis_points: Option<u64>,
        priority_fee: Option<PriorityFee>,
    ) -> Result<Signature, ClientError> {
        if percent > 100 {
            return Err(ClientError::Other("Percentage must be between 0 and 100".to_string()));
        }

        let ata = get_associated_token_address(&self.payer.pubkey(), mint);
        let balance = self.rpc.get_token_account_balance(&ata)?;
        let balance_u64 = balance.amount.parse::<u64>()
            .map_err(|_| ClientError::Other("Failed to parse token balance".to_string()))?;
        
        if balance_u64 == 0 {
            return Err(ClientError::Other("Balance is 0".to_string()));
        }

        let amount = balance_u64 * percent / 100;
        self.sell(mint, Some(amount), slippage_basis_points, priority_fee).await
    }

    /// Sell tokens using Jito
    pub async fn sell_with_jito(
        &self,
        mint: &Pubkey,
        amount_token: Option<u64>,
        slippage_basis_points: Option<u64>,
        priority_fee: Option<PriorityFee>,
    ) -> Result<Signature, ClientError> {
        let start_time = Instant::now();

        let jito_client = self.jito_client.as_ref()
            .ok_or_else(|| ClientError::Other("Jito client not found".to_string()))?;

        let ata = get_associated_token_address(&self.payer.pubkey(), mint);
        let balance = self.rpc.get_token_account_balance(&ata)?;
        let balance_u64 = balance.amount.parse::<u64>()
            .map_err(|_| ClientError::Other("Failed to parse token balance".to_string()))?;
        let amount = amount_token.unwrap_or(balance_u64);

        if amount == 0 {
            return Err(ClientError::Other("Amount cannot be zero".to_string()));
        }

        let global_account = self.get_global_account()?;
        let bonding_curve_pda = Self::get_bonding_curve_pda(mint)
            .ok_or(ClientError::BondingCurveNotFound)?;
        let bonding_curve_account = self.get_bonding_curve_account(mint)?;
        let min_sol_output = bonding_curve_account
            .get_sell_price(amount, global_account.fee_basis_points)
            .map_err(ClientError::BondingCurveError)?;
        let min_sol_output_with_slippage = utils::calculate_with_slippage_sell(
            min_sol_output,
            slippage_basis_points.unwrap_or(DEFAULT_SLIPPAGE),
        );

        let (unit_limit, _unit_price) = self.get_compute_units(priority_fee);
        let mut instructions = self.create_priority_fee_instructions(priority_fee);

        let priority_fees = jito_client.estimate_priority_fees(&bonding_curve_pda).await?;
        let tip_account = jito_client.get_tip_account().await?;

        instructions.push(instruction::sell(
            &self.payer.clone(),
            mint,
            &global_account.fee_recipient,
            cpi::instruction::Sell {
                _amount: amount,
                _min_sol_output: min_sol_output_with_slippage,
            },
        ));

        let total_priority_fee = self.calculate_priority_fee(priority_fees.per_compute_unit.extreme, unit_limit);

        instructions.push(
            system_instruction::transfer(
                &self.payer.pubkey(),
                &tip_account,
                total_priority_fee,
            ),
        );

        let recent_blockhash = self.rpc.get_latest_blockhash()?;
        let transaction = Transaction::new_signed_with_payer(
            &instructions,
            Some(&self.payer.pubkey()),
            &[&self.payer.clone()],
            recent_blockhash,
        );

        let signature = jito_client.send_transaction(&transaction).await?;
        println!("Total Jito sell operation time: {:?}ms", start_time.elapsed().as_millis());

        Ok(signature)
    }

    // Helper methods
    fn create_priority_fee_instructions(&self, priority_fee: Option<PriorityFee>) -> Vec<Instruction> {
        let mut instructions = Vec::new();
        if let Some(fee) = priority_fee {
            if let Some(limit) = fee.limit {
                instructions.push(ComputeBudgetInstruction::set_compute_unit_limit(limit));
            }
            if let Some(price) = fee.price {
                instructions.push(ComputeBudgetInstruction::set_compute_unit_price(price));
            }
        }
        instructions
    }

    fn get_compute_units(&self, priority_fee: Option<PriorityFee>) -> (u32, u64) {
        let unit_limit = priority_fee
            .and_then(|fee| fee.limit)
            .unwrap_or(DEFAULT_COMPUTE_UNIT_LIMIT);
        let unit_price = priority_fee
            .and_then(|fee| fee.price)
            .unwrap_or(DEFAULT_COMPUTE_UNIT_PRICE);
        (unit_limit, unit_price)
    }

    fn calculate_priority_fee(&self, priority_fee_per_cu: u64, unit_limit: u32) -> u64 {
        let total_priority_fee_microlamports = priority_fee_per_cu as u128 * unit_limit as u128;
        (total_priority_fee_microlamports / 1_000_000) as u64
    }

    // Public interface methods
    pub fn get_payer_pubkey(&self) -> Pubkey {
        self.payer.pubkey()
    }

    pub fn get_token_balance(&self, account: &Pubkey, mint: &Pubkey) -> Result<u64, ClientError> {
        let ata = get_associated_token_address(account, mint);
        let balance = self.rpc.get_token_account_balance(&ata)?;
        balance.amount.parse::<u64>()
            .map_err(|_| ClientError::Other("Failed to parse token balance".to_string()))
    }

    pub fn get_sol_balance(&self, account: &Pubkey) -> Result<u64, ClientError> {
        self.rpc.get_balance(account).map_err(ClientError::SolanaClientError)
    }

    pub fn get_payer_token_balance(&self, mint: &Pubkey) -> Result<u64, ClientError> {
        self.get_token_balance(&self.payer.pubkey(), mint)
    }

    pub fn get_payer_sol_balance(&self) -> Result<u64, ClientError> {
        self.get_sol_balance(&self.payer.pubkey())
    }

    // PDA related methods
    pub fn get_global_pda() -> Pubkey {
        Pubkey::find_program_address(&[constants::seeds::GLOBAL_SEED], &cpi::ID).0
    }

    pub fn get_mint_authority_pda() -> Pubkey {
        Pubkey::find_program_address(&[constants::seeds::MINT_AUTHORITY_SEED], &cpi::ID).0
    }

    pub fn get_bonding_curve_pda(mint: &Pubkey) -> Option<Pubkey> {
        Pubkey::try_find_program_address(
            &[constants::seeds::BONDING_CURVE_SEED, mint.as_ref()],
            &cpi::ID
        ).map(|(pubkey, _)| pubkey)
    }

    pub fn get_metadata_pda(mint: &Pubkey) -> Pubkey {
        Pubkey::find_program_address(
            &[
                constants::seeds::METADATA_SEED,
                constants::accounts::MPL_TOKEN_METADATA.as_ref(),
                mint.as_ref(),
            ],
            &constants::accounts::MPL_TOKEN_METADATA
        ).0
    }

    // Account related methods
    pub fn get_global_account(&self) -> Result<accounts::GlobalAccount, ClientError> {
        let global = Self::get_global_pda();
        let account = self.rpc.get_account(&global)?;
        accounts::GlobalAccount::try_from_slice(&account.data)
            .map_err(ClientError::BorshError)
    }

    pub fn get_bonding_curve_account(
        &self,
        mint: &Pubkey,
    ) -> Result<accounts::BondingCurveAccount, ClientError> {
        let bonding_curve_pda = Self::get_bonding_curve_pda(mint)
            .ok_or(ClientError::BondingCurveNotFound)?;
        let account = self.rpc.get_account(&bonding_curve_pda)?;
        accounts::BondingCurveAccount::try_from_slice(&account.data)
            .map_err(ClientError::BorshError)
    }

    // Subscription related methods
    pub async fn tokens_subscription<F>(
        &self,
        ws_url: &str,
        commitment: CommitmentConfig,
        callback: F,
        bot_wallet: Option<Pubkey>,
    ) -> Result<SubscriptionHandle, Box<dyn std::error::Error>>
    where
        F: Fn(DexEvent) + Send + Sync + 'static,
    {
        logs_subscribe::tokens_subscription(ws_url, commitment, callback, bot_wallet).await
    }

    pub async fn stop_subscription(&self, subscription_handle: SubscriptionHandle) {
        subscription_handle.shutdown().await;
    }

    pub fn get_buy_amount_with_slippage(&self, amount_sol: u64, slippage_basis_points: Option<u64>) -> u64 {
        utils::calculate_with_slippage_buy(amount_sol, slippage_basis_points.unwrap_or(DEFAULT_SLIPPAGE))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_client() {
        let payer = Arc::new(Keypair::new());
        let client = PumpFun::new(Cluster::Devnet, None, Arc::clone(&payer), None);
        assert_eq!(client.payer.pubkey(), payer.pubkey());
    }

    #[test]
    fn test_get_pdas() {
        let mint = Keypair::new();
        let global_pda = PumpFun::get_global_pda();
        let mint_authority_pda = PumpFun::get_mint_authority_pda();
        let bonding_curve_pda = PumpFun::get_bonding_curve_pda(&mint.pubkey());
        let metadata_pda = PumpFun::get_metadata_pda(&mint.pubkey());

        assert!(global_pda != Pubkey::default());
        assert!(mint_authority_pda != Pubkey::default());
        assert!(bonding_curve_pda.is_some());
        assert!(metadata_pda != Pubkey::default());
    }
}
