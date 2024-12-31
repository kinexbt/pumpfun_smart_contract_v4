use crate::instruction::logs_data::{CreateTokenInfo, TradeInfo};
use crate::instruction::logs_paser::{parse_create_token_data, parse_trade_data};
use crate::error::ClientResult;

pub struct LogFilter;

#[derive(Debug)]
pub enum DexInstruction {
    CreateToken(CreateTokenInfo),
    Trade(TradeInfo),
    Other,
}

impl LogFilter {
    const PROGRAM_ID: &'static str = "6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P";
    
    /// 解析交易日志并返回具体的指令类型和数据
    pub fn parse_instruction(logs: &[String]) -> ClientResult<Vec<DexInstruction>> {
        let mut current_instruction = None;
        let mut program_data = String::new();
        let mut invoke_depth = 0;
        let mut last_data_len = 0;
        let mut instructions = Vec::new();
        for log in logs {
            // println!("log: {:?}", log);
            // 检查程序调用
            if log.contains(&format!("Program {} invoke", Self::PROGRAM_ID)) {
                invoke_depth += 1;
                if invoke_depth == 1 {  // 只在顶层调用时重置状态
                    current_instruction = None;
                    program_data.clear();
                    last_data_len = 0;
                }
                continue;
            }
            
            // 如果不在我们的程序中，跳过
            if invoke_depth == 0 {
                continue;
            }
            
            // 识别指令类型（只在顶层调用时）
            if invoke_depth == 1 && log.contains("Program log: Instruction:") {
                if log.contains("Create") {
                    current_instruction = Some("create");
                } else if log.contains("Buy") || log.contains("Sell") {
                    current_instruction = Some("trade");
                }
                continue;
            }
            
            // 收集 Program data
            if log.starts_with("Program data: ") {
                let data = log.trim_start_matches("Program data: ");
                if data.len() > last_data_len {
                    program_data = data.to_string();
                    last_data_len = data.len();
                }
            }
            
            // 检查程序是否结束
            if log.contains(&format!("Program {} success", Self::PROGRAM_ID)) {
                invoke_depth -= 1;
                if invoke_depth == 0 {  // 只在顶层程序结束时处理数据
                    if let Some(instruction_type) = current_instruction {
                        if !program_data.is_empty() {
                            match instruction_type {
                                "create" => {
                                    if let Ok(token_info) = parse_create_token_data(&program_data) {
                                        instructions.push(DexInstruction::CreateToken(token_info));
                                    }
                                },
                                "trade" => {
                                    if let Ok(trade_info) = parse_trade_data(&program_data) {
                                        instructions.push(DexInstruction::Trade(trade_info));
                                    }
                                },
                                _ => {}
                            }
                        }
                    }
                }
            }
        }

        Ok(instructions)
    }
} 