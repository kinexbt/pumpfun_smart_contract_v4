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

use std::sync::Arc;

use spl_associated_token_account::instruction::create_associated_token_account;
use spl_token::instruction::close_account;
use crate::common::SolanaRpcClient;
use crate::constants::trade::DEFAULT_SLIPPAGE;
use crate::ipfs::TokenMetadataIPFS;
use crate::pumpfun::common::{calculate_with_slippage_buy, calculate_with_slippage_sell, get_bonding_curve_account, get_buy_amount_with_slippage, get_global_account, get_initial_buy_price, get_token_balance, get_token_balance_and_ata};
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

use anyhow::{anyhow, Result};
pub struct Create {
    pub _name: String,
    pub _symbol: String,
    pub _uri: String,
}

impl Create {
    pub fn data(&self) -> Vec<u8> {
        let mut data = Vec::with_capacity(8 + 4 + self._name.len() + 4 + self._symbol.len() + 4 + self._uri.len());

        // 追加 discriminator
        data.extend_from_slice(&[24, 30, 200, 40, 5, 28, 7, 119]); // discriminator

        // 添加 name 字符串长度和内容
        data.extend_from_slice(&(self._name.len() as u32).to_le_bytes());  // 添加 name 长度
        data.extend_from_slice(self._name.as_bytes());  // 添加 name 内容

        // 添加 symbol 字符串长度和内容
        data.extend_from_slice(&(self._symbol.len() as u32).to_le_bytes());  // 添加 symbol 长度
        data.extend_from_slice(self._symbol.as_bytes());  // 添加 symbol 内容

        // 添加 uri 字符串长度和内容
        data.extend_from_slice(&(self._uri.len() as u32).to_le_bytes());  // 添加 uri 长度
        data.extend_from_slice(self._uri.as_bytes());  // 添加 uri 内容

        data
    }
}

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
pub fn create(payer: &Keypair, mint: &Keypair, args: Create) -> Instruction {
    let bonding_curve: Pubkey = get_bonding_curve_pda(&mint.pubkey()).unwrap();
    Instruction::new_with_bytes(
        constants::accounts::PUMPFUN,
        &args.data(),
        vec![
            AccountMeta::new(mint.pubkey(), true),
            AccountMeta::new(get_mint_authority_pda(), false),
            AccountMeta::new(bonding_curve, false),
            AccountMeta::new(
                get_associated_token_address(&bonding_curve, &mint.pubkey()),
                false,
            ),
            AccountMeta::new_readonly(get_global_pda(), false),
            AccountMeta::new_readonly(constants::accounts::MPL_TOKEN_METADATA, false),
            AccountMeta::new(get_metadata_pda(&mint.pubkey()), false),
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
    args: Buy,
) -> Instruction {
    let bonding_curve: Pubkey = get_bonding_curve_pda(mint).unwrap();
    Instruction::new_with_bytes(
        constants::accounts::PUMPFUN,
        &args.data(),
        vec![
            AccountMeta::new_readonly(get_global_pda(), false),
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
    args: Sell,
) -> Instruction {
    let bonding_curve: Pubkey = get_bonding_curve_pda(mint).unwrap();
    Instruction::new_with_bytes(
        constants::accounts::PUMPFUN,
        &args.data(),
        vec![
            AccountMeta::new_readonly(get_global_pda(), false),
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

pub async fn build_create_and_buy_instructions(
    rpc: Arc<SolanaRpcClient>,
    payer: Arc<Keypair>,
    mint: Arc<Keypair>,
    ipfs: TokenMetadataIPFS,
    amount_sol: u64,
    slippage_basis_points: Option<u64>,
) -> Result<Vec<Instruction>, anyhow::Error> {
    if amount_sol == 0 {
        return Err(anyhow!("build_create_and_buy_instructions: Amount cannot be zero"));
    }

    let rpc = rpc.as_ref();
    let global_account = get_global_account(&rpc).await?;
    let buy_amount = global_account.get_initial_buy_price(amount_sol);
    let buy_amount_with_slippage =
        get_buy_amount_with_slippage(amount_sol, slippage_basis_points);

    let mut instructions = vec![];

    instructions.push(create(
        payer.as_ref(),
        mint.as_ref(),
        Create {
            _name: ipfs.metadata.name.clone(),
            _symbol: ipfs.metadata.symbol.clone(),
            _uri: ipfs.metadata_uri.clone(),
        },
    ));

    let ata = get_associated_token_address(&payer.pubkey(), &mint.pubkey());
    instructions.push(create_associated_token_account(
        &payer.pubkey(),
        &payer.pubkey(),
        &mint.pubkey(),
        &constants::accounts::TOKEN_PROGRAM,
    ));
    
    instructions.push(buy(
        payer.as_ref(),
        &mint.pubkey(),
        &global_account.fee_recipient,
        Buy {
            _amount: buy_amount,
            _max_sol_cost: buy_amount_with_slippage,
        },
    ));

    Ok(instructions)
}

pub async fn build_buy_instructions(
    rpc: Arc<SolanaRpcClient>,
    payer: Arc<Keypair>,
    mint: Arc<Pubkey>,
    amount_sol: u64,
    slippage_basis_points: Option<u64>,
) -> Result<Vec<Instruction>, anyhow::Error> {
    if amount_sol == 0 {
        return Err(anyhow!("build_buy_instructions:Amount cannot be zero"));
    }

    let global_account = get_global_account(&rpc).await?;
    let buy_amount = match get_bonding_curve_account(&rpc, mint.as_ref()).await {
        Ok(account) => {
            account.get_buy_price(amount_sol).map_err(|e| anyhow!(e))?
        },
        Err(_e) => {
            let initial_buy_amount = get_initial_buy_price(&global_account, amount_sol).await?;
            initial_buy_amount * 80 / 100
        }
    };

    let buy_amount_with_slippage = calculate_with_slippage_buy(amount_sol, slippage_basis_points.unwrap_or(DEFAULT_SLIPPAGE));
    let mut instructions = vec![];
    // let ata = get_associated_token_address(&payer.pubkey(), &mint);
    // match rpc.get_account(&ata).await {
    //     Ok(_) => {},
    //     Err(_) => {
    //         instructions.push(create_associated_token_account(
    //             &payer.pubkey(),
    //             &payer.pubkey(),
    //             &mint,
    //             &constants::accounts::TOKEN_PROGRAM,
    //         ));
    //     }
    // }

    instructions.push(create_associated_token_account(
        &payer.pubkey(),
        &payer.pubkey(),
        &mint,
        &constants::accounts::TOKEN_PROGRAM,
    ));

    instructions.push(buy(
        payer.as_ref(),
        &mint,
        &global_account.fee_recipient,
        Buy {
            _amount: buy_amount,
            _max_sol_cost: buy_amount_with_slippage,
        },
    ));

    Ok(instructions)
}

pub async fn build_sell_instructions(
    rpc: Arc<SolanaRpcClient>,
    payer: Arc<Keypair>,
    mint: Arc<Pubkey>,
    amount_token: u64,
    slippage_basis_points: Option<u64>,
) -> Result<Vec<Instruction>, anyhow::Error> {
    if amount_token == 0 {
        return Err(anyhow!("build_sell_instructions: Amount cannot be zero"));
    }

    let ata = get_associated_token_address(&payer.pubkey(), mint.as_ref());
    let global_account = get_global_account(&rpc).await?;
    let bonding_curve_account = get_bonding_curve_account(&rpc, mint.as_ref()).await?;
    let min_sol_output = bonding_curve_account
        .get_sell_price(amount_token, global_account.fee_basis_points)
        .map_err(|e| anyhow!(e))?;
    let min_sol_output_with_slippage = calculate_with_slippage_sell(
        min_sol_output,
        slippage_basis_points.unwrap_or(DEFAULT_SLIPPAGE),
    );

    let mut instructions = vec![];

    instructions.push(sell(
        payer.as_ref(),
        &mint,
        &global_account.fee_recipient,
        Sell {
            _amount: amount_token,
            _min_sol_output: min_sol_output_with_slippage,
        },
    ));

    instructions.push(close_account(
        &spl_token::ID,
        &ata,
        &payer.pubkey(),
        &payer.pubkey(),
        &[&payer.pubkey()],
    )?);

    Ok(instructions)
}

