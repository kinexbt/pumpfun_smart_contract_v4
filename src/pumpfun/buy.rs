use anyhow::anyhow;
use solana_sdk::{
    compute_budget::ComputeBudgetInstruction, instruction::Instruction, message::{v0, VersionedMessage}, native_token::sol_to_lamports, pubkey::Pubkey, signature::Keypair, signer::Signer, system_instruction, transaction::{Transaction, VersionedTransaction}
};
use solana_hash::Hash;
use spl_associated_token_account::instruction::create_associated_token_account;
use tokio::task::JoinHandle;
use std::{str::FromStr, time::Instant, sync::Arc};

use crate::{common::{PriorityFee, SolanaRpcClient}, constants::{self, global_constants::FEE_RECIPIENT, trade::DEFAULT_SLIPPAGE}, instruction, swqos::FeeClient};

const MAX_LOADED_ACCOUNTS_DATA_SIZE_LIMIT: u32 = 250000;

use super::common::{calculate_with_slippage_buy, get_bonding_curve_account, get_buy_token_amount, get_creator_vault_pda, get_global_account, get_initial_buy_price};

pub async fn buy(
    rpc: Arc<SolanaRpcClient>,
    payer: Arc<Keypair>,
    mint: Pubkey,
    amount_sol: u64,
    slippage_basis_points: Option<u64>,
    priority_fee: PriorityFee,
) -> Result<(), anyhow::Error> {
    let transaction = build_buy_transaction(rpc.clone(), payer.clone(), mint.clone(), amount_sol, slippage_basis_points, priority_fee.clone()).await?;
    rpc.send_and_confirm_transaction(&transaction).await?;
    Ok(())
}

/// Buy tokens using Jito
pub async fn buy_with_tip(
    rpc: Arc<SolanaRpcClient>,
    fee_clients: Vec<Arc<FeeClient>>,
    payer: Arc<Keypair>,
    mint: Pubkey,
    amount_sol: u64,
    slippage_basis_points: Option<u64>,
    priority_fee: PriorityFee,
) -> Result<(), anyhow::Error> {
    let start_time = Instant::now();

    let mint = Arc::new(mint.clone());
    let instructions = build_buy_instructions(rpc.clone(), payer.clone(), mint.clone(), amount_sol, slippage_basis_points).await?;

    let mut transactions = vec![];
    let recent_blockhash = rpc.get_latest_blockhash().await?;
    for fee_client in fee_clients.clone() {
        let payer = payer.clone();
        let priority_fee = priority_fee.clone();
        let tip_account = fee_client.get_tip_account().await.map_err(|e| anyhow!(e.to_string()))?;
        let tip_account = Arc::new(Pubkey::from_str(&tip_account).map_err(|e| anyhow!(e))?);

        let transaction = build_buy_transaction_with_tip(tip_account, payer, priority_fee, instructions.clone(), recent_blockhash).await?;
        transactions.push(transaction);
    }

    let mut handles: Vec<JoinHandle<Result<(), anyhow::Error>>> = vec![];
    for i in 0..fee_clients.len() {
        let fee_client = fee_clients[i].clone();
        let transactions = transactions.clone();
        let start_time = start_time.clone();
        let transaction = transactions[i].clone();
        let handle = tokio::spawn(async move {
           fee_client.send_transaction(&transaction).await?;
            println!("index: {}, Total Jito buy operation time: {:?}ms", i, start_time.elapsed().as_millis());
            Ok::<(), anyhow::Error>(())
        });

        handles.push(handle);        
    }

    for handle in handles {
        match handle.await {
            Ok(Ok(_)) => (),
            Ok(Err(e)) => println!("Error in task: {}", e),
            Err(e) => println!("Task join error: {}", e),
        }
    }

    Ok(())
}

pub async fn build_buy_transaction(
    rpc: Arc<SolanaRpcClient>,
    payer: Arc<Keypair>,
    mint: Pubkey,
    amount_sol: u64,
    slippage_basis_points: Option<u64>,
    priority_fee: PriorityFee,
) -> Result<Transaction, anyhow::Error> {
    let mut instructions = vec![
        ComputeBudgetInstruction::set_loaded_accounts_data_size_limit(MAX_LOADED_ACCOUNTS_DATA_SIZE_LIMIT),
        ComputeBudgetInstruction::set_compute_unit_price(priority_fee.unit_price),
        ComputeBudgetInstruction::set_compute_unit_limit(priority_fee.unit_limit),
    ];

    let build_instructions = build_buy_instructions(rpc.clone(), payer.clone(), Arc::new(mint), amount_sol, slippage_basis_points).await?;
    instructions.extend(build_instructions);

    let recent_blockhash = rpc.get_latest_blockhash().await?;
    let transaction = Transaction::new_signed_with_payer(
        &instructions,
        Some(&payer.pubkey()),
        &[payer],
        recent_blockhash,
    );

    Ok(transaction)
}

pub async fn build_buy_transaction_with_tip(
    tip_account: Arc<Pubkey>,
    payer: Arc<Keypair>,
    priority_fee: PriorityFee,  
    build_instructions: Vec<Instruction>,
    blockhash: Hash,
) -> Result<VersionedTransaction, anyhow::Error> {
    let mut instructions = vec![
        ComputeBudgetInstruction::set_loaded_accounts_data_size_limit(MAX_LOADED_ACCOUNTS_DATA_SIZE_LIMIT),
        ComputeBudgetInstruction::set_compute_unit_price(priority_fee.unit_price),
        ComputeBudgetInstruction::set_compute_unit_limit(priority_fee.unit_limit),
        system_instruction::transfer(
            &payer.pubkey(),
            &tip_account,
            sol_to_lamports(priority_fee.buy_tip_fee),
        ),
    ];

    instructions.extend(build_instructions);

    let v0_message: v0::Message =
        v0::Message::try_compile(&payer.pubkey(), &instructions, &[], blockhash)?;
    let versioned_message: VersionedMessage = VersionedMessage::V0(v0_message);
    let transaction = VersionedTransaction::try_new(versioned_message, &[&payer])?;

    Ok(transaction)
}

pub async fn build_buy_instructions(
    rpc: Arc<SolanaRpcClient>,
    payer: Arc<Keypair>,
    mint: Arc<Pubkey>,
    buy_sol_cost: u64,
    slippage_basis_points: Option<u64>,
) -> Result<Vec<Instruction>, anyhow::Error> {
    if buy_sol_cost == 0 {
        return Err(anyhow!("Amount cannot be zero"));
    }

    let (bonding_curve_account, bonding_curve_pda) = get_bonding_curve_account(&rpc, &mint).await?;

    let creator_vault_pda = get_creator_vault_pda(&bonding_curve_account.creator).unwrap();

    let (buy_token_amount, max_sol_cost) = get_buy_token_amount(&bonding_curve_account, buy_sol_cost, slippage_basis_points)?;

    let mut instructions = vec![];
    instructions.push(create_associated_token_account(
        &payer.pubkey(),
        &payer.pubkey(),
        &mint,
        &constants::accounts::TOKEN_PROGRAM,
    ));

    instructions.push(instruction::buy(
        payer.as_ref(),
        &mint,
        &bonding_curve_pda,
        &creator_vault_pda,
        &FEE_RECIPIENT,
        instruction::Buy {
            _amount: buy_token_amount,
            _max_sol_cost: max_sol_cost,
        },
    ));

    Ok(instructions)
}