use crate::error::ClientResult;
use crate::instruction::logs_filters::{LogFilter, DexInstruction};

pub async fn process_logs<F>(
    signature: &str,
    logs: Vec<String>,
    callback: F,
) -> ClientResult<()>
where
    F: Fn(&str, DexInstruction) + Send + Sync,
{
    let instructions = LogFilter::parse_instruction(&logs)?;
    for instruction in instructions {
        callback(signature, instruction);
    }
    Ok(())
}