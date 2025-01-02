use std::sync::Arc;
use anchor_client::solana_client::nonblocking::rpc_client::RpcClient;
use anchor_spl::associated_token::get_associated_token_address;
use anchor_client::{
    solana_sdk::{
        native_token::LAMPORTS_PER_SOL,
        instruction::Instruction,
        pubkey::Pubkey,
        signature::Keypair,
        signer::Signer,
        transaction::Transaction,
        commitment_config::CommitmentConfig,
        compute_budget::ComputeBudgetInstruction,
        system_program::ID as SYSTEM_PROGRAM_ID,
        sysvar::rent::ID as RENT_ID,
        instruction::AccountMeta,
        program_error::ProgramError,
        program_pack::Pack,
    },
    Cluster,
};
use mai3_pumpfun_sdk::constants::accounts::PUMPFUN;
use mai3_pumpfun_sdk::instruction::logs_subscribe;
use std::str::FromStr;
use tokio::signal;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    start_token_subscription().await?;

    Ok(())
}

pub async fn start_token_subscription() -> Result<(), Box<dyn std::error::Error>> {
    println!("Starting token subscription\n");

    let ws_url = "wss://api.mainnet-beta.solana.com";
    let commitment = CommitmentConfig::confirmed();

    println!("program_address: {}", PUMPFUN);

    let subscription_callback = |signature: &str, logs: Vec<String>| {
        println!("=======Signature: {}=============", signature);
        for log in logs {
            println!("============Log: {}============", log);
        }
    };

    let subscription = logs_subscribe::start_subscription(
        ws_url,
        &PUMPFUN.to_string(),
        commitment,
        subscription_callback,
    ).await.unwrap();

    subscription.task.await.unwrap();
    // stop_subscription(subscription);

    tokio::time::sleep(tokio::time::Duration::from_secs(1000)).await;

    println!("Subscription closed.");

    Ok(())  
}