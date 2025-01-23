use anyhow::anyhow;
use base64::engine::general_purpose;
use base64::Engine;
use borsh::{BorshDeserialize, BorshSerialize};
use regex::Regex;
use solana_sdk::pubkey::Pubkey;

use super::myerror::AppError;

pub const PROGRAM_DATA: &str = "Program data: ";

#[derive(Clone, Debug, Default, PartialEq, BorshDeserialize, BorshSerialize)]
pub struct TradeEvent {
    pub mint: Pubkey,
    pub sol_amount: u64,
    pub token_amount: u64,
    pub is_buy: bool,
    pub user: Pubkey,
    pub timestamp: u64,
    pub virtual_sol_reserves: u64,
    pub virtual_token_reserves: u64,
    pub real_sol_reserves: u64,
    pub real_token_reserves: u64,
}

#[derive(Clone, Debug, Default, PartialEq, BorshDeserialize, BorshSerialize)]
pub struct CompleteEvent {
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
    fn from_bytes(bytes: &[u8]) -> Result<Self, AppError>;
}

impl EventTrait for TradeEvent {
    fn from_bytes(bytes: &[u8]) -> Result<Self, AppError> {
        TradeEvent::try_from_slice(bytes).map_err(|e| AppError::from(anyhow!(e.to_string())))
    }
}

impl EventTrait for CompleteEvent {
    fn from_bytes(bytes: &[u8]) -> Result<Self, AppError> {
        CompleteEvent::try_from_slice(bytes).map_err(|e| AppError::from(anyhow!(e.to_string())))
    }
}

impl EventTrait for SwapBaseInLog {
    fn from_bytes(bytes: &[u8]) -> Result<Self, AppError> {
        SwapBaseInLog::try_from_slice(bytes).map_err(|e| AppError::from(anyhow!(e.to_string())))
    }
}

#[derive(Debug, Clone, Copy)]
pub struct PumpEvent {}

impl PumpEvent {
    pub fn parse_logs<T: EventTrait + Clone>(logs: &Vec<String>) -> Option<T> {
        let mut event: Option<T> = None;
        if !logs.is_empty() {
            let logs_iter = logs.iter().peekable();

            for l in logs_iter.rev() {
                if let Some(log) = l.strip_prefix(PROGRAM_DATA) {
                    let borsh_bytes = general_purpose::STANDARD.decode(log).unwrap();
                    let slice: &[u8] = &borsh_bytes[8..];

                    if let Ok(e) = T::from_bytes(slice) {
                        event = Some(e);
                    }
                }
            }
        }
        event
    }
}

#[derive(Debug, Clone, Copy)]
pub struct RaydiumEvent {}

impl RaydiumEvent {
    pub fn parse_logs<T: EventTrait + Clone>(logs: &Vec<String>) -> Option<T> {
        let mut event: Option<T> = None;

        if !logs.is_empty() {
            let logs_iter = logs.iter().peekable();

            for l in logs_iter.rev() {
                let re = Regex::new(r"ray_log: (?P<base64>[A-Za-z0-9+/=]+)").unwrap();

                if let Some(caps) = re.captures(l) {
                    if let Some(base64) = caps.name("base64") {
                        let bytes = general_purpose::STANDARD.decode(base64.as_str()).unwrap();

                        if let Ok(e) = T::from_bytes(&bytes) {
                            event = Some(e);
                        }
                    }
                }
            }
        }

        event
    }
}