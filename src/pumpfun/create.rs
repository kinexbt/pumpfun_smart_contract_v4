use std::{str::FromStr, time::Instant, sync::Arc};

use anyhow::anyhow;
use solana_client::rpc_config::RpcSimulateTransactionConfig;
use solana_sdk::{
    commitment_config::CommitmentConfig, compute_budget::ComputeBudgetInstruction, instruction::Instruction, message::{v0, VersionedMessage}, native_token::sol_to_lamports, pubkey::Pubkey, signature::{Keypair, Signature}, signer::Signer, system_instruction, transaction::{Transaction, VersionedTransaction}
};
use spl_associated_token_account::{
    instruction::create_associated_token_account,
};

use crate::{
    common::{PriorityFee, SolanaRpcClient}, constants, instruction, 
    ipfs::TokenMetadataIPFS,  jito::FeeClient,
    pumpfun::buy::build_buy_transaction_with_tip
};

use crate::pumpfun::common::{
    create_priority_fee_instructions, 
    get_buy_amount_with_slippage, get_global_account
};

/// Create a new token
pub async fn create(
    rpc: Arc<SolanaRpcClient>,
    payer: Arc<Keypair>,
    mint: Keypair,
    ipfs: TokenMetadataIPFS,
    priority_fee: PriorityFee,
) -> Result<(), anyhow::Error> {
    let mut instructions = create_priority_fee_instructions(priority_fee);

    instructions.push(instruction::create(
        payer.as_ref(),
        &mint,
        instruction::Create {
            _name: ipfs.metadata.name,
            _symbol: ipfs.metadata.symbol,
            _uri: ipfs.metadata_uri,
            _creator: payer.pubkey(),
        },
    ));

    let recent_blockhash = rpc.get_latest_blockhash().await?;
    let transaction = Transaction::new_signed_with_payer(
        &instructions,
        Some(&payer.pubkey()),
        &[payer.as_ref(), &mint],
        recent_blockhash,
    );

    rpc.send_and_confirm_transaction(&transaction).await?;

    Ok(())
}

/// Create and buy tokens in one transaction
pub async fn create_and_buy(
    rpc: Arc<SolanaRpcClient>,
    payer: Arc<Keypair>,
    mint: Keypair,
    ipfs: TokenMetadataIPFS,
    amount_sol: u64,
    slippage_basis_points: Option<u64>,
    priority_fee: PriorityFee,
) -> Result<(), anyhow::Error> {
    if amount_sol == 0 {
        return Err(anyhow!("Amount cannot be zero"));
    }

    let mint = Arc::new(mint);
    let transaction = build_create_and_buy_transaction(rpc.clone(), payer.clone(), mint.clone(), ipfs, amount_sol, slippage_basis_points, priority_fee.clone()).await?;
    rpc.send_and_confirm_transaction(&transaction).await?;

    Ok(())
}

