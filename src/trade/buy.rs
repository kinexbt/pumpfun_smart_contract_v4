use anyhow::anyhow;
use solana_client::{rpc_client::RpcClient, rpc_config::RpcSimulateTransactionConfig};
use solana_sdk::{
    commitment_config::CommitmentConfig, compute_budget::ComputeBudgetInstruction, instruction::Instruction, native_token::sol_to_lamports, pubkey::Pubkey, signature::{Keypair, Signature}, signer::Signer, system_instruction, transaction::Transaction
};
use spl_associated_token_account::{
    get_associated_token_address,
    instruction::create_associated_token_account,
};
use std::time::Instant;

use crate::{constants::{self, trade::{DEFAULT_COMPUTE_UNIT_PRICE, DEFAULT_SLIPPAGE, JITO_TIP_AMOUNT}}, instruction, jito::JitoClient};

use super::common::{calculate_with_slippage_buy, get_bonding_curve_account, get_global_account, get_initial_buy_price, PriorityFee};

pub async fn buy(
    rpc: &RpcClient,
    payer: &Keypair,
    mint: &Pubkey,
    amount_sol: u64,
    slippage_basis_points: Option<u64>,
    priority_fee: PriorityFee,
) -> Result<Signature, anyhow::Error> {
    let transaction = build_buy_transaction(rpc, payer, mint, amount_sol, slippage_basis_points, priority_fee).await?;
    let signature = rpc.send_transaction(&transaction)?;
    Ok(signature)
}

/// Buy tokens using Jito
pub async fn buy_with_jito(
    rpc: &RpcClient,
    jito_client: &JitoClient,
    payer: &Keypair,
    mint: &Pubkey,
    amount_sol: u64,
    slippage_basis_points: Option<u64>,
    priority_fee: PriorityFee,
) -> Result<String, anyhow::Error> {
    let start_time = Instant::now();

    let transaction = build_buy_transaction_with_jito(rpc, jito_client, payer, mint, amount_sol, slippage_basis_points, priority_fee).await?;
    let signature = jito_client.send_transaction(&transaction).await?;

    println!("Total Jito buy operation time: {:?}ms", start_time.elapsed().as_millis());

    Ok(signature)
}

pub async fn buy_list_with_jito(
    rpc: &RpcClient,
    jito_client: &JitoClient,
    payers: Vec<&Keypair>,
    mint: &Pubkey,
    amount_sols: Vec<u64>,
    slippage_basis_points: Option<u64>,
    priority_fee: PriorityFee,
) -> Result<String, anyhow::Error> {
    let start_time = Instant::now();

    let mut transactions = vec![];
    for (i, payer) in payers.iter().enumerate() {
        let transaction = build_buy_transaction_with_jito(rpc, jito_client, payer, mint, amount_sols[i], slippage_basis_points, priority_fee).await?;
        transactions.push(transaction);
    }
    
    let signature = jito_client.send_transactions(&transactions).await?;

    println!("Total Jito buy operation time: {:?}ms", start_time.elapsed().as_millis());

    Ok(signature)
}

pub async fn build_buy_transaction(
    rpc: &RpcClient,
    payer: &Keypair,
    mint: &Pubkey,
    amount_sol: u64,
    slippage_basis_points: Option<u64>,
    priority_fee: PriorityFee,
) -> Result<Transaction, anyhow::Error> {
    let instructions = build_buy_instructions(rpc, payer, mint, amount_sol, slippage_basis_points, priority_fee).await?;

    let recent_blockhash = rpc.get_latest_blockhash()?;
    let transaction = Transaction::new_signed_with_payer(
        &instructions,
        Some(&payer.pubkey()),
        &[payer],
        recent_blockhash,
    );

    Ok(transaction)
}

pub async fn build_buy_transaction_with_jito(
    rpc: &RpcClient,
    jito_client: &JitoClient,
    payer: &Keypair,
    mint: &Pubkey,
    amount_sol: u64,
    slippage_basis_points: Option<u64>,
    priority_fee: PriorityFee,  
) -> Result<Transaction, anyhow::Error> {
    let instructions = build_buy_instructions_with_jito(rpc, jito_client, payer, mint, amount_sol, slippage_basis_points, priority_fee).await?;
    let recent_blockhash = rpc.get_latest_blockhash()?;
    let transaction = Transaction::new_signed_with_payer(
        &instructions,
        Some(&payer.pubkey()),
        &[payer],
        recent_blockhash,
    );

    Ok(transaction)
}

