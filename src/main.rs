use std::sync::Arc;
use pumpfun_sdk::{common::{logs_events::PumpfunEvent, Cluster, PriorityFee}, grpc::YellowstoneGrpc, ipfs, PumpFun};
use solana_sdk::{commitment_config::CommitmentConfig, native_token::sol_to_lamports, signature::Keypair, signer::Signer};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // create grpc client
    let grpc_url = "http://127.0.0.1:10000";
    let client = YellowstoneGrpc::new(grpc_url.to_string());

    // Define callback function
    let callback = |event: PumpfunEvent| {
        match event {
            PumpfunEvent::NewToken(token_info) => {
                println!("Received new token event: {:?}", token_info);
            },
            PumpfunEvent::NewDevTrade(trade_info) => {
                println!("Received dev trade event: {:?}", trade_info);
            },
            PumpfunEvent::NewUserTrade(trade_info) => {
                println!("Received new trade event: {:?}", trade_info);
            },
            PumpfunEvent::NewBotTrade(trade_info) => {
                println!("Received new bot trade event: {:?}", trade_info);
            }
            PumpfunEvent::Error(err) => {
                println!("Received error: {}", err);
            }
        }
    };

    let payer_keypair = Keypair::from_base58_string("your private key");
    client.subscribe_pumpfun(callback, Some(payer_keypair.pubkey())).await?;

    Ok(())  
}