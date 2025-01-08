use serde::{Serialize, Deserialize};
use crate::instruction::logs_data::{CreateTokenInfo, TradeInfo};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DexEvent {
    NewToken(CreateTokenInfo),
    NewUserTrade(TradeInfo),
    NewBotTrade(TradeInfo),
    Error(String),
}