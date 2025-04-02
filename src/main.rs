use pumpfun_sdk::common::{
    logs_events::PumpfunEvent,
    logs_subscribe::{tokens_subscription, stop_subscription}
};
use solana_sdk::commitment_config::CommitmentConfig;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Starting token subscription\n");

    let ws_url = "wss://api.mainnet-beta.solana.com";
            
    // Set commitment
    let commitment = CommitmentConfig::confirmed();
    
    // Define callback function
    let callback = |event: PumpfunEvent| {
        match event {
            PumpfunEvent::NewDevTrade(trade_info) => {
                println!("Received new dev trade event: {:?}", trade_info);
            },
            PumpfunEvent::NewToken(token_info) => {
                println!("Received new token event: {:?}", token_info);
            },
            PumpfunEvent::NewUserTrade(trade_info) => {
                println!("Received new trade event: {:?}", trade_info);
            },
            PumpfunEvent::NewBotTrade(trade_info) => {
                println!("Received new bot trade event: {:?}", trade_info);
            },
            PumpfunEvent::Error(err) => {
                println!("Received error: {}", err);
            }
        }
    };

    // Start subscription
    let subscription = tokens_subscription(
        ws_url,
        commitment,
        callback,
        None
    ).await.unwrap();

    // Wait for a while to receive events
    tokio::time::sleep(tokio::time::Duration::from_secs(60)).await;

    // Stop subscription
    stop_subscription(subscription).await;

    Ok(())  
}