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

async fn create_and_buy() -> Result<(), Box<dyn std::error::Error>> {
    let priority_fee = PriorityFee{
        unit_limit: 72000,
        unit_price: 1000000,
        buy_tip_fee: 0.001,
        sell_tip_fee: 0.0001,
    };

    let cluster = Cluster {
        rpc_url: "https://api.mainnet-beta.solana.com".to_string(),
        block_engine_url: "https://block-engine.example.com".to_string(),
        nextblock_url: "https://nextblock.example.com".to_string(),
        nextblock_auth_token: "nextblock_api_token".to_string(),
        zeroslot_url: "https://zeroslot.example.com".to_string(),
        zeroslot_auth_token: "zeroslot_api_token".to_string(),
        use_jito: true,
        use_nextblock: false,
        use_zeroslot: false,
        priority_fee,
        commitment: CommitmentConfig::processed(),
    };

    // create pumpfun instance
    let payer = Keypair::new();
    let pumpfun = PumpFun::new(
        Arc::new(payer), 
        &cluster,
    ).await;

    let metadata = ipfs::CreateTokenMetadata {
        name: "Doge".to_string(),
        symbol: "DOGE".to_string(),
        description: "Dogecoin".to_string(),
        file: "/Users/joseph/Desktop/doge.png".to_string(),
        twitter: None,
        telegram: None,
        website: None,
        metadata_uri: None,
    };

    // ipfs jwt_token for https://pinata.cloud 
    let jwt_token = "your ipfs jwt_token";
    let ipfs_metadata = ipfs::create_token_metadata(metadata, &jwt_token).await?;
    println!("ipfs_metadata: {:?}", ipfs_metadata);

    let mint = Keypair::new();

    pumpfun.create_and_buy(
        mint, 
        ipfs_metadata, 
        sol_to_lamports(0.1), 
        None,
    ).await?;
    
    Ok(()) // 添加返回值
}