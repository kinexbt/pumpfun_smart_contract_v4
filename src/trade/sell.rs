use anyhow::anyhow;
use solana_client::{rpc_client::RpcClient, rpc_config::RpcSimulateTransactionConfig};
use solana_sdk::{
    commitment_config::CommitmentConfig, compute_budget::ComputeBudgetInstruction, instruction::Instruction, native_token::sol_to_lamports, pubkey::Pubkey, signature::{Keypair, Signature}, signer::Signer, system_instruction, transaction::Transaction
};
use spl_associated_token_account::get_associated_token_address;
use spl_token::instruction::close_account;

use std::time::Instant;

use crate::{constants::trade::{DEFAULT_COMPUTE_UNIT_PRICE, DEFAULT_SLIPPAGE}, instruction, jito::JitoClient};

use super::common::{calculate_with_slippage_sell, get_bonding_curve_account, get_global_account, PriorityFee};

async fn get_token_balance(rpc: &RpcClient, payer: &Keypair, mint: &Pubkey) -> Result<(u64, Pubkey), anyhow::Error> {
    let ata = get_associated_token_address(&payer.pubkey(), mint);
    let balance = rpc.get_token_account_balance(&ata)?;
    let balance_u64 = balance.amount.parse::<u64>()
        .map_err(|_| anyhow!("Failed to parse token balance"))?;
    
    if balance_u64 == 0 {
        return Err(anyhow!("Balance is 0"));
    }

    Ok((balance_u64, ata))
}

pub async fn sell(
    rpc: &RpcClient,
    payer: &Keypair,
    mint: &Pubkey,
    amount_token: Option<u64>,
    slippage_basis_points: Option<u64>,
    priority_fee: PriorityFee,
) -> Result<Signature, anyhow::Error> {

    let transaction = build_sell_transaction(rpc, payer, mint, amount_token, slippage_basis_points, priority_fee).await?;
    let signature = rpc.send_and_confirm_transaction(&transaction)?;

    Ok(signature)
}

/// Sell tokens by percentage
pub async fn sell_by_percent(
    rpc: &RpcClient,
    payer: &Keypair,
    mint: &Pubkey,
    percent: u64,
    slippage_basis_points: Option<u64>,
    priority_fee: PriorityFee,
) -> Result<Signature, anyhow::Error> {
    if percent == 0 || percent > 100 {
        return Err(anyhow!("Percentage must be between 1 and 100"));
    }

    let (balance_u64, _) = get_token_balance(rpc, payer, mint).await?;
    let amount = balance_u64 * percent / 100;
    sell(rpc, payer, mint, Some(amount), slippage_basis_points, priority_fee).await
}

pub async fn sell_by_percent_with_jito(
    rpc: &RpcClient,
    payer: &Keypair,
    jito_client: &JitoClient,
    mint: &Pubkey,
    percent: u64,
    slippage_basis_points: Option<u64>,
    priority_fee: PriorityFee,
) -> Result<String, anyhow::Error> {
    if percent == 0 || percent > 100 {
        return Err(anyhow!("Percentage must be between 1 and 100"));
    }

    let (balance_u64, _) = get_token_balance(rpc, payer, mint).await?;
    let amount = balance_u64 * percent / 100;
    sell_with_jito(rpc, payer, jito_client, mint, Some(amount), slippage_basis_points, priority_fee).await
}

/// Sell tokens using Jito
pub async fn sell_with_jito(
    rpc: &RpcClient,
    payer: &Keypair,
    jito_client: &JitoClient,
    mint: &Pubkey,
    amount_token: Option<u64>,
    slippage_basis_points: Option<u64>,
    priority_fee: PriorityFee,
) -> Result<String, anyhow::Error> {
    let start_time = Instant::now();

    let transaction = build_sell_transaction_with_jito(rpc, jito_client, payer, mint, amount_token, slippage_basis_points, priority_fee).await?;
    let signature = jito_client.send_transaction(&transaction).await?;
    
    println!("Total Jito sell operation time: {:?}ms, signature: {}", start_time.elapsed().as_millis(), signature);

    Ok(signature)
}

pub async fn build_sell_transaction(
    rpc: &RpcClient,
    payer: &Keypair,
    mint: &Pubkey,
    amount_token: Option<u64>,
    slippage_basis_points: Option<u64>,
    priority_fee: PriorityFee,
) -> Result<Transaction, anyhow::Error> {
    let instructions = build_sell_instructions(rpc, payer, mint, amount_token, slippage_basis_points, priority_fee).await?;
    let recent_blockhash = rpc.get_latest_blockhash()?;
    let transaction = Transaction::new_signed_with_payer(
        &instructions,
        Some(&payer.pubkey()),
        &[payer],
        recent_blockhash,
    );

    Ok(transaction)
}

