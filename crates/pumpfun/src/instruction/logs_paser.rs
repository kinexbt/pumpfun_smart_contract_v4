use crate::error::{ClientError, ClientResult};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use serde::{Serialize, Deserialize};

use crate::instruction::logs_data::*;
// 添加解析函数
pub fn parse_create_token_data(data: &str) -> ClientResult<CreateTokenInfo> {
    // 首先进行 base64 解码
    let decoded = BASE64.decode(data)
        .map_err(|e| ClientError::Other(format!("Failed to decode base64: {}", e)))?;
    
    // 跳过前缀字节（如果有的话）
    let mut cursor = if decoded.len() > 8 { 8 } else { 0 };
    
    // 读取名称长度和名称
    if cursor + 4 > decoded.len() {
        return Err(ClientError::Other("Data too short for name length".to_string()));
    }
    let name_len = read_u32(&decoded[cursor..]) as usize;
    cursor += 4;
    
    if cursor + name_len > decoded.len() {
        return Err(ClientError::Other(format!("Data too short for name: need {} bytes", name_len)));
    }
    let name = String::from_utf8(decoded[cursor..cursor + name_len].to_vec())
        .map_err(|e| ClientError::Other(format!("Invalid UTF-8 in name: {}", e)))?;
    cursor += name_len;
    
    // 读取符号长度和符号
    if cursor + 4 > decoded.len() {
        return Err(ClientError::Other("Data too short for symbol length".to_string()));
    }
    let symbol_len = read_u32(&decoded[cursor..]) as usize;
    cursor += 4;
    
    if cursor + symbol_len > decoded.len() {
        return Err(ClientError::Other(format!("Data too short for symbol: need {} bytes", symbol_len)));
    }
    let symbol = String::from_utf8(decoded[cursor..cursor + symbol_len].to_vec())
        .map_err(|e| ClientError::Other(format!("Invalid UTF-8 in symbol: {}", e)))?;
    cursor += symbol_len;
    
    // 读取 URI 长度和 URI
    if cursor + 4 > decoded.len() {
        return Err(ClientError::Other("Data too short for URI length".to_string()));
    }
    let uri_len = read_u32(&decoded[cursor..]) as usize;
    cursor += 4;
    
    if cursor + uri_len > decoded.len() {
        return Err(ClientError::Other(format!("Data too short for URI: need {} bytes", uri_len)));
    }
    let uri = String::from_utf8(decoded[cursor..cursor + uri_len].to_vec())
        .map_err(|e| ClientError::Other(format!("Invalid UTF-8 in uri: {}", e)))?;
    cursor += uri_len;
    
    // 确保还有足够的数据来读取公钥
    if cursor + 32 * 3 > decoded.len() {
        return Err(ClientError::Other("Data too short for public keys".to_string()));
    }
    
    // 解析 Mint Public Key
    let mint = bs58::encode(&decoded[cursor..cursor+32]).into_string();
    cursor += 32;

    // 解析 Bonding Curve Public Key
    let bonding_curve = bs58::encode(&decoded[cursor..cursor+32]).into_string();
    cursor += 32;

    // 解析 User Public Key
    let user = bs58::encode(&decoded[cursor..cursor+32]).into_string();

    Ok(CreateTokenInfo {
        signature: String::new(),
        name,
        symbol,
        uri,
        mint,
        bonding_curve,
        user,
    })
}

fn read_u32(data: &[u8]) -> u32 {
    let mut bytes = [0u8; 4];
    bytes.copy_from_slice(&data[..4]);
    u32::from_le_bytes(bytes)
}

pub fn parse_trade_data(data: &str) -> ClientResult<TradeInfo> {
    let engine = base64::engine::general_purpose::STANDARD;
    let decoded = engine.decode(data).map_err(|e| 
        ClientError::Parse(
            "Failed to decode base64".to_string(),
            e.to_string()
        )
    )?;

    let mut cursor = 8;  // 跳过前缀

    // 1. Mint (32 bytes)
    let mint = bs58::encode(&decoded[cursor..cursor + 32]).into_string();
    cursor += 32;

    // 2. Sol Amount (8 bytes)
    let sol_amount = u64::from_le_bytes(decoded[cursor..cursor + 8].try_into().unwrap());
    cursor += 8;

    // 3. Token Amount (8 bytes)
    let token_amount = u64::from_le_bytes(decoded[cursor..cursor + 8].try_into().unwrap());
    cursor += 8;

    // 4. Is Buy (1 byte)
    let is_buy = decoded[cursor] != 0;
    cursor += 1;

    // 5. User (32 bytes)
    let user = bs58::encode(&decoded[cursor..cursor + 32]).into_string();
    cursor += 32;

    // 6. Timestamp (8 bytes)
    let timestamp = i64::from_le_bytes(decoded[cursor..cursor + 8].try_into().unwrap());
    cursor += 8;

    // 7. Virtual Sol Reserves (8 bytes)
    let virtual_sol_reserves = u64::from_le_bytes(decoded[cursor..cursor + 8].try_into().unwrap());
    cursor += 8;

    // 8. Virtual Token Reserves (8 bytes)
    let virtual_token_reserves = u64::from_le_bytes(decoded[cursor..cursor + 8].try_into().unwrap());
    cursor += 8;

    let real_sol_reserves = u64::from_le_bytes(decoded[cursor..cursor + 8].try_into().unwrap());
    cursor += 8;

    let real_token_reserves = u64::from_le_bytes(decoded[cursor..cursor + 8].try_into().unwrap());

    Ok(TradeInfo {
        signature: String::new(),
        mint,
        bonding_curve: String::new(),
        sol_amount,
        token_amount,
        is_buy,
        user,
        timestamp,
        virtual_sol_reserves,
        virtual_token_reserves,
        real_sol_reserves,
        real_token_reserves,
    })
}