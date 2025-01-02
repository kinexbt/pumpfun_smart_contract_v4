use serde::{Serialize, Deserialize};

#[derive(Debug)]
pub enum DexInstruction {
    CreateToken(CreateTokenInfo),
    Trade(TradeInfo),
    Other,
}

// 添加新的数据结构
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CreateTokenInfo {
    pub signature: String,
    pub name: String,
    pub symbol: String,
    pub uri: String,
    pub mint: String,
    pub bonding_curve: String,
    pub user: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TradeInfo {
    pub signature: String,
    pub mint: String,
    pub bonding_curve: String,
    pub sol_amount: u64,
    pub token_amount: u64,
    pub is_buy: bool,
    pub user: String,
    pub timestamp: i64,
    pub virtual_sol_reserves: u64,
    pub virtual_token_reserves: u64,
    pub real_sol_reserves: u64,
    pub real_token_reserves: u64,
}