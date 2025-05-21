//! Instructions for interacting with the Pump.fun program.
//!
//! This module contains instruction builders for creating Solana instructions to interact with the
//! Pump.fun program. Each function takes the required accounts and instruction data and returns a
//! properly formatted Solana instruction.
//!
//! # Instructions
//!
//! - `create`: Instruction to create a new token with an associated bonding curve.
//! - `buy`: Instruction to buy tokens from a bonding curve by providing SOL.
//! - `sell`: Instruction to sell tokens back to the bonding curve in exchange for SOL.
use crate::{
    constants, 
    pumpfun::common::{
        get_bonding_curve_pda, get_global_pda, get_metadata_pda, get_mint_authority_pda
    },
};
use spl_associated_token_account::get_associated_token_address;

use solana_sdk::{
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
    signature::Keypair,
    signer::Signer,
};

pub struct Buy {
    pub _amount: u64,
    pub _max_sol_cost: u64,
}

impl Buy {
    pub fn data(&self) -> Vec<u8> {
        let mut data = Vec::with_capacity(8 + 8 + 8);
        data.extend_from_slice(&[102, 6, 61, 18, 1, 218, 235, 234]); // discriminator
        data.extend_from_slice(&self._amount.to_le_bytes());
        data.extend_from_slice(&self._max_sol_cost.to_le_bytes());
        data
    }
}

pub struct Sell {
    pub _amount: u64,
    pub _min_sol_output: u64,
}

impl Sell {
    pub fn data(&self) -> Vec<u8> {
        let mut data = Vec::with_capacity(8 + 8 + 8);
        data.extend_from_slice(&[51, 230, 133, 164, 1, 127, 131, 173]); // discriminator
        data.extend_from_slice(&self._amount.to_le_bytes());
        data.extend_from_slice(&self._min_sol_output.to_le_bytes());
        data
    }
}

/// Creates an instruction to buy tokens from a bonding curve
///
/// Buys tokens by providing SOL. The amount of tokens received is calculated based on
/// the bonding curve formula. A portion of the SOL is taken as a fee and sent to the
/// fee recipient account.
///
/// # Arguments
///
/// * `payer` - Keypair that will provide the SOL to buy tokens
/// * `mint` - Public key of the token mint to buy
/// * `fee_recipient` - Public key of the account that will receive the transaction fee
/// * `args` - Buy instruction data containing the SOL amount and maximum acceptable token price
///
/// # Returns
///
/// Returns a Solana instruction that when executed will buy tokens from the bonding curve
pub fn buy(
    payer: &Keypair,
    mint: &Pubkey,
    bonding_curve: &Pubkey,
    creator_vault: &Pubkey,
    fee_recipient: &Pubkey,
    args: Buy,
) -> Instruction {
    Instruction::new_with_bytes(
        constants::accounts::PUMPFUN,
        &args.data(),
        vec![
            AccountMeta::new_readonly(constants::global_constants::GLOBAL_ACCOUNT, false),
            AccountMeta::new(*fee_recipient, false),
            AccountMeta::new_readonly(*mint, false),
            AccountMeta::new(*bonding_curve, false),
            AccountMeta::new(get_associated_token_address(bonding_curve, mint), false),
            AccountMeta::new(get_associated_token_address(&payer.pubkey(), mint), false),
            AccountMeta::new(payer.pubkey(), true),
            AccountMeta::new_readonly(constants::accounts::SYSTEM_PROGRAM, false),
            AccountMeta::new_readonly(constants::accounts::TOKEN_PROGRAM, false),
            AccountMeta::new(*creator_vault, false),
            AccountMeta::new_readonly(constants::accounts::EVENT_AUTHORITY, false),
            AccountMeta::new_readonly(constants::accounts::PUMPFUN, false),
        ],
    )
}

/// Creates an instruction to sell tokens back to a bonding curve
///
/// Sells tokens back to the bonding curve in exchange for SOL. The amount of SOL received
/// is calculated based on the bonding curve formula. A portion of the SOL is taken as
/// a fee and sent to the fee recipient account.
///
/// # Arguments
///
/// * `payer` - Keypair that owns the tokens to sell
/// * `mint` - Public key of the token mint to sell
/// * `fee_recipient` - Public key of the account that will receive the transaction fee
/// * `args` - Sell instruction data containing token amount and minimum acceptable SOL output
///
/// # Returns
///
/// Returns a Solana instruction that when executed will sell tokens to the bonding curve
pub fn sell(
    payer: &Keypair,
    mint: &Pubkey,
    bonding_curve: &Pubkey,
    creator_vault: &Pubkey,
    fee_recipient: &Pubkey,
    args: Sell,
) -> Instruction {
    Instruction::new_with_bytes(
        constants::accounts::PUMPFUN,
        &args.data(),
        vec![
            AccountMeta::new_readonly(constants::global_constants::GLOBAL_ACCOUNT, false),
            AccountMeta::new(*fee_recipient, false),
            AccountMeta::new_readonly(*mint, false),
            AccountMeta::new(*bonding_curve, false),
            AccountMeta::new(get_associated_token_address(&bonding_curve, mint), false),
            AccountMeta::new(get_associated_token_address(&payer.pubkey(), mint), false),
            AccountMeta::new(payer.pubkey(), true),
            AccountMeta::new_readonly(constants::accounts::SYSTEM_PROGRAM, false),
            AccountMeta::new(*creator_vault, false),
            AccountMeta::new_readonly(constants::accounts::TOKEN_PROGRAM, false),
            AccountMeta::new_readonly(constants::accounts::EVENT_AUTHORITY, false),
            AccountMeta::new_readonly(constants::accounts::PUMPFUN, false),
        ],
    )
}

