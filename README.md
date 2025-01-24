# PumpFun Rust SDK

A comprehensive Rust SDK for seamless interaction with the PumpFun Solana program. This SDK provides a robust set of tools and interfaces to integrate PumpFun functionality into your applications.


# Explanation
1. Add `logs_filters` to parse the logs.
1. Add `logs_parser` to process the logs.
2. Add `logs_data` to define the data structure of the logs.
4. Add `logs_events` to define the event of the logs.
3. Add `logs_subscribe` to subscribe the logs of the PumpFun program.
6. Add `jito` to send transaction with Jito.

## Installation

Add the following to your `Cargo.toml`:

```toml
[dependencies]
mai3-pumpfun-sdk = "2.4.5"
```

## Usage

### logs subscription for token create and trade  transaction
```rust
use mai3_pumpfun_sdk::common::{
    logs_events::DexEvent,
    logs_subscribe::{tokens_subscription, stop_subscription}
};
use solana_sdk::commitment_config::CommitmentConfig;

println!("Starting token subscription...");

// wss 
let ws_url = "wss://api.mainnet-beta.solana.com";

// Set commitment
let commitment = CommitmentConfig::confirmed();

// Define callback function
let callback = |event: DexEvent| {
    match event {
        DexEvent::NewToken(token_info) => {
            println!("Received new token event: {:?}", token_info);
        },
        DexEvent::NewUserTrade(trade_info) => {
            println!("Received new trade event: {:?}", trade_info);
        },
        DexEvent::NewBotTrade(trade_info) => {
            println!("Received new bot trade event: {:?}", trade_info);
        }
        DexEvent::Error(err) => {
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

tokio::time::sleep(tokio::time::Duration::from_secs(60)).await;

// Stop subscription
stop_subscription(subscription).await;

println!("Ended token subscription.");
```

### pumpfun Create, Buy, Sell
```rust
use solana_sdk::{
    native_token::LAMPORTS_PER_SOL,
    signature::{Keypair, Signature},
    signer::Signer,
};

use mai3_pumpfun_sdk::{accounts::BondingCurveAccount, utils::CreateTokenMetadata, PriorityFee, PumpFun};

// Create a new PumpFun client
let payer: Keypair = Keypair::new();
let client: PumpFun = PumpFun::new(Cluster::Mainnet, None, &payer, None, None);

// Mint keypair
let mint: Keypair = Keypair::new();

// Token metadata
let metadata: CreateTokenMetadata = CreateTokenMetadata {
    name: "Lorem ipsum".to_string(),
    symbol: "LIP".to_string(),
    description: "Lorem ipsum dolor, sit amet consectetur adipisicing elit. Quam, nisi.".to_string(),
    file: "/path/to/image.png".to_string(),
    twitter: None,
    telegram: None,
    website: Some("https://example.com".to_string()),
};

// Optional priority fee to expedite transaction processing (e.g., 100 LAMPORTS per compute unit, equivalent to a 0.01 SOL priority fee)
let fee: Option<PriorityFee> = Some(PriorityFee {
    limit: Some(100_000),
    price: Some(100_000_000),
});

// Create token with metadata
let signature: Signature = client.create(&mint, metadata.clone(), fee).await?;
println!("Created token: {}", signature);

// Print amount of SOL and LAMPORTS
let amount_sol: u64 = 1;
let amount_lamports: u64 = LAMPORTS_PER_SOL * amount_sol;
println!("Amount in SOL: {}", amount_sol);
println!("Amount in LAMPORTS: {}", amount_lamports);

// Create and buy tokens with metadata
let signature: Signature = client.create_and_buy(&mint, metadata.clone(), amount_lamports, None, fee).await?;
println!("Created and bought tokens: {}", signature);

// Print the curve
let curve: BondingCurveAccount = client.get_bonding_curve_account(&mint.pubkey())?;
println!("{:?}", curve);

// Buy tokens (ATA will be created automatically if needed)
let signature: Signature = client.buy(&mint.pubkey(), amount_lamports, None, fee).await?;
println!("Bought tokens: {}", signature);

// Sell tokens (sell all tokens)
let signature: Signature = client.sell(&mint.pubkey(), None, None, fee).await?;
println!("Sold tokens: {}", signature);
```
