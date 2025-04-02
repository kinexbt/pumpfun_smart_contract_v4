use borsh::{BorshDeserialize, BorshSerialize};
use solana_sdk::pubkey::Pubkey;

use crate::error::{ClientError, ClientResult};

#[derive(Debug)]
pub enum DexInstruction {
    CreateToken(CreateTokenInfo),
    UserTrade(TradeInfo),
    BotTrade(TradeInfo),
    Other,
}

#[derive(Clone, Debug, Default, PartialEq, BorshDeserialize, BorshSerialize)]
pub struct CreateTokenInfo {
    pub slot: u64,
    pub name: String,
    pub symbol: String,
    pub uri: String,
    pub mint: Pubkey,
    pub bonding_curve: Pubkey,
    pub user: Pubkey,
}

#[derive(Clone, Debug, Default, PartialEq, BorshDeserialize, BorshSerialize)]
pub struct TradeInfo {
    pub slot: u64,
    pub mint: Pubkey,
    pub sol_amount: u64,
    pub token_amount: u64,
    pub is_buy: bool,
    pub user: Pubkey,
    pub timestamp: i64,
    pub virtual_sol_reserves: u64,
    pub virtual_token_reserves: u64,
    pub real_sol_reserves: u64,
    pub real_token_reserves: u64,
}

#[derive(Clone, Debug, Default, PartialEq, BorshDeserialize, BorshSerialize)]
pub struct CompleteInfo {
    pub user: Pubkey,
    pub mint: Pubkey,
    pub bonding_curve: Pubkey,
    pub timestamp: u64,
}

#[derive(Clone, Debug, Default, PartialEq, BorshDeserialize, BorshSerialize)]
pub struct SwapBaseInLog {
    pub log_type: u8,
    // input
    pub amount_in: u64,
    pub minimum_out: u64,
    pub direction: u64,
    // user info
    pub user_source: u64,
    // pool info
    pub pool_coin: u64,
    pub pool_pc: u64,
    // calc result
    pub out_amount: u64,
}

pub trait EventTrait: Sized + std::fmt::Debug {
    fn from_bytes(bytes: &[u8]) -> ClientResult<Self>;
}

impl EventTrait for CreateTokenInfo {
    fn from_bytes(bytes: &[u8]) -> ClientResult<Self> {
        CreateTokenInfo::try_from_slice(bytes).map_err(|e| ClientError::Other(e.to_string()))
    }
}

impl EventTrait for TradeInfo {
    fn from_bytes(bytes: &[u8]) -> ClientResult<Self> {
        TradeInfo::try_from_slice(bytes).map_err(|e| ClientError::Other(e.to_string()))
    }
}

impl EventTrait for CompleteInfo {
    fn from_bytes(bytes: &[u8]) -> ClientResult<Self> {
        CompleteInfo::try_from_slice(bytes).map_err(|e| ClientError::Other(e.to_string()))
    }
}

impl EventTrait for SwapBaseInLog {
    fn from_bytes(bytes: &[u8]) -> ClientResult<Self> {
        SwapBaseInLog::try_from_slice(bytes).map_err(|e| ClientError::Other(e.to_string()))
    }
}