pub async fn create_and_buy_with_tip(
    rpc: Arc<SolanaRpcClient>,
    fee_clients: Vec<Arc<FeeClient>>,
    payer: Arc<Keypair>,
    mint: Keypair,
    ipfs: TokenMetadataIPFS,
    amount_sol: u64,
    slippage_basis_points: Option<u64>,
    priority_fee: PriorityFee,
) -> Result<(), anyhow::Error> {
    let start_time = Instant::now();
    let mint = Arc::new(mint);
    let build_instructions = build_create_and_buy_instructions(rpc.clone(), payer.clone(), mint.clone(), ipfs.clone(), amount_sol, slippage_basis_points, priority_fee.clone()).await?;
    let mut handles = vec![];
    for fee_client in fee_clients {
        let rpc = rpc.clone();
        let payer = payer.clone(); 
        let mint = mint.clone();
        let priority_fee = priority_fee.clone();
        let tip_account = fee_client.get_tip_account().await.map_err(|e| anyhow!(e.to_string()))?;
        let tip_account = Arc::new(Pubkey::from_str(&tip_account).map_err(|e| anyhow!(e))?);
        let build_instructions = build_instructions.clone();

        let handle = tokio::spawn(async move {    
            let transaction = build_create_and_buy_transaction_with_tip(rpc, tip_account, payer, mint, priority_fee, build_instructions).await?;
            fee_client.send_transaction(&transaction).await.map_err(|e| anyhow!(e.to_string()))?;
            println!("Total Jito create and buy operation time: {:?}ms", start_time.elapsed().as_millis());
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

pub async fn build_create_and_buy_transaction(
    rpc: Arc<SolanaRpcClient>,
    payer: Arc<Keypair>,
    mint: Arc<Keypair>,
    ipfs: TokenMetadataIPFS,
    amount_sol: u64,
    slippage_basis_points: Option<u64>,
    priority_fee: PriorityFee,
) -> Result<Transaction, anyhow::Error> {
    let mut instructions: Vec<Instruction> = vec![
        ComputeBudgetInstruction::set_compute_unit_price(priority_fee.unit_price),
        ComputeBudgetInstruction::set_compute_unit_limit(priority_fee.unit_limit),
    ];

    let build_instructions: Vec<Instruction> = build_create_and_buy_instructions(rpc.clone(), payer.clone(), mint.clone(), ipfs, amount_sol, slippage_basis_points, priority_fee.clone()).await?;
    instructions.extend(build_instructions);

    let recent_blockhash = rpc.get_latest_blockhash().await?;
    let transaction = Transaction::new_signed_with_payer(
        &instructions,
        Some(&payer.pubkey()),
        &[payer.as_ref(), mint.as_ref()],
        recent_blockhash,
    );

    Ok(transaction)
}

pub async fn build_create_and_buy_transaction_with_tip(
    rpc: Arc<SolanaRpcClient>,
    tip_account: Arc<Pubkey>,
    payer: Arc<Keypair>,
    mint: Arc<Keypair>,
    priority_fee: PriorityFee,  
    build_instructions: Vec<Instruction>,
) -> Result<VersionedTransaction, anyhow::Error> {
    let mut instructions: Vec<Instruction> = vec![
        ComputeBudgetInstruction::set_compute_unit_price(priority_fee.unit_price),
        ComputeBudgetInstruction::set_compute_unit_limit(priority_fee.unit_limit),
        system_instruction::transfer(
            &payer.pubkey(),
            &tip_account,
            sol_to_lamports(priority_fee.buy_tip_fee),
        ),
    ];
    instructions.extend(build_instructions);

    let recent_blockhash = rpc.get_latest_blockhash().await?;
    let v0_message: v0::Message =
        v0::Message::try_compile(&payer.pubkey(), &instructions, &[], recent_blockhash)?;
    
    let versioned_message: VersionedMessage = VersionedMessage::V0(v0_message);
    let transaction = VersionedTransaction::try_new(versioned_message, &[&payer])?;

    Ok(transaction)
}

pub async fn build_create_and_buy_instructions(
    rpc: Arc<SolanaRpcClient>,
    payer: Arc<Keypair>,
    mint: Arc<Keypair>,
    ipfs: TokenMetadataIPFS,
    amount_sol: u64,
    slippage_basis_points: Option<u64>,
    priority_fee: PriorityFee, 
) -> Result<Vec<Instruction>, anyhow::Error> {
    if amount_sol == 0 {
        return Err(anyhow!("Amount cannot be zero"));
    }

    let rpc = rpc.as_ref();
    let global_account = get_global_account(rpc).await?;
    let buy_amount = global_account.get_initial_buy_price(amount_sol);
    let buy_amount_with_slippage =
        get_buy_amount_with_slippage(amount_sol, slippage_basis_points);

    let mut instructions = vec![];

    instructions.push(instruction::create(
        payer.as_ref(),
        mint.as_ref(),
        instruction::Create {
            _name: ipfs.metadata.name.clone(),
            _symbol: ipfs.metadata.symbol.clone(),
            _uri: ipfs.metadata_uri.clone(),
            _creator: payer.pubkey(),
        },
    ));

    instructions.push(create_associated_token_account(
        &payer.pubkey(),
        &payer.pubkey(),
        &mint.pubkey(),
        &constants::accounts::TOKEN_PROGRAM,
    ));

    instructions.push(instruction::buy(
        payer.as_ref(),
        &mint.pubkey(),
        &global_account.fee_recipient,
        instruction::Buy {
            _amount: buy_amount,
            _max_sol_cost: buy_amount_with_slippage,
        },
    ));

    Ok(instructions)
}
