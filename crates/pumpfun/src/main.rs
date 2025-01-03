use mai3_pumpfun_sdk::instruction::{
    logs_events::DexEvent,
    logs_subscribe::{tokens_subscription, stop_subscription}
};
use anchor_client::solana_sdk::commitment_config::CommitmentConfig;

use std::str::FromStr;
use tokio::signal;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Starting token subscription\n");

    let ws_url = "wss://api.mainnet-beta.solana.com";
            
    // Set commitment
    let commitment = CommitmentConfig::confirmed();
    
    // Define callback function
    let callback = |event: DexEvent| {
        match event {
            DexEvent::NewToken(token_info) => {
                println!("Received new token event: {:?}", token_info);
            },
            DexEvent::NewTrade(trade_info) => {
                println!("Received new trade event: {:?}", trade_info);
            },
            DexEvent::Error(err) => {
                println!("Received error: {}", err);
            }
        }
    };

    // Start subscription
    let subscription = tokens_subscription(
        ws_url,
        commitment,
        callback
    ).await.unwrap();

    // Wait for a while to receive events
    tokio::time::sleep(tokio::time::Duration::from_secs(60)).await;

    // Stop subscription
    stop_subscription(subscription).await;

    Ok(())  
}