pub async fn build_buy_instructions(
    rpc: &RpcClient,
    payer: &Keypair,
    mint: &Pubkey,
    amount_sol: u64,
    slippage_basis_points: Option<u64>,
    priority_fee: PriorityFee,
) -> Result<Vec<Instruction>, anyhow::Error> {
    if amount_sol == 0 {
        return Err(anyhow!("Amount cannot be zero"));
    }

    let global_account = get_global_account(rpc).await?;
    let bonding_curve_account = get_bonding_curve_account(rpc, mint).await?;
    let buy_amount = bonding_curve_account
        .get_buy_price(amount_sol)
        .map_err(|e| anyhow!(e))?;
    let buy_amount_with_slippage = calculate_with_slippage_buy(amount_sol, slippage_basis_points.unwrap_or(DEFAULT_SLIPPAGE));

    let mut instructions = vec![
        ComputeBudgetInstruction::set_compute_unit_limit(1_400_000),
        ComputeBudgetInstruction::set_compute_unit_price(0),
    ];

    let ata = get_associated_token_address(&payer.pubkey(), mint);
    if rpc.get_account(&ata).is_err() {
        instructions.push(create_associated_token_account(
            &payer.pubkey(),
            &payer.pubkey(),
            mint,
            &constants::accounts::TOKEN_PROGRAM,
        ));
    }

    instructions.push(instruction::buy(
        payer,
        mint,
        &global_account.fee_recipient,
        instruction::Buy {
            _amount: buy_amount,
            _max_sol_cost: buy_amount_with_slippage,
        },
    ));

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
        sol_to_lamports(DEFAULT_COMPUTE_UNIT_PRICE)
    } else {
        fees.iter()
            .map(|fee| fee.prioritization_fee)
            .sum::<u64>() / fees.len() as u64
    };

    let unit_price = if average_fees == 0 { sol_to_lamports(priority_fee.price) } else { average_fees };
    instructions[0] = ComputeBudgetInstruction::set_compute_unit_limit(result_cu as u32);
    instructions[1] = ComputeBudgetInstruction::set_compute_unit_price(unit_price);

    Ok(instructions)

}

pub async fn build_buy_instructions_with_jito(
    rpc: &RpcClient,
    jito_client: &JitoClient,
    payer: &Keypair,
    mint: &Pubkey,
    amount_sol: u64,
    slippage_basis_points: Option<u64>,
    priority_fee: PriorityFee,
) -> Result<Vec<Instruction>, anyhow::Error> {
    if amount_sol == 0 {
        return Err(anyhow!("Amount cannot be zero"));
    }

    let global_account = get_global_account(rpc).await?;
    let buy_amount = match get_bonding_curve_account(rpc, mint).await {
        Ok(account) => account.get_buy_price(amount_sol).map_err(|e| anyhow!(e))?,
        Err(_e) => {
            let initial_buy_amount = get_initial_buy_price(&global_account, amount_sol).await?;
            initial_buy_amount * 80 / 100
        }
    };
    
    let buy_amount_with_slippage = calculate_with_slippage_buy(amount_sol, slippage_basis_points.unwrap_or(DEFAULT_SLIPPAGE));

    let mut instructions = vec![
        ComputeBudgetInstruction::set_compute_unit_price(sol_to_lamports(priority_fee.price)),
        ComputeBudgetInstruction::set_compute_unit_limit(sol_to_lamports(priority_fee.limit) as u32),
    ];

    let ata = get_associated_token_address(&payer.pubkey(), mint);
    if rpc.get_account(&ata).is_err() {
        instructions.push(create_associated_token_account(
            &payer.pubkey(),
            &payer.pubkey(),
            mint,
            &constants::accounts::TOKEN_PROGRAM,
        ));
    }

    instructions.push(instruction::buy(
        payer,
        mint,
        &global_account.fee_recipient,
        instruction::Buy {
            _amount: buy_amount,
            _max_sol_cost: buy_amount_with_slippage,
        },
    ));

    let tip_account = jito_client.get_tip_account().await.map_err(|e| anyhow!(e))?;

    let jito_fee = priority_fee.jito_fee.unwrap_or(JITO_TIP_AMOUNT);
    instructions.push(
        system_instruction::transfer(
            &payer.pubkey(),
            &tip_account,
            sol_to_lamports(jito_fee),
        ),
    );

    Ok(instructions)
}
