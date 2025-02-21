use anyhow::anyhow;
use tokio::sync::RwLock;
use std::{collections::HashMap, sync::Arc};
use solana_client::rpc_client::RpcClient;
use solana_sdk::{
    compute_budget::ComputeBudgetInstruction, instruction::Instruction, native_token::sol_to_lamports, pubkey::Pubkey, signature::Keypair, signer::Signer, system_instruction, transaction::Transaction
};
use spl_associated_token_account::get_associated_token_address;
use crate::{accounts, common::logs_data::TradeInfo, constants::{self, trade::{DEFAULT_COMPUTE_UNIT_LIMIT, DEFAULT_COMPUTE_UNIT_PRICE, DEFAULT_SLIPPAGE}}};
use borsh::BorshDeserialize;

lazy_static::lazy_static! {
    static ref ACCOUNT_CACHE: RwLock<HashMap<Pubkey, Arc<accounts::GlobalAccount>>> = RwLock::new(HashMap::new());
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PriorityFee {
    pub limit: f64,
    pub price: f64,
    pub jito_fee: Option<f64>,
}

impl Default for PriorityFee {
    fn default() -> Self {
        Self { limit: DEFAULT_COMPUTE_UNIT_LIMIT, price: DEFAULT_COMPUTE_UNIT_PRICE, jito_fee: None }
    }
}

pub async fn transfer_sol(rpc: &RpcClient, payer: &Keypair, receive_wallet: &Pubkey, amount: u64) -> Result<(), anyhow::Error> {
    if amount == 0 {
        return Err(anyhow!("Amount cannot be zero"));
    }

    let balance = get_sol_balance(rpc, &payer.pubkey())?;
    if balance < amount {
        return Err(anyhow!("Insufficient balance"));
    }

    let transfer_instruction = system_instruction::transfer(
        &payer.pubkey(),
        receive_wallet,
        amount,
    );

    let recent_blockhash = rpc.get_latest_blockhash()?;

    let transaction = Transaction::new_signed_with_payer(
        &[transfer_instruction],
        Some(&payer.pubkey()),
        &[payer],
        recent_blockhash,
    );  

    rpc.send_and_confirm_transaction(&transaction)?;

    Ok(())
}

#[inline]
pub fn create_priority_fee_instructions(priority_fee: PriorityFee) -> Vec<Instruction> {
    let mut instructions = Vec::with_capacity(2);
    instructions.push(ComputeBudgetInstruction::set_compute_unit_limit(sol_to_lamports(priority_fee.limit) as u32));
    instructions.push(ComputeBudgetInstruction::set_compute_unit_price(sol_to_lamports(priority_fee.price)));
    
    instructions
}

pub fn get_token_balance(rpc: &RpcClient, account: &Pubkey, mint: &Pubkey) -> Result<u64, anyhow::Error> {
    let ata = get_associated_token_address(account, mint);
    if rpc.get_account(&ata).is_err() {
        return Ok(0);
    }

    let balance = rpc.get_token_account_balance(&ata)?;
    balance.amount.parse::<u64>()
        .map_err(|_| anyhow!("Failed to parse token balance"))
}

pub fn get_sol_balance(rpc: &RpcClient, account: &Pubkey) -> Result<u64, anyhow::Error> {
    rpc.get_balance(account).map_err(|_| anyhow!("Failed to get SOL balance"))
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
    let seeds: &[&[u8]; 2] = &[constants::seeds::BONDING_CURVE_SEED, mint.as_ref()];
    let program_id: &Pubkey = &constants::accounts::PUMPFUN;
    let pda: Option<(Pubkey, u8)> = Pubkey::try_find_program_address(seeds, program_id);
    pda.map(|pubkey| pubkey.0)
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
pub async fn get_global_account(rpc: &RpcClient) -> Result<Arc<accounts::GlobalAccount>, anyhow::Error> {
    let global = get_global_pda();
    
    // Try cache first
    if let Some(account) = ACCOUNT_CACHE.read().await.get(&global) {
        return Ok(account.clone());
    }

    // Cache miss, fetch from RPC
    let account = rpc.get_account(&global)?;
    let global_account = Arc::new(accounts::GlobalAccount::try_from_slice(&account.data)?);
    
    // Update cache
    ACCOUNT_CACHE.write().await.insert(global, global_account.clone());
    
    Ok(global_account)
}

#[inline]
pub async fn get_initial_buy_price(global_account: &Arc<accounts::GlobalAccount>, amount_sol: u64) -> Result<u64, anyhow::Error> {
    let buy_amount = global_account.get_initial_buy_price(amount_sol);
    Ok(buy_amount)
}

#[inline]
pub async fn get_bonding_curve_account(
    rpc: &RpcClient,
    mint: &Pubkey,
) -> Result<Arc<accounts::BondingCurveAccount>, anyhow::Error> {
    let bonding_curve_pda = get_bonding_curve_pda(mint)
        .ok_or(anyhow!("Bonding curve not found"))?;
    
    if rpc.get_account(&bonding_curve_pda).is_err() {
        return Err(anyhow!("Bonding curve not found"));
    }

    let account = rpc.get_account(&bonding_curve_pda)?;
    let bonding_curve = Arc::new(accounts::BondingCurveAccount::try_from_slice(&account.data)?);
    Ok(bonding_curve)
}

#[inline]
pub fn get_buy_amount_with_slippage(amount_sol: u64, slippage_basis_points: Option<u64>) -> u64 {
    let slippage = slippage_basis_points.unwrap_or(DEFAULT_SLIPPAGE);
    amount_sol + (amount_sol * slippage / 10000)
}

pub fn get_token_price(virtual_sol_reserves: u64, virtual_token_reserves: u64) -> f64 {
    let v_sol = virtual_sol_reserves as f64 / 100_000_000.0;
    let v_tokens = virtual_token_reserves as f64 / 100_000.0;
    v_sol / v_tokens
}

pub fn get_buy_price(amount: u64, trade_info: &TradeInfo) -> u64 {
    if amount == 0 {
        return 0;
    }

    let n: u128 = (trade_info.virtual_sol_reserves as u128) * (trade_info.virtual_token_reserves as u128);
    let i: u128 = (trade_info.virtual_sol_reserves as u128) + (amount as u128);
    let r: u128 = n / i + 1;
    let s: u128 = (trade_info.virtual_token_reserves as u128) - r;
    let s_u64 = s as u64;
    
    s_u64.min(trade_info.real_token_reserves)
}

#[inline]
pub fn calculate_with_slippage_buy(amount: u64, basis_points: u64) -> u64 {
    amount + (amount * basis_points) / 10000
}

#[inline]
pub fn calculate_with_slippage_sell(amount: u64, basis_points: u64) -> u64 {
    amount - (amount * basis_points) / 10000
}
