use serde::{Serialize, Deserialize};
use crate::common::logs_data::{CreateTokenInfo, TradeInfo};
use crate::common::event::{CreateEvent, TradeEvent};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DexEvent {
    NewToken(CreateTokenInfo),
    NewUserTrade(TradeInfo),
    NewBotTrade(TradeInfo),
    Error(String),
}