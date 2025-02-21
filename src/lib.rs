pub mod accounts;
pub mod constants;
pub mod error;
pub mod instruction;
pub mod utils;
pub mod jito;
pub mod grpc;
pub mod common;

use anyhow::anyhow;
use solana_client::{
    rpc_client::RpcClient,
    rpc_config::RpcSimulateTransactionConfig
};
use solana_sdk::{
    commitment_config::CommitmentConfig, compute_budget::ComputeBudgetInstruction, instruction::Instruction, native_token::sol_to_lamports, pubkey::Pubkey, signature::{Keypair, Signature}, signer::Signer, system_instruction, transaction::Transaction
};
use spl_associated_token_account::{
    get_associated_token_address,
    instruction::create_associated_token_account,
};

use common::{logs_data::TradeInfo, logs_events::PumpfunEvent, logs_subscribe};
use common::logs_subscribe::SubscriptionHandle;
use spl_token::instruction::close_account;

use std::sync::Arc;
use std::time::Instant;
use std::collections::HashMap;
use tokio::sync::RwLock;

use crate::jito::JitoClient;

use borsh::BorshDeserialize;

// Constants
const DEFAULT_SLIPPAGE: u64 = 1000; // 10%
const DEFAULT_COMPUTE_UNIT_LIMIT: u32 = 78000;
const DEFAULT_COMPUTE_UNIT_PRICE: u64 = 500000;
const JITO_TIP_AMOUNT: f64 = 0.00006;

// Cache
lazy_static::lazy_static! {
    static ref ACCOUNT_CACHE: RwLock<HashMap<Pubkey, Arc<accounts::GlobalAccount>>> = RwLock::new(HashMap::new());
    static ref BONDING_CURVE_CACHE: RwLock<HashMap<Pubkey, Arc<accounts::BondingCurveAccount>>> = RwLock::new(HashMap::new());
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PriorityFee {
    pub limit: Option<u32>,
    pub price: Option<u64>,
}

impl Default for PriorityFee {
    fn default() -> Self {
        Self { limit: Some(DEFAULT_COMPUTE_UNIT_LIMIT), price: Some(DEFAULT_COMPUTE_UNIT_PRICE) }
    }
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
    #[inline]
    pub fn new(
        rpc_url: String,
        commitment: Option<CommitmentConfig>,
        payer: Arc<Keypair>,
        jito_url: Option<String>,
    ) -> Self {
        let rpc = RpcClient::new_with_commitment(
            rpc_url,
            commitment.unwrap_or(CommitmentConfig::processed())
        );   

        let jito_client = jito_url.map(|url| JitoClient::new(&url, None));

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
    ) -> Result<Signature, anyhow::Error> {
        let ipfs = utils::create_token_metadata(metadata)
            .await
            .map_err(|e| anyhow!("Failed to upload metadata: {}", e))?;

        let mut instructions = self.create_priority_fee_instructions(priority_fee);

        instructions.push(instruction::create(
            &self.payer.clone(),
            mint,
            instruction::Create {
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
    ) -> Result<Signature, anyhow::Error> {
        if amount_sol == 0 {
            return Err(anyhow!("Amount cannot be zero"));
        }

        let ipfs = utils::create_token_metadata(metadata)
            .await
            .map_err(|e| anyhow!("Failed to upload metadata: {}", e))?;

        let global_account = self.get_global_account().await?;
        let buy_amount = global_account.get_initial_buy_price(amount_sol);
        let buy_amount_with_slippage =
            utils::calculate_with_slippage_buy(amount_sol, slippage_basis_points.unwrap_or(DEFAULT_SLIPPAGE));

        let mut instructions = self.create_priority_fee_instructions(priority_fee);

        instructions.push(instruction::create(
            &self.payer.clone(),
            mint,
            instruction::Create {
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
            instruction::Buy {
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
    ) -> Result<Signature, anyhow::Error> {
        if amount_sol == 0 {
            return Err(anyhow!("Amount cannot be zero"));
        }

        let global_account = self.get_global_account().await?;
        let bonding_curve_account = self.get_bonding_curve_account(mint).await?;
        let buy_amount = bonding_curve_account
            .get_buy_price(amount_sol)
            .map_err(|e| anyhow!(e))?;
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
            instruction::Buy {
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
        buy_token_amount: u64,
        max_sol_cost: u64,
        slippage_basis_points: Option<u64>,
        jito_fee: Option<f64>,
    ) -> Result<String, anyhow::Error> {
        if buy_token_amount == 0 || max_sol_cost == 0 {
            return Err(anyhow!("Amount cannot be zero"));
        }

        let start_time = Instant::now();

        let jito_client = self.jito_client.as_ref()
            .ok_or_else(|| anyhow!("Jito client not found"))?;

        let global_account = self.get_global_account().await?;
        let buy_amount_with_slippage =
            utils::calculate_with_slippage_buy(max_sol_cost, slippage_basis_points.unwrap_or(DEFAULT_SLIPPAGE));

        let mut instructions = self.create_priority_fee_instructions(None);
        let tip_account = jito_client.get_tip_account().await.map_err(|e| anyhow!(e))?;
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
            instruction::Buy {
                _amount: buy_token_amount,
                _max_sol_cost: buy_amount_with_slippage,
            },
        ));

        let jito_fee = jito_fee.unwrap_or(JITO_TIP_AMOUNT);
        instructions.push(
            system_instruction::transfer(
                &self.payer.pubkey(),
                &tip_account,
                sol_to_lamports(jito_fee),
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
    ) -> Result<(), anyhow::Error> {
        let ata = get_associated_token_address(&self.payer.pubkey(), mint);
        let balance = self.rpc.get_token_account_balance(&ata)?;
        let balance_u64 = balance.amount.parse::<u64>()
            .map_err(|_| anyhow!("Failed to parse token balance"))?;
        let amount = amount_token.unwrap_or(balance_u64);
        
        if amount == 0 {
            return Err(anyhow!("Amount cannot be zero"));
        }

        let global_account = self.get_global_account().await?;
        let bonding_curve_account = self.get_bonding_curve_account(mint).await?;
        let min_sol_output = bonding_curve_account
            .get_sell_price(amount, global_account.fee_basis_points)
            .map_err(|e| anyhow!(e))?;
        let min_sol_output_with_slippage = utils::calculate_with_slippage_sell(
            min_sol_output,
            slippage_basis_points.unwrap_or(DEFAULT_SLIPPAGE),
        );

        let mut instructions = vec![
            ComputeBudgetInstruction::set_compute_unit_limit(1_400_000),
            ComputeBudgetInstruction::set_compute_unit_price(0),
        ];

        instructions.push(instruction::sell(
            &self.payer.clone(),
            mint,
            &global_account.fee_recipient,
            instruction::Sell {
                _amount: amount,
                _min_sol_output: min_sol_output_with_slippage,
            },
        ));

        instructions.push(close_account(
            &spl_token::ID,
            &ata,
            &self.payer.pubkey(),
            &self.payer.pubkey(),
            &[&self.payer.pubkey()],
        )?);

        let commitment_config = CommitmentConfig::confirmed();
        let recent_blockhash = self.rpc.get_latest_blockhash_with_commitment(commitment_config)?
            .0;

        let simulate_tx = Transaction::new_signed_with_payer(
            &instructions,
            Some(&self.payer.pubkey()),
            &[&self.payer.clone()],
            recent_blockhash,
        );

        let config = RpcSimulateTransactionConfig {
            sig_verify: true,
            commitment: Some(commitment_config),
            ..RpcSimulateTransactionConfig::default()
        };
        
        let result = self.rpc.simulate_transaction_with_config(&simulate_tx, config)?
            .value;

        if result.logs.as_ref().map_or(true, |logs| logs.is_empty()) {
            return Err(anyhow!("Simulation failed: {:?}", result.err));
        }

        let result_cu = result.units_consumed.ok_or_else(|| anyhow!("No compute units consumed"))?;
        let fees = self.rpc.get_recent_prioritization_fees(&[])?;
        let average_fees = if fees.is_empty() {
            DEFAULT_COMPUTE_UNIT_PRICE
        } else {
            fees.iter()
                .map(|fee| fee.prioritization_fee)
                .sum::<u64>() / fees.len() as u64
        };

        let unit_price = match priority_fee {
            None => average_fees,
            Some(pf) => pf.price.unwrap_or(DEFAULT_COMPUTE_UNIT_PRICE)
        };

        let unit_price = if unit_price == 0 { DEFAULT_COMPUTE_UNIT_PRICE } else { unit_price };

        instructions[0] = ComputeBudgetInstruction::set_compute_unit_limit(result_cu as u32);
        instructions[1] = ComputeBudgetInstruction::set_compute_unit_price(unit_price);

        let transaction = Transaction::new_signed_with_payer(
            &instructions,
            Some(&self.payer.pubkey()),
            &[&self.payer.clone()],
            recent_blockhash,
        );

        self.rpc.send_and_confirm_transaction(&transaction)?;
        Ok(())
    }

    /// Sell tokens by percentage
    pub async fn sell_by_percent(
        &self,
        mint: &Pubkey,
        percent: u64,
        slippage_basis_points: Option<u64>,
        priority_fee: Option<PriorityFee>,
    ) -> Result<(), anyhow::Error> {
        if percent == 0 || percent > 100 {
            return Err(anyhow!("Percentage must be between 1 and 100"));
        }

        let ata = get_associated_token_address(&self.payer.pubkey(), mint);
        let balance = self.rpc.get_token_account_balance(&ata)?;
        let balance_u64 = balance.amount.parse::<u64>()
            .map_err(|_| anyhow!("Failed to parse token balance"))?;
        
        if balance_u64 == 0 {
            return Err(anyhow!("Balance is 0"));
        }

        let amount = balance_u64 * percent / 100;
        self.sell(mint, Some(amount), slippage_basis_points, priority_fee).await
    }

    pub async fn sell_by_percent_with_jito(
        &self,
        mint: &Pubkey,
        percent: u64,
        slippage_basis_points: Option<u64>,
        jito_fee: Option<f64>,
    ) -> Result<String, anyhow::Error> {
        if percent == 0 || percent > 100 {
            return Err(anyhow!("Percentage must be between 1 and 100"));
        }

        let ata = get_associated_token_address(&self.payer.pubkey(), mint);
        let balance = self.rpc.get_token_account_balance(&ata)?;
        let balance_u64 = balance.amount.parse::<u64>()
            .map_err(|_| anyhow!("Failed to parse token balance"))?;
        
        if balance_u64 == 0 {
            return Err(anyhow!("Balance is 0"));
        }

        let amount = balance_u64 * percent / 100;
        self.sell_with_jito(mint, Some(amount), slippage_basis_points, jito_fee).await
    }

    /// Sell tokens using Jito
    pub async fn sell_with_jito(
        &self,
        mint: &Pubkey,
        amount_token: Option<u64>,
        slippage_basis_points: Option<u64>,
        jito_fee: Option<f64>,
    ) -> Result<String, anyhow::Error> {
        let start_time = Instant::now();

        let jito_client = self.jito_client.as_ref()
            .ok_or_else(|| anyhow!("Jito client not found"))?;

        let ata = get_associated_token_address(&self.payer.pubkey(), mint);
        let balance = self.rpc.get_token_account_balance(&ata)?;
        let balance_u64 = balance.amount.parse::<u64>()
            .map_err(|_| anyhow!("Failed to parse token balance"))?;
        let amount = amount_token.unwrap_or(balance_u64);

        if amount == 0 {
            return Err(anyhow!("Amount cannot be zero"));
        }

        let global_account = self.get_global_account().await?;
        let bonding_curve_account = self.get_bonding_curve_account(mint).await?;
        let min_sol_output = bonding_curve_account
            .get_sell_price(amount, global_account.fee_basis_points)
            .map_err(|e| anyhow!(e))?;
        let min_sol_output_with_slippage = utils::calculate_with_slippage_sell(
            min_sol_output,
            slippage_basis_points.unwrap_or(DEFAULT_SLIPPAGE),
        );

        let mut instructions = self.create_priority_fee_instructions(None);
        let tip_account = jito_client.get_tip_account().await.map_err(|e| anyhow!(e))?;
        instructions.push(instruction::sell(
            &self.payer.clone(),
            mint,
            &global_account.fee_recipient,
            instruction::Sell {
                _amount: amount,
                _min_sol_output: min_sol_output_with_slippage,
            },
        ));

        instructions.push(close_account(
            &spl_token::ID,
            &ata,
            &self.payer.pubkey(),
            &self.payer.pubkey(),
            &[&self.payer.pubkey()],
        )?);

        let jito_fee = jito_fee.unwrap_or(JITO_TIP_AMOUNT);
        instructions.push(
            system_instruction::transfer(
                &self.payer.pubkey(),
                &tip_account,
                sol_to_lamports(jito_fee),
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

    pub async fn transfer_sol(&self, receive_wallet: &Pubkey, amount: u64) -> Result<(), anyhow::Error> {
        if amount == 0 {
            return Err(anyhow!("Amount cannot be zero"));
        }

        let balance = self.get_payer_sol_balance()?;
        if balance < amount {
            return Err(anyhow!("Insufficient balance"));
        }

        let transfer_instruction = system_instruction::transfer(
            &self.payer.pubkey(),
            receive_wallet,
            amount,
        );
    
        let recent_blockhash = self.rpc.get_latest_blockhash()?;
    
        let transaction = Transaction::new_signed_with_payer(
            &[transfer_instruction],
            Some(&self.payer.pubkey()),
            &[&self.payer.clone()],
            recent_blockhash,
        );  
    
        self.rpc.send_and_confirm_transaction(&transaction)?;
    
        Ok(())
    }

    // Helper methods
    #[inline]
    fn create_priority_fee_instructions(&self, priority_fee: Option<PriorityFee>) -> Vec<Instruction> {
        let mut instructions = Vec::with_capacity(2);
        let fee = priority_fee.unwrap_or(PriorityFee::default());
        if let Some(limit) = fee.limit {
            instructions.push(ComputeBudgetInstruction::set_compute_unit_limit(limit));
        }
        if let Some(price) = fee.price {
            instructions.push(ComputeBudgetInstruction::set_compute_unit_price(price));
        }
        
        instructions
    }

    #[inline]
    pub fn get_rpc(&self) -> &RpcClient {
        &self.rpc
    }

    #[inline]
    pub fn get_payer_pubkey(&self) -> Pubkey {
        self.payer.pubkey()
    }

    #[inline]
    pub fn get_token_balance(&self, account: &Pubkey, mint: &Pubkey) -> Result<u64, anyhow::Error> {
        let ata = get_associated_token_address(account, mint);
        if self.rpc.get_account(&ata).is_err() {
            return Ok(0);
        }

        let balance = self.rpc.get_token_account_balance(&ata)?;
        balance.amount.parse::<u64>()
            .map_err(|_| anyhow!("Failed to parse token balance"))
    }

    #[inline]
    pub fn get_sol_balance(&self, account: &Pubkey) -> Result<u64, anyhow::Error> {
        self.rpc.get_balance(account).map_err(|_| anyhow!("Failed to get SOL balance"))
    }

    #[inline]
    pub fn get_payer_token_balance(&self, mint: &Pubkey) -> Result<u64, anyhow::Error> {
        self.get_token_balance(&self.payer.pubkey(), mint)
    }

    #[inline]
    pub fn get_payer_sol_balance(&self) -> Result<u64, anyhow::Error> {
        self.get_sol_balance(&self.payer.pubkey())
    }

    #[inline]
    pub fn get_global_pda() -> Pubkey {
        static GLOBAL_PDA: once_cell::sync::Lazy<Pubkey> = once_cell::sync::Lazy::new(|| {
            Pubkey::find_program_address(&[constants::seeds::GLOBAL_SEED], &constants::accounts::PUMPFUN).0
        });
        *GLOBAL_PDA
    }

    #[inline]
    pub fn get_mint_authority_pda() -> Pubkey {
        static MINT_AUTHORITY_PDA: once_cell::sync::Lazy<Pubkey> = once_cell::sync::Lazy::new(|| {
            Pubkey::find_program_address(&[constants::seeds::MINT_AUTHORITY_SEED], &constants::accounts::PUMPFUN).0
        });
        *MINT_AUTHORITY_PDA
    }

    #[inline]
    pub fn get_bonding_curve_pda(mint: &Pubkey) -> Option<Pubkey> {
        Pubkey::try_find_program_address(
            &[constants::seeds::BONDING_CURVE_SEED, mint.as_ref()],
            &constants::accounts::PUMPFUN
        ).map(|(pubkey, _)| pubkey)
    }

    #[inline]
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

    #[inline]
    pub async fn get_global_account(&self) -> Result<Arc<accounts::GlobalAccount>, anyhow::Error> {
        let global = Self::get_global_pda();
        
        // Try cache first
        if let Some(account) = ACCOUNT_CACHE.read().await.get(&global) {
            return Ok(account.clone());
        }

        // Cache miss, fetch from RPC
        let account = self.rpc.get_account(&global)?;
        let global_account = Arc::new(accounts::GlobalAccount::try_from_slice(&account.data)?);
        
        // Update cache
        ACCOUNT_CACHE.write().await.insert(global, global_account.clone());
        
        Ok(global_account)
    }

    #[inline]
    pub async fn get_bonding_curve_account(
        &self,
        mint: &Pubkey,
    ) -> Result<Arc<accounts::BondingCurveAccount>, anyhow::Error> {
        let bonding_curve_pda = Self::get_bonding_curve_pda(mint)
            .ok_or(anyhow!("Bonding curve not found"))?;

        // Try cache first  
        if let Some(account) = BONDING_CURVE_CACHE.read().await.get(&bonding_curve_pda) {
            return Ok(account.clone());
        }

        // Cache miss, fetch from RPC
        let account = self.rpc.get_account(&bonding_curve_pda)?;
        let bonding_curve = Arc::new(accounts::BondingCurveAccount::try_from_slice(&account.data)?);
        
        // Update cache
        BONDING_CURVE_CACHE.write().await.insert(bonding_curve_pda, bonding_curve.clone());

        Ok(bonding_curve)
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
    pub fn get_buy_amount_with_slippage(&self, amount_sol: u64, slippage_basis_points: Option<u64>) -> u64 {
        utils::calculate_with_slippage_buy(amount_sol, slippage_basis_points.unwrap_or(DEFAULT_SLIPPAGE))
    }

    #[inline]
    pub fn get_token_price(&self, virtual_sol_reserves: u64, virtual_token_reserves: u64) -> f64 {
        let v_sol = virtual_sol_reserves as f64 / 100_000_000.0;
        let v_tokens = virtual_token_reserves as f64 / 100_000.0;
        v_sol / v_tokens
    }

    #[inline]
    pub fn get_buy_price(&self, amount: u64, trade_info: &TradeInfo) -> Result<u64, &'static str> {
        if amount == 0 {
            return Ok(0);
        }

        let n: u128 = (trade_info.virtual_sol_reserves as u128) * (trade_info.virtual_token_reserves as u128);
        let i: u128 = (trade_info.virtual_sol_reserves as u128) + (amount as u128);
        let r: u128 = n / i + 1;
        let s: u128 = (trade_info.virtual_token_reserves as u128) - r;
        let s_u64 = s as u64;
        
        Ok(s_u64.min(trade_info.real_token_reserves))
    }
}
