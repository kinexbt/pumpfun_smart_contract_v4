# PumpFun Rust SDK

A comprehensive Rust SDK for seamless interaction with the PumpFun Solana program. This SDK provides a robust set of tools and interfaces to integrate PumpFun functionality into your applications.


# Explanation
1. Add `create, buy, sell` for pump.fun.
2. Add `logs_subscribe` to subscribe the logs of the PumpFun program.
3. Add `yellowstone grpc` to subscribe the logs of the PumpFun program.
4. Add `jito` to send transaction with Jito.
5. Add `nextblock` to send transaction with nextblock.
6. Add `0slot` to send transaction with 0slot.
7. Submit a transaction using Jito, Nextblock, and 0slot simultaneously; the fastest one will succeed, while the others will fail. 

## Usage
```shell
cd `your project root directory`
git clone https://github.com/MiracleAI-Labs/pumpfun-sdk
```

```toml
# add to your Cargo.toml
pumpfun-sdk = { path = "./pumpfun-sdk", version = "2.4.3" }
```

### logs subscription for token create and trade  transaction
```rust
use pumpfun_sdk::{common::logs_events::PumpfunEvent, grpc::YellowstoneGrpc};

// create grpc client
let grpc_url = "http://127.0.0.1:10000";
let client = YellowstoneGrpc::new(grpc_url);

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
let client = GrpcClient::get_instance();
client.subscribe_pumpfun(callback, Some(payer_keypair.pubkey())).await?;

```

### Init Pumpfun instance for configs
```rust
use std::sync::Arc;
use pumpfun_sdk::{common::{Cluster, PriorityFee}, PumpFun};
use solana_sdk::{commitment_config::CommitmentConfig, signature::Keypair, signer::Signer};

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
let payer = Keypair::from_base58_string("your private key");
let pumpfun = PumpFun::new(
    Arc::new(payer), 
    &cluster,
).await;
```

### pumpfun Create Token
```rust
use std::sync::Arc;
use pumpfun_sdk::{ipfs, PumpFun};
use solana_sdk::{native_token::sol_to_lamports, signature::Keypair, signer::Signer};

// create pumpfun instance
let pumpfun = PumpFun::new(Arc::new(payer), &cluster).await;

// Mint keypair
let mint_pubkey: Keypair = Keypair::new();

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

// ipfs_api_key for https://api.pinata.cloud 
let ipfs_metadata = ipfs::create_token_metadata(metadata, &"".to_string()).await?;
println!("ipfs_metadata: {:?}", ipfs_metadata);

pumpfun.create_and_buy(
    mint, 
    ipfs_metadata, 
    sol_to_lamports(0.1), 
    None,
).await?;
```

### pumpfun Buy token
```rust
use pumpfun_sdk::PumpFun;
use solana_sdk::{native_token::sol_to_lamports, signature::Keypair, signer::Signer};

// create pumpfun instance
let pumpfun = PumpFun::new(Arc::new(payer), &cluster).await;

// Mint keypair
let mint_pubkey: Keypair = Keypair::new();

// buy token with tip
pumpfun.buy_with_tip(mint_pubkey, 10000, None).await?;

```

### pumpfun Sell token
```rust
use pumpfun_sdk::PumpFun;
use solana_sdk::{native_token::sol_to_lamports, signature::Keypair, signer::Signer};

// create pumpfun instance
let pumpfun = PumpFun::new(Arc::new(payer), &cluster).await;

// sell token with tip
pumpfun.sell_with_tip(mint_pubkey, 100000, None).await?;

// sell token by percent with tip
pumpfun.sell_by_percent_with_tip(mint_pubkey, 100, None).await?;

```
