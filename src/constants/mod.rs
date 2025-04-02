//! Constants used by the crate.
//!
//! This module contains various constants used throughout the crate, including:
//!
//! - Seeds for deriving Program Derived Addresses (PDAs)
//! - Program account addresses and public keys
//!
//! The constants are organized into submodules for better organization:
//!
//! - `seeds`: Contains seed values used for PDA derivation
//! - `accounts`: Contains important program account addresses

/// Constants used as seeds for deriving PDAs (Program Derived Addresses)
pub mod seeds {
    /// Seed for the global state PDA
    pub const GLOBAL_SEED: &[u8] = b"global";

    /// Seed for the mint authority PDA
    pub const MINT_AUTHORITY_SEED: &[u8] = b"mint-authority";

    /// Seed for bonding curve PDAs
    pub const BONDING_CURVE_SEED: &[u8] = b"bonding-curve";

    /// Seed for metadata PDAs
    pub const METADATA_SEED: &[u8] = b"metadata";
}

/// Constants related to program accounts and authorities
pub mod accounts {
    use std::collections::{HashMap, HashSet};

    use solana_sdk::{pubkey, pubkey::Pubkey};

    /// Public key for the Pump.fun program
    pub const PUMPFUN: Pubkey = pubkey!("6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P");

    /// Public key for the MPL Token Metadata program
    pub const MPL_TOKEN_METADATA: Pubkey = pubkey!("metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s");

    /// Authority for program events
    pub const EVENT_AUTHORITY: Pubkey = pubkey!("Ce6TQqeHC9p8KetsN6JsjHK7UTZk7nasjjnr7XxXp9F1");

    /// System Program ID
    pub const SYSTEM_PROGRAM: Pubkey = pubkey!("11111111111111111111111111111111");

    /// Token Program ID
    pub const TOKEN_PROGRAM: Pubkey = pubkey!("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA");

    /// Associated Token Program ID
    pub const ASSOCIATED_TOKEN_PROGRAM: Pubkey =
        pubkey!("ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL");

    /// Rent Sysvar ID
    pub const RENT: Pubkey = pubkey!("SysvarRent111111111111111111111111111111111");

    pub const JITO_TIP_ACCOUNTS: [&str; 8] = [
        "96gYZGLnJYVFmbjzopPSU6QiEV5fGqZNyN9nmNhvrZU5",
        "HFqU5x63VTqvQss8hp11i4wVV8bD44PvwucfZ2bU7gRe",
        "Cw8CFyM9FkoMi7K7Crf6HNQqf4uEMzpKw6QNghXLvLkY",
        "ADaUMid9yfUytqMBgopwjb2DTLSokTSzL1zt6iGPaS49",
        "DfXygSm4jCyNCybVYYK6DwvWqjKee8pbDmJGcLWNDXjh",
        "ADuUkR4vqLUMWXxW9gh6D6L8pMSawimctcNZ5pGwDcEt",
        "DttWaMuVvTiduZRnguLF7jNxTgiMBZ1hyAumKUiL2KRL",
        "3AVi9Tg9Uo68tJfuvoKvqKNWKkC5wPdSSdeBnizKZ6jT",
    ];


    /// Tip accounts
    pub const NEXTBLOCK_TIP_ACCOUNTS: &[&str] = &[
        "NextbLoCkVtMGcV47JzewQdvBpLqT9TxQFozQkN98pE",
        "NexTbLoCkWykbLuB1NkjXgFWkX9oAtcoagQegygXXA2",
        "NeXTBLoCKs9F1y5PJS9CKrFNNLU1keHW71rfh7KgA1X",
        "NexTBLockJYZ7QD7p2byrUa6df8ndV2WSd8GkbWqfbb",
        "neXtBLock1LeC67jYd1QdAa32kbVeubsfPNTJC1V5At",
        "nEXTBLockYgngeRmRrjDV31mGSekVPqZoMGhQEZtPVG",
        "NEXTbLoCkB51HpLBLojQfpyVAMorm3zzKg7w9NFdqid",
        "nextBLoCkPMgmG8ZgJtABeScP35qLa2AMCNKntAP7Xc"
    ];

    pub const ZEROSLOT_TIP_ACCOUNTS: &[&str] = &[
        "Eb2KpSC8uMt9GmzyAEm5Eb1AAAgTjRaXWFjKyFXHZxF3",
        "FCjUJZ1qozm1e8romw216qyfQMaaWKxWsuySnumVCCNe",
        "ENxTEjSQ1YabmUpXAdCgevnHQ9MHdLv8tzFiuiYJqa13",
        "6rYLG55Q9RpsPGvqdPNJs4z5WTxJVatMB8zV3WJhs5EK",
        "Cix2bHfqPcKcM233mzxbLk14kSggUUiz2A87fJtGivXr",
    ];

    pub const AMM_PROGRAM: Pubkey = pubkey!("675kPX9MHTjS2zt1qfr1NYHuzeLXfQM9H24wFSUt1Mp8");
}

pub mod trade {
    pub const TRADER_TIP_AMOUNT: f64 = 0.0001;
    pub const DEFAULT_SLIPPAGE: u64 = 3000; // 30%
    pub const DEFAULT_COMPUTE_UNIT_LIMIT: u32 = 78000;
    pub const DEFAULT_COMPUTE_UNIT_PRICE: u64 = 500000;
    pub const DEFAULT_BUY_TIP_FEE: f64 = 0.0006;
    pub const DEFAULT_SELL_TIP_FEE: f64 = 0.0001;
}

pub struct Symbol;

impl Symbol {
    pub const SOLANA: &'static str = "solana";
}
