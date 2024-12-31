# Pump.fun Solana Program SDK

This library provides a Rust interface for interacting with the Pump.fun Solana program.
Pump.fun is a Solana-based marketplace that enables users to create and distribute their own tokens, primarily memecoins.

## Getting Started

Add this crate to your project using cargo:

```sh
cargo add pumpfun
```

## Usage

The main entry point is the `PumpFun` struct which provides methods for interacting with the program:

> **Note:** The SDK automatically creates Associated Token Accounts (ATAs) when needed during buy transactions. No manual ATA creation is required.

```rust,no_run
use anchor_client::{
    solana_sdk::{
        native_token::LAMPORTS_PER_SOL,
        signature::{Keypair, Signature},
        signer::Signer,
    },
    Cluster,
};
use pumpfun::{accounts::BondingCurveAccount, utils::CreateTokenMetadata, PriorityFee, PumpFun};

// Create a new PumpFun client
let payer: Keypair = Keypair::new();
let client: PumpFun<'_> = PumpFun::new(Cluster::Mainnet, &payer, None, None);

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

## Features

- Create new tokens with metadata and custom image
- Buy tokens using SOL with automatic ATA creation
- Sell tokens for SOL with slippage protection
- Query global and bonding curve state
- Calculate prices, fees and slippage
- Priority fee support for faster transactions
- IPFS metadata storage

## Architecture

The SDK is organized into several modules:

- `cpi`: Cross-program invocation interfaces
- `accounts`: Account structs for deserializing on-chain state
- `constants`: Program constants like seeds and public keys
- `error`: Custom error types for error handling
- `instruction`: Transaction instruction builders
- `utils`: Helper functions and utilities

The main `PumpFun` struct provides high-level methods that abstract away the complexity of:

- Managing Program Derived Addresses (PDAs)
- Constructing and signing transactions
- Handling account lookups and deserialization
- Calculating prices, fees and slippage
- IPFS metadata uploads
- Priority fee configuration
- Associated Token Account management
