use base64::engine::general_purpose;
use base64::Engine;
use regex::Regex;
use crate::common::logs_data::{CreateTokenInfo, TradeInfo, EventTrait};

pub const PROGRAM_DATA: &str = "Program data: ";

#[derive(Debug)]
pub enum PumpfunEvent {
    NewToken(CreateTokenInfo),
    NewUserTrade(TradeInfo),
    NewBotTrade(TradeInfo),
    Error(String),
}


#[derive(Debug)]
pub enum DexEvent {
    NewToken(CreateTokenInfo),
    NewUserTrade(TradeInfo),
    NewBotTrade(TradeInfo),
    Error(String),
}

// #[derive(Debug, Clone, Copy)]
// pub struct PumpEvent {}

impl PumpfunEvent {
    pub fn parse_logs(logs: &Vec<String>) -> (Option<CreateTokenInfo>, Option<TradeInfo>) {
        let mut create_info: Option<CreateTokenInfo> = None;
        let mut trade_info: Option<TradeInfo> = None;

        if !logs.is_empty() {
            let logs_iter = logs.iter().peekable();

            for l in logs_iter.rev() {
                if let Some(log) = l.strip_prefix(PROGRAM_DATA) {
                    let borsh_bytes = general_purpose::STANDARD.decode(log).unwrap();
                    let slice: &[u8] = &borsh_bytes[8..];

                    if create_info.is_none() {
                        if let Ok(e) = CreateTokenInfo::from_bytes(slice) {
                            create_info = Some(e);
                            continue;
                        }
                    }

                    if trade_info.is_none() {
                        if let Ok(e) = TradeInfo::from_bytes(slice) {
                            trade_info = Some(e);
                        }
                    }
                }
            }
        }
        (create_info, trade_info)
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