pub async fn build_sell_transaction_with_jito(
    rpc: &RpcClient,
    jito_client: &JitoClient,
    payer: &Keypair,
    mint: &Pubkey,
    amount_token: Option<u64>,
    slippage_basis_points: Option<u64>,
    priority_fee: PriorityFee,
) -> Result<Transaction, anyhow::Error> {
    let instructions = build_sell_instructions_with_jito(rpc, jito_client, payer, mint, amount_token, slippage_basis_points, priority_fee).await?;
    let recent_blockhash = rpc.get_latest_blockhash()?;
    let transaction = Transaction::new_signed_with_payer(
        &instructions,
        Some(&payer.pubkey()),
        &[payer],
        recent_blockhash,
    );

    Ok(transaction)
}

pub async fn build_sell_instructions(
    rpc: &RpcClient,
    payer: &Keypair,
    mint: &Pubkey,
    amount_token: Option<u64>,
    slippage_basis_points: Option<u64>,
    priority_fee: PriorityFee,
) -> Result<Vec<Instruction>, anyhow::Error> {
    let (balance_u64, ata) = get_token_balance(rpc, payer, mint).await?;
    let amount = amount_token.unwrap_or(balance_u64);
    
    if amount == 0 {
        return Err(anyhow!("Amount cannot be zero"));
    }
    
    let global_account = get_global_account(rpc).await?;
    let bonding_curve_account = get_bonding_curve_account(rpc, mint).await?;
    let min_sol_output = bonding_curve_account
        .get_sell_price(amount, global_account.fee_basis_points)
        .map_err(|e| anyhow!(e))?;
    let min_sol_output_with_slippage = calculate_with_slippage_sell(
        min_sol_output,
        slippage_basis_points.unwrap_or(DEFAULT_SLIPPAGE),
    );

    let mut instructions = vec![
        ComputeBudgetInstruction::set_compute_unit_limit(1_400_000),
        ComputeBudgetInstruction::set_compute_unit_price(0),
    ];

    instructions.push(instruction::sell(
        payer,
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
        &payer.pubkey(),
        &payer.pubkey(),
        &[&payer.pubkey()],
    )?);

    let commitment_config = CommitmentConfig::confirmed();
    let recent_blockhash = rpc.get_latest_blockhash_with_commitment(commitment_config)?
        .0;

    let simulate_tx = Transaction::new_signed_with_payer(
        &instructions,
        Some(&payer.pubkey()),
        &[payer],
        recent_blockhash,
    );

    let config = RpcSimulateTransactionConfig {
        sig_verify: true,
        commitment: Some(commitment_config),
        ..RpcSimulateTransactionConfig::default()
    };
    
    let result = rpc.simulate_transaction_with_config(&simulate_tx, config)?
        .value;

    if result.logs.as_ref().map_or(true, |logs| logs.is_empty()) {
        return Err(anyhow!("Simulation failed: {:?}", result.err));
    }

    let result_cu = result.units_consumed.ok_or_else(|| anyhow!("No compute units consumed"))?;
    let fees = rpc.get_recent_prioritization_fees(&[])?;
    let average_fees = if fees.is_empty() {
        DEFAULT_COMPUTE_UNIT_PRICE
    } else {
        fees.iter()
            .map(|fee| fee.prioritization_fee)
            .sum::<u64>() / fees.len() as u64
    };

    let unit_price = if average_fees == 0 { priority_fee.unit_price } else { average_fees };
    instructions[0] = ComputeBudgetInstruction::set_compute_unit_limit(result_cu as u32);
    instructions[1] = ComputeBudgetInstruction::set_compute_unit_price(unit_price);

    Ok(instructions)
}

pub async fn build_sell_instructions_with_jito(
    rpc: &RpcClient,
    jito_client: &JitoClient,
    payer: &Keypair,
    mint: &Pubkey,
    amount_token: Option<u64>,
    slippage_basis_points: Option<u64>,
    priority_fee: PriorityFee,
) -> Result<Vec<Instruction>, anyhow::Error> {
    let (balance_u64, ata) = get_token_balance(rpc, payer, mint).await?;
    let amount = amount_token.unwrap_or(balance_u64);
    
    if amount == 0 {
        return Err(anyhow!("Amount cannot be zero"));
    }
    
    let global_account = get_global_account(rpc).await?;
    let bonding_curve_account = get_bonding_curve_account(rpc, mint).await?;
    let min_sol_output = bonding_curve_account
        .get_sell_price(amount, global_account.fee_basis_points)
        .map_err(|e| anyhow!(e))?;
    let min_sol_output_with_slippage = calculate_with_slippage_sell(
        min_sol_output,
        slippage_basis_points.unwrap_or(DEFAULT_SLIPPAGE),
    );

    let mut instructions = vec![
        ComputeBudgetInstruction::set_compute_unit_price(priority_fee.unit_price),
        ComputeBudgetInstruction::set_compute_unit_limit(priority_fee.unit_limit),
    ];

    instructions.push(instruction::sell(
        payer,
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
        &payer.pubkey(),
        &payer.pubkey(),
        &[&payer.pubkey()],
    )?);

    let tip_account = jito_client.get_tip_account().await.map_err(|e| anyhow!(e))?;
    let jito_fee = priority_fee.sell_jito_fee;
    instructions.push(
        system_instruction::transfer(
            &payer.pubkey(),
            &tip_account,
            sol_to_lamports(jito_fee),
        ),
    );

    Ok(instructions)
}
