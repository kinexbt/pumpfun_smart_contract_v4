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

pub mod logs_data;
pub mod logs_parser;
pub mod logs_filters;
pub mod logs_events;
pub mod logs_process;   
pub mod logs_subscribe;

pub use logs_data::*;
pub use logs_parser::*;
pub use logs_filters::*;
pub use logs_events::*;
pub use logs_process::*;
pub use logs_subscribe::*;

use crate::{constants, PumpFun};
use anchor_client::anchor_lang::InstructionData;
use anchor_spl::associated_token::get_associated_token_address;
use pumpfun_cpi as cpi;
use solana_sdk::{
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
    signature::Keypair,
    signer::Signer,
};

/// Creates an instruction to create a new token with bonding curve
///
/// Creates a new SPL token with an associated bonding curve that determines its price.
///
/// # Arguments
///
/// * `payer` - Keypair that will pay for account creation and transaction fees
/// * `mint` - Keypair for the new token mint account that will be created
/// * `args` - Create instruction data containing token name, symbol and metadata URI
///
/// # Returns
///
/// Returns a Solana instruction that when executed will create the token and its accounts
pub fn create(payer: &Keypair, mint: &Keypair, args: cpi::instruction::Create) -> Instruction {
    let bonding_curve: Pubkey = PumpFun::get_bonding_curve_pda(&mint.pubkey()).unwrap();
    Instruction::new_with_bytes(
        constants::accounts::PUMPFUN,
        &args.data(),
        vec![
            AccountMeta::new(mint.pubkey(), true),
            AccountMeta::new(PumpFun::get_mint_authority_pda(), false),
            AccountMeta::new(bonding_curve, false),
            AccountMeta::new(
                get_associated_token_address(&bonding_curve, &mint.pubkey()),
                false,
            ),
            AccountMeta::new_readonly(PumpFun::get_global_pda(), false),
            AccountMeta::new_readonly(constants::accounts::MPL_TOKEN_METADATA, false),
            AccountMeta::new(PumpFun::get_metadata_pda(&mint.pubkey()), false),
            AccountMeta::new(payer.pubkey(), true),
            AccountMeta::new_readonly(constants::accounts::SYSTEM_PROGRAM, false),
            AccountMeta::new_readonly(constants::accounts::TOKEN_PROGRAM, false),
            AccountMeta::new_readonly(constants::accounts::ASSOCIATED_TOKEN_PROGRAM, false),
            AccountMeta::new_readonly(constants::accounts::RENT, false),
            AccountMeta::new_readonly(constants::accounts::EVENT_AUTHORITY, false),
            AccountMeta::new_readonly(constants::accounts::PUMPFUN, false),
        ],
    )
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
    fee_recipient: &Pubkey,
    args: cpi::instruction::Buy,
) -> Instruction {
    let bonding_curve: Pubkey = PumpFun::get_bonding_curve_pda(mint).unwrap();
    Instruction::new_with_bytes(
        constants::accounts::PUMPFUN,
        &args.data(),
        vec![
            AccountMeta::new_readonly(PumpFun::get_global_pda(), false),
            AccountMeta::new(*fee_recipient, false),
            AccountMeta::new_readonly(*mint, false),
            AccountMeta::new(bonding_curve, false),
            AccountMeta::new(get_associated_token_address(&bonding_curve, mint), false),
            AccountMeta::new(get_associated_token_address(&payer.pubkey(), mint), false),
            AccountMeta::new(payer.pubkey(), true),
            AccountMeta::new_readonly(constants::accounts::SYSTEM_PROGRAM, false),
            AccountMeta::new_readonly(constants::accounts::TOKEN_PROGRAM, false),
            AccountMeta::new_readonly(constants::accounts::RENT, false),
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
    fee_recipient: &Pubkey,
    args: cpi::instruction::Sell,
) -> Instruction {
    let bonding_curve: Pubkey = PumpFun::get_bonding_curve_pda(mint).unwrap();
    Instruction::new_with_bytes(
        constants::accounts::PUMPFUN,
        &args.data(),
        vec![
            AccountMeta::new_readonly(PumpFun::get_global_pda(), false),
            AccountMeta::new(*fee_recipient, false),
            AccountMeta::new_readonly(*mint, false),
            AccountMeta::new(bonding_curve, false),
            AccountMeta::new(get_associated_token_address(&bonding_curve, mint), false),
            AccountMeta::new(get_associated_token_address(&payer.pubkey(), mint), false),
            AccountMeta::new(payer.pubkey(), true),
            AccountMeta::new_readonly(constants::accounts::SYSTEM_PROGRAM, false),
            AccountMeta::new_readonly(constants::accounts::ASSOCIATED_TOKEN_PROGRAM, false),
            AccountMeta::new_readonly(constants::accounts::TOKEN_PROGRAM, false),
            AccountMeta::new_readonly(constants::accounts::EVENT_AUTHORITY, false),
            AccountMeta::new_readonly(constants::accounts::PUMPFUN, false),
        ],
    )
}
