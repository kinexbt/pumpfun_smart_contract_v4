use std::time::Instant;

use anyhow::anyhow;
use solana_client::{rpc_client::RpcClient, rpc_config::RpcSimulateTransactionConfig};
use solana_sdk::{
    commitment_config::CommitmentConfig, compute_budget::ComputeBudgetInstruction, instruction::Instruction, native_token::sol_to_lamports, signature::{Keypair, Signature}, signer::Signer, system_instruction, transaction::Transaction
};
use spl_associated_token_account::{
    get_associated_token_address,
    instruction::create_associated_token_account,
};

use crate::{constants::{self, trade::JITO_TIP_AMOUNT}, instruction, ipfs::TokenMetadataIPFS, jito::JitoClient, trade::buy::build_buy_transaction_with_jito};

use super::common::{create_priority_fee_instructions, get_buy_amount_with_slippage, get_global_account, PriorityFee};

/// Create a new token
pub async fn create(
    rpc: &RpcClient,
    payer: &Keypair,
    mint: &Keypair,
    ipfs: TokenMetadataIPFS,
    priority_fee: PriorityFee,
) -> Result<Signature, anyhow::Error> {
    let mut instructions = create_priority_fee_instructions(priority_fee);

    instructions.push(instruction::create(
        payer,
        mint,
        instruction::Create {
            _name: ipfs.metadata.name,
            _symbol: ipfs.metadata.symbol,
            _uri: ipfs.metadata_uri,
        },
    ));

    let recent_blockhash = rpc.get_latest_blockhash()?;
    let transaction = Transaction::new_signed_with_payer(
        &instructions,
        Some(&payer.pubkey()),
        &[payer, mint],
        recent_blockhash,
    );

    let signature = rpc.send_and_confirm_transaction(&transaction)?;

    Ok(signature)
}

/// Create and buy tokens in one transaction
pub async fn create_and_buy(
    rpc: &RpcClient,
    payer: &Keypair,
    mint: &Keypair,
    ipfs: TokenMetadataIPFS,
    amount_sol: u64,
    slippage_basis_points: Option<u64>,
    priority_fee: PriorityFee,
) -> Result<Signature, anyhow::Error> {
    if amount_sol == 0 {
        return Err(anyhow!("Amount cannot be zero"));
    }

    let transaction = build_create_and_buy_transaction(rpc, payer, mint, ipfs, amount_sol, slippage_basis_points, priority_fee).await?;
    let signature = rpc.send_and_confirm_transaction(&transaction)?;

    Ok(signature)
}

pub async fn create_and_buy_list_with_jito(
    rpc: &RpcClient,
    jito_client: &JitoClient,
    payers: Vec<&Keypair>,
    mint: &Keypair,
    ipfs: TokenMetadataIPFS,
    amount_sols: Vec<u64>,
    slippage_basis_points: Option<u64>,
    priority_fee: PriorityFee,
) -> Result<String, anyhow::Error> {
    
    let start_time = Instant::now();

    let mut transactions = Vec::new();
    let transaction = build_create_and_buy_transaction_with_jito(rpc, jito_client, payers[0], mint, ipfs, amount_sols[0], slippage_basis_points, priority_fee).await?;
    transactions.push(transaction);
    
    for (i, payer) in payers.iter().skip(1).enumerate() {
        println!("Creating and buying token index: {}", i);
        let buy_transaction = build_buy_transaction_with_jito(rpc, jito_client, payer, &mint.pubkey(), amount_sols[i], slippage_basis_points, priority_fee).await?;
        transactions.push(buy_transaction);
    }

    let signatures = jito_client.send_transactions(&transactions).await?;

    println!("Total Jito create and buy operation time: {:?}ms", start_time.elapsed().as_millis());
    
    Ok(signatures)
}

pub async fn create_and_buy_with_jito(
    rpc: &RpcClient,
    jito_client: &JitoClient,
    payer: &Keypair,
    mint: &Keypair,
    ipfs: TokenMetadataIPFS,
    amount_sol: u64,
    slippage_basis_points: Option<u64>,
    priority_fee: PriorityFee,
) -> Result<String, anyhow::Error> {

    let start_time = Instant::now();

    let transaction = build_create_and_buy_transaction_with_jito(rpc, jito_client, payer, mint, ipfs, amount_sol, slippage_basis_points, priority_fee).await?;

    let signature = jito_client.send_transaction(&transaction).await?;

    println!("Total Jito create and buy operation time: {:?}ms, signature: {}", start_time.elapsed().as_millis(), signature);

    Ok(signature)
}

pub async fn build_create_and_buy_transaction(
    rpc: &RpcClient,
    payer: &Keypair,
    mint: &Keypair,
    ipfs: TokenMetadataIPFS,
    amount_sol: u64,
    slippage_basis_points: Option<u64>,
    priority_fee: PriorityFee,
) -> Result<Transaction, anyhow::Error> {
    let instructions = build_create_and_buy_instructions(rpc, payer, mint, ipfs, amount_sol, slippage_basis_points, priority_fee).await?;
    let recent_blockhash = rpc.get_latest_blockhash()?;
    let transaction = Transaction::new_signed_with_payer(
        &instructions,
        Some(&payer.pubkey()),
        &[payer, mint],
        recent_blockhash,
    );

    Ok(transaction)
}

