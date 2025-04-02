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

### logs subscription for token create and trade  transaction
```rust
use std::sync::{Arc, OnceLock};
use pumpfun_sdk::grpc::YellowstoneGrpc;

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

### pumpfun Create, Buy, Sell
```rust
use std::sync::{Arc, OnceLock};
use solana_sdk::{
    signature::Keypair,
    commitment_config::CommitmentConfig,
};
use pumpfun_sdk::{error::ClientResult, PumpFun};
use pumpfun_sdk::common::{Cluster, PriorityFee};

let payer = Keypair::from_base58_string(&settings.dex.payer.clone());
let cluster = Cluster::new( 
    rpc_url.clone(),
    jito_url.clone(),
    nextblock_url.clone(),
    nextblock_auth_token.clone(),
    zeroslot_url.clone(),
    zeroslot_auth_token.clone(),
    priority_fee,
    CommitmentConfig::processed(),
    use_jito,
    use_nextblock,
    use_zeroslot,
);

// create pumpfun instance
let pumpfun = PumpFun::new(Arc::new(payer), &cluster).await;

// Mint keypair
let mint_pubkey: Keypair = Keypair::new();

// buy token with tip
pumpfun.buy_with_tip(mint_pubkey, 10000, None).await?;

// sell token by percent with tip
pumpfun.sell_by_percent_with_tip(mint_pubkey, 100, None).await?;

```
