use crate::common::logs_data::DexInstruction;
use crate::common::logs_parser::{parse_create_token_data, parse_trade_data};
use crate::error::ClientResult;
use solana_sdk::pubkey::Pubkey;
pub struct LogFilter;

impl LogFilter {
    const PROGRAM_ID: &'static str = "6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P";
    
    /// Parse transaction logs and return instruction type and data
    pub fn parse_instruction(logs: &[String], bot_wallet: Option<Pubkey>) -> ClientResult<Vec<DexInstruction>> {
        let mut current_instruction = None;
        let mut program_data = String::new();
        let mut invoke_depth = 0;
        let mut last_data_len = 0;
        let mut instructions = Vec::new();
        for log in logs {
            // Check program invocation
            if log.contains(&format!("Program {} invoke", Self::PROGRAM_ID)) {
                invoke_depth += 1;
                if invoke_depth == 1 {  // Only reset state at top level call
                    current_instruction = None;
                    program_data.clear();
                    last_data_len = 0;
                }
                continue;
            }
            
            // Skip if not in our program
            if invoke_depth == 0 {
                continue;
            }
            
            // Identify instruction type (only at top level)
            if invoke_depth == 1 && log.contains("Program log: Instruction:") {
                if log.contains("Create") {
                    current_instruction = Some("create");
                } else if log.contains("Buy") || log.contains("Sell") {
                    current_instruction = Some("trade");
                }
                continue;
            }
            
            // Collect Program data
            if log.starts_with("Program data: ") {
                let data = log.trim_start_matches("Program data: ");
                if data.len() > last_data_len {
                    program_data = data.to_string();
                    last_data_len = data.len();
                }
            }
            
            // Check if program ends
            if log.contains(&format!("Program {} success", Self::PROGRAM_ID)) {
                invoke_depth -= 1;
                if invoke_depth == 0 {  // Only process data when top level program ends
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
                                        if let Some(bot_wallet_pubkey) = bot_wallet {
                                            if trade_info.user == bot_wallet_pubkey.to_string() {
                                                instructions.push(DexInstruction::BotTrade(trade_info));
                                            } else {
                                                instructions.push(DexInstruction::UserTrade(trade_info));
                                            }
                                        } else {
                                            instructions.push(DexInstruction::UserTrade(trade_info));
                                        }
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