pub async fn build_create_and_buy_transaction_with_jito(
    rpc: &RpcClient,
    jito_client: &JitoClient,
    payer: &Keypair,
    mint: &Keypair,
    ipfs: TokenMetadataIPFS,
    amount_sol: u64,
    slippage_basis_points: Option<u64>,
    priority_fee: PriorityFee,  
) -> Result<Transaction, anyhow::Error> {
    let instructions = build_create_and_buy_instructions_with_jito(rpc, jito_client, payer, mint, ipfs, amount_sol, slippage_basis_points, priority_fee).await?;
    let recent_blockhash = rpc.get_latest_blockhash()?;
    let transaction = Transaction::new_signed_with_payer(
        &instructions,
        Some(&payer.pubkey()),
        &[payer, mint],
        recent_blockhash,
    );

    Ok(transaction)
}

pub async fn build_create_and_buy_instructions(
    rpc: &RpcClient,
    payer: &Keypair,
    mint: &Keypair,
    ipfs: TokenMetadataIPFS,
    amount_sol: u64,
    slippage_basis_points: Option<u64>,
    priority_fee: PriorityFee,
) -> Result<Vec<Instruction>, anyhow::Error> {
    if amount_sol == 0 {
        return Err(anyhow!("Amount cannot be zero"));
    }

    let global_account = get_global_account(rpc).await?;
    let buy_amount = global_account.get_initial_buy_price(amount_sol);
    let buy_amount_with_slippage =
        get_buy_amount_with_slippage(amount_sol, slippage_basis_points);

    let mut instructions = vec![
        ComputeBudgetInstruction::set_compute_unit_limit(1_400_000),
        ComputeBudgetInstruction::set_compute_unit_price(0),
    ];

    instructions.push(instruction::create(
        payer,
        mint,
        instruction::Create {
            _name: ipfs.metadata.name,
            _symbol: ipfs.metadata.symbol,
            _uri: ipfs.metadata_uri,
        },
    ));

    let ata = get_associated_token_address(&payer.pubkey(), &mint.pubkey());
    if rpc.get_account(&ata).is_err() {
        instructions.push(create_associated_token_account(
            &payer.pubkey(),
            &payer.pubkey(),
            &mint.pubkey(),
            &constants::accounts::TOKEN_PROGRAM,
        ));
    }

    instructions.push(instruction::buy(
        payer,
        &mint.pubkey(),
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
        &[payer, mint],
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
        priority_fee.unit_price
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

pub async fn build_create_and_buy_instructions_with_jito(
    rpc: &RpcClient,
    jito_client: &JitoClient,
    payer: &Keypair,
    mint: &Keypair,
    ipfs: TokenMetadataIPFS,
    amount_sol: u64,
    slippage_basis_points: Option<u64>,
    priority_fee: PriorityFee,
) -> Result<Vec<Instruction>, anyhow::Error> {
    if amount_sol == 0 {
        return Err(anyhow!("Amount cannot be zero"));
    }

    let global_account = get_global_account(rpc).await?;
    let buy_amount = global_account.get_initial_buy_price(amount_sol);
    let buy_amount_with_slippage =
        get_buy_amount_with_slippage(amount_sol, slippage_basis_points);

    let mut instructions = vec![
        ComputeBudgetInstruction::set_compute_unit_price(priority_fee.unit_price),
        ComputeBudgetInstruction::set_compute_unit_limit(priority_fee.unit_limit),
    ];

    instructions.push(instruction::create(
        payer,
        mint,
        instruction::Create {
            _name: ipfs.metadata.name,
            _symbol: ipfs.metadata.symbol,
            _uri: ipfs.metadata_uri,
        },
    ));

    let ata = get_associated_token_address(&payer.pubkey(), &mint.pubkey());
    if rpc.get_account(&ata).is_err() {
        instructions.push(create_associated_token_account(
            &payer.pubkey(),
            &payer.pubkey(),
            &mint.pubkey(),
            &constants::accounts::TOKEN_PROGRAM,
        ));
    }

    instructions.push(instruction::buy(
        payer,
        &mint.pubkey(),
        &global_account.fee_recipient,
        instruction::Buy {
            _amount: buy_amount,
            _max_sol_cost: buy_amount_with_slippage,
        },
    ));

    let tip_account = jito_client.get_tip_account().await.map_err(|e| anyhow!(e))?;
    let jito_fee = priority_fee.buy_jito_fee;
    instructions.push(
        system_instruction::transfer(
            &payer.pubkey(),
            &tip_account,
            sol_to_lamports(jito_fee * 2.0),
        ),
    );

    Ok(instructions)
}
