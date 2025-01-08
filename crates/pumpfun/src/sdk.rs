use {
    super::jito::JitoClient, crate::{InfrastructureError, InfrastructureResult}, borsh::{BorshDeserialize, BorshSerialize}, solana_client::rpc_client::RpcClient, solana_sdk::{
        commitment_config::CommitmentConfig, compute_budget::ComputeBudgetInstruction, instruction::{AccountMeta, Instruction}, pubkey::Pubkey, signature::{Keypair, Signature, Signer}, system_instruction, system_program, sysvar, transaction::Transaction
    }, spl_associated_token_account::{get_associated_token_address, instruction::create_associated_token_account}, spl_token, std::str::FromStr
};

const PROGRAM_ID: &str = "6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P";
const DEFAULT_SLIPPAGE: u64 = 500; // 10%
const DEFAULT_COMPUTE_UNIT_LIMIT: u32 = 68_000;
const DEFAULT_COMPUTE_UNIT_PRICE: u64 = 400_000;
pub struct PumpFunSDK {
    client: RpcClient,
    jito_client: Option<JitoClient>,
}

impl Clone for PumpFunSDK {
    fn clone(&self) -> Self {
        Self {
            client: RpcClient::new_with_commitment(
                self.client.url().to_string(),
                self.client.commitment()
            ),
            jito_client: self.jito_client.clone(),
        }
    }
}

#[derive(BorshDeserialize, BorshSerialize)]
pub struct GlobalAccount {
    pub discriminator: u64,
    pub initialized: bool,
    pub authority: Pubkey,
    pub fee_recipient: Pubkey,
    pub initial_virtual_token_reserves: u64,
    pub initial_virtual_sol_reserves: u64,
    pub initial_real_token_reserves: u64,
    pub token_total_supply: u64,
    pub fee_basis_points: u64,
}

impl GlobalAccount {
    pub fn get_initial_buy_price(&self, amount: u64) -> u64 {
        if amount == 0 {
            return 0;
        }

        let n = self.initial_virtual_sol_reserves
            .checked_mul(self.initial_virtual_token_reserves)
            .unwrap_or(0);
            
        let i = self.initial_virtual_sol_reserves
            .checked_add(amount)
            .unwrap_or(u64::MAX);
            
        let r = n.checked_div(i)
            .map(|x| x.checked_add(1).unwrap_or(u64::MAX))
            .unwrap_or(0);
            
        let s = self.initial_virtual_token_reserves
            .checked_sub(r)
            .unwrap_or(0);

        if s < self.initial_real_token_reserves {
            s
        } else {
            self.initial_real_token_reserves
        }
    }

    pub fn from_buffer(buffer: &[u8]) -> InfrastructureResult<Self> {
        Self::try_from_slice(buffer)
            .map_err(|e| InfrastructureError::Other(e.to_string()))
    }
}

#[derive(BorshDeserialize, BorshSerialize)]
pub struct BondingCurveAccount {
    pub discriminator: u64,
    pub virtual_token_reserves: u64,
    pub virtual_sol_reserves: u64,
    pub real_token_reserves: u64,
    pub real_sol_reserves: u64,
    pub token_total_supply: u64,
    pub complete: bool,
}

impl BondingCurveAccount {
    pub fn get_buy_price(&self, amount: u64) -> InfrastructureResult<u64> {
        if self.complete {
            return Err(InfrastructureError::Other("Curve is complete".to_string()));
        }

        if amount == 0 {
            return Ok(0);
        }

        // 使用 u128 进行计算，模仿 TypeScript 中的 bigint 计算
        let virtual_sol = self.virtual_sol_reserves as u128;
        let virtual_token = self.virtual_token_reserves as u128;
        let amount = amount as u128;

        // n = virtualSolReserves * virtualTokenReserves
        let n = virtual_sol
            .checked_mul(virtual_token)
            .ok_or_else(|| InfrastructureError::Other("Overflow in virtual reserves multiplication".to_string()))?;

        // i = virtualSolReserves + amount
        let i = virtual_sol
            .checked_add(amount)
            .ok_or_else(|| InfrastructureError::Other("Overflow in sol reserves addition".to_string()))?;

        // r = n / i + 1
        let r = n.checked_div(i)
            .ok_or_else(|| InfrastructureError::Other("Division by zero".to_string()))?
            .checked_add(1)
            .ok_or_else(|| InfrastructureError::Other("Overflow in token calculation".to_string()))?;

        // s = virtualTokenReserves - r
        let s = virtual_token
            .checked_sub(r)
            .ok_or_else(|| InfrastructureError::Other("Underflow in token calculation".to_string()))?;

        // 确保结果不超过 u64
        if s > u64::MAX as u128 {
            return Err(InfrastructureError::Other("Result exceeds u64 max value".to_string()));
        }

        let result = s as u64;
        
        // Return the minimum of s and realTokenReserves
        Ok(if result < self.real_token_reserves {
            result
        } else {
            self.real_token_reserves
        })
    }

    pub fn get_sell_price(&self, amount: u64, fee_basis_points: u64) -> InfrastructureResult<u64> {
        if self.complete {
            return Err(InfrastructureError::Other("Curve is complete".to_string()));
        }

        if amount == 0 {
            return Ok(0);
        }

        let amount = amount as u128;
        let virtual_sol = self.virtual_sol_reserves as u128;
        let virtual_token = self.virtual_token_reserves as u128;
        let fee_basis_points = fee_basis_points as u128;

        // n = (amount * virtualSolReserves) / (virtualTokenReserves + amount)
        let n = amount
            .checked_mul(virtual_sol)
            .ok_or_else(|| InfrastructureError::Other("Overflow in sol calculation".to_string()))?
            .checked_div(
                virtual_token
                    .checked_add(amount)
                    .ok_or_else(|| InfrastructureError::Other("Overflow in token addition".to_string()))?
            )
            .ok_or_else(|| InfrastructureError::Other("Division by zero".to_string()))?;

        // a = (n * feeBasisPoints) / 10000
        let a = n
            .checked_mul(fee_basis_points)
            .ok_or_else(|| InfrastructureError::Other("Overflow in fee calculation".to_string()))?
            .checked_div(10000)
            .ok_or_else(|| InfrastructureError::Other("Division by zero in fee calculation".to_string()))?;

        // result = n - a
        let result = n
            .checked_sub(a)
            .ok_or_else(|| InfrastructureError::Other("Underflow in final calculation".to_string()))?;

        if result > u64::MAX as u128 {
            return Err(InfrastructureError::Other("Result exceeds u64 max value".to_string()));
        }

        Ok(result as u64)
    }

    pub fn get_market_cap_sol(&self) -> u64 {
        if self.virtual_token_reserves == 0 {
            return 0;
        }

        self.token_total_supply
            .checked_mul(self.virtual_sol_reserves)
            .and_then(|n| n.checked_div(self.virtual_token_reserves))
            .unwrap_or(0)
    }

    pub fn get_final_market_cap_sol(&self, fee_basis_points: u64) -> Result<u64, Box<dyn std::error::Error>> {
        let total_sell_value = self.get_buy_out_price(self.real_token_reserves, fee_basis_points)?;
        let total_virtual_value = self.virtual_sol_reserves
            .checked_add(total_sell_value)
            .ok_or("Overflow in virtual value calculation")?;
        let total_virtual_tokens = self.virtual_token_reserves
            .checked_sub(self.real_token_reserves)
            .ok_or("Underflow in virtual tokens calculation")?;

        if total_virtual_tokens == 0 {
            return Ok(0);
        }

        self.token_total_supply
            .checked_mul(total_virtual_value)
            .ok_or("Overflow in market cap calculation")?
            .checked_div(total_virtual_tokens)
            .ok_or("Division by zero in market cap calculation".into())
    }

    pub fn get_buy_out_price(&self, amount: u64, fee_basis_points: u64) -> Result<u64, Box<dyn std::error::Error>> {
        let sol_tokens = if amount < self.real_sol_reserves {
            self.real_sol_reserves
        } else {
            amount
        };

        let total_sell_value = sol_tokens
            .checked_mul(self.virtual_sol_reserves)
            .ok_or("Overflow in sell value calculation")?
            .checked_div(
                self.virtual_token_reserves
                    .checked_sub(sol_tokens)
                    .ok_or("Underflow in token reserves calculation")?
            )
            .ok_or("Division by zero")?
            .checked_add(1)
            .ok_or("Overflow in sell value calculation")?;

        let fee = total_sell_value
            .checked_mul(fee_basis_points)
            .ok_or("Overflow in fee calculation")?
            .checked_div(10000)
            .ok_or("Division by zero in fee calculation")?;

        total_sell_value
            .checked_add(fee)
            .ok_or("Overflow in final price calculation".into())
    }

    pub fn from_buffer(buffer: &[u8]) -> Result<Self, Box<dyn std::error::Error>> {
        Self::try_from_slice(buffer).map_err(|e| e.into())
    }
}

impl PumpFunSDK {
    pub fn new(rpc_url: &str) -> Self {
        Self {
            client: RpcClient::new_with_commitment(
                rpc_url.to_string(),
                CommitmentConfig::processed(),
            ),
            jito_client: None,
        }
    }

    pub fn with_jito(mut self, jito_endpoint: &str) -> Self {
        self.jito_client = Some(JitoClient::new(jito_endpoint));
        self
    }

    // 添加公共方法来获取代币余额
    pub async fn get_token_balance(&self, wallet_address: &Pubkey, mint: &Pubkey) -> InfrastructureResult<u64> {
        // 获取用户代币账户地址
        let user_token_account = get_associated_token_address(&wallet_address, &mint);
        // 首先检查账户是否存在
        match self.client.get_token_account_balance(&user_token_account) {
            Ok(balance) => {
                balance.amount
                    .parse::<u64>()
                    .map_err(|e| InfrastructureError::Other(e.to_string()))
            },
            Err(_) => Ok(0)  // 如果账户不存在，返回 None
        }
    }

    // 添加公共方法来获取 SOL 余额
    pub async fn get_sol_balance(&self, address: &Pubkey) -> Result<u64, Box<dyn std::error::Error>> {
        Ok(self.client.get_balance(address)?)
    }

    // Buy token with SOL
    pub async fn buy(
        &self,
        buyer: &Keypair,
        mint: Pubkey,
        buy_amount_sol: u64,
        slippage_basis_points: Option<u64>,
    ) -> InfrastructureResult<Signature> {
        let start_time = std::time::Instant::now();
        println!("Starting buy operation...");

        // 1. 获取 bonding curve 账户
        let bonding_curve_start = std::time::Instant::now();
        let bonding_curve_address = self.get_bonding_curve_pda(&mint)?;
        let bonding_curve = self.get_bonding_curve_account(&bonding_curve_address).await?;
        println!("Bonding curve virtual sol reserves: {}", bonding_curve.virtual_sol_reserves);
        println!("Bonding curve real sol reserves: {}", bonding_curve.real_sol_reserves);
        println!("Got bonding curve account: {:?}ms", bonding_curve_start.elapsed().as_millis());

        // 2. 计算买入数量和滑点
        let buy_amount = bonding_curve.get_buy_price(buy_amount_sol)?;
        let slippage = slippage_basis_points.unwrap_or(DEFAULT_SLIPPAGE);
        let max_sol_cost = calculate_with_slippage_buy(buy_amount_sol, slippage);

        // 3. 获取全局账户
        let global_start = std::time::Instant::now();
        let global_account = self.get_global_account().await?;
        println!("Got global account: {:?}ms", global_start.elapsed().as_millis());

        // 4. 准备所有指令
        let mut instructions = vec![];

        instructions.push(
            ComputeBudgetInstruction::set_compute_unit_limit(DEFAULT_COMPUTE_UNIT_LIMIT)
        );

        // 5. 添加 ATA 创建指令
        instructions.push(
            create_associated_token_account(
                &buyer.pubkey(),  // payer
                &buyer.pubkey(),  // wallet
                &mint,            // mint
                &spl_token::id(), // token program
            ),
        );

        // 6. 添加买入指令
        instructions.push(
            self.create_buy_instruction(
                &buyer.pubkey(),
                mint,
                global_account.fee_recipient,
                buy_amount,
                max_sol_cost,
                bonding_curve_address,
            )?,
        );

        // 7. 创建并发送交易
        let tx_start = std::time::Instant::now();
        let recent_blockhash = self.client.get_latest_blockhash()?;
        let transaction = Transaction::new_signed_with_payer(
            &instructions,
            Some(&buyer.pubkey()),
            &[buyer],
            recent_blockhash,
        );

        let signature = self.client
            .send_transaction(&transaction)
            .map_err(|e| InfrastructureError::Other(format!("Transaction failed: {}", e)))?;
        println!("Transaction sent and confirmed: {:?}ms", tx_start.elapsed().as_millis());

        println!("Total buy operation time: {:?}ms", start_time.elapsed().as_millis());
        Ok(signature)
    }

    // Sell token for SOL
    pub async fn sell(
        &self,
        seller: &Keypair,
        mint: Pubkey,
        sell_amount: u64,
        slippage_basis_points: Option<u64>,
    ) -> Result<Signature, Box<dyn std::error::Error>> {
        let slippage = slippage_basis_points.unwrap_or(DEFAULT_SLIPPAGE);
        
        // 1. 获取账户信息
        let bonding_curve_address = self.get_bonding_curve_pda(&mint)?;
        let bonding_curve = self.get_bonding_curve_account(&bonding_curve_address).await?;
        let global_account = self.get_global_account().await?;
        
        // 2. 计算卖出价格和滑点
        let sell_price = bonding_curve.get_sell_price(sell_amount, global_account.fee_basis_points)?;
        let min_out_amount = calculate_with_slippage_sell(sell_price, slippage);

        // 3. 构建交易指令
        let instructions = vec![
            // // 设置计算预算
            // ComputeBudgetInstruction::set_compute_unit_limit(DEFAULT_COMPUTE_UNIT_LIMIT),
            // ComputeBudgetInstruction::set_compute_unit_price(DEFAULT_COMPUTE_UNIT_PRICE),
            
            
            // 卖出指令
            self.create_sell_instruction(
                &seller.pubkey(),
                mint,
                global_account.fee_recipient,
                sell_amount,
                min_out_amount,
                bonding_curve_address,
            )?,
        ];

        // 5. 发送交易
        let recent_blockhash = self.client.get_latest_blockhash()?;
        let transaction = Transaction::new_signed_with_payer(
            &instructions,
            Some(&seller.pubkey()),
            &[seller],
            recent_blockhash,
        );

        let signature = self.client.send_transaction(&transaction)?;
        println!("Transaction sent: {}", signature);

        Ok(signature)
    }

    // 辅助函数：获取 bonding curve PDA
    fn get_bonding_curve_pda(&self, mint: &Pubkey) -> InfrastructureResult<Pubkey> {
        let seeds: &[&[u8]] = &[b"bonding-curve", mint.as_ref()];
        let (pda, _) = Pubkey::find_program_address(
            seeds,
            &Pubkey::from_str(PROGRAM_ID).unwrap(),
        );
        Ok(pda)
    }

    // Helper functions
    fn create_buy_instruction(
        &self,
        buyer: &Pubkey,
        mint: Pubkey,
        fee_recipient: Pubkey,
        amount: u64,
        max_sol_cost: u64,
        bonding_curve_address: Pubkey,
    ) -> InfrastructureResult<Instruction> {
        let associated_bonding_curve = get_associated_token_address(&bonding_curve_address, &mint);
        let associated_user = get_associated_token_address(buyer, &mint);
        let global_seeds: &[&[u8]] = &[b"global"];
        let (global_pda, _) = Pubkey::find_program_address(
            global_seeds,
            &Pubkey::from_str(PROGRAM_ID).unwrap(),
        );

        let accounts = vec![
            AccountMeta::new_readonly(global_pda, false),
            AccountMeta::new(fee_recipient, false),
            AccountMeta::new_readonly(mint, false),
            AccountMeta::new(bonding_curve_address, false),
            AccountMeta::new(associated_bonding_curve, false),
            AccountMeta::new(associated_user, false),
            AccountMeta::new(*buyer, true),
            AccountMeta::new_readonly(system_program::id(), false),
            AccountMeta::new_readonly(spl_token::id(), false),
            AccountMeta::new_readonly(sysvar::rent::id(), false),
            AccountMeta::new_readonly(Pubkey::from_str("Ce6TQqeHC9p8KetsN6JsjHK7UTZk7nasjjnr7XxXp9F1").unwrap(), false),
            AccountMeta::new_readonly(Pubkey::from_str(PROGRAM_ID).unwrap(), false),
        ];

        let mut data = Vec::with_capacity(8 + 8 + 8);
        data.extend_from_slice(&[102, 6, 61, 18, 1, 218, 235, 234]); // discriminator
        data.extend_from_slice(&amount.to_le_bytes());
        data.extend_from_slice(&max_sol_cost.to_le_bytes());

        Ok(Instruction {
            program_id: Pubkey::from_str(PROGRAM_ID).unwrap(),
            accounts,
            data,
        })
    }

    fn create_sell_instruction(
        &self,
        seller: &Pubkey,
        mint: Pubkey,
        fee_recipient: Pubkey,
        amount: u64,
        min_sol_output: u64,
        bonding_curve_address: Pubkey,
    ) -> InfrastructureResult<Instruction> {
        let associated_bonding_curve = get_associated_token_address(&bonding_curve_address, &mint);
        let associated_user = get_associated_token_address(seller, &mint);
        let global_seeds: &[&[u8]] = &[b"global"];
        let (global_pda, _) = Pubkey::find_program_address(
            global_seeds,
            &Pubkey::from_str(PROGRAM_ID).unwrap(),
        );

        let accounts = vec![
            AccountMeta::new_readonly(global_pda, false),
            AccountMeta::new(fee_recipient, false),
            AccountMeta::new_readonly(mint, false),
            AccountMeta::new(bonding_curve_address, false),
            AccountMeta::new(associated_bonding_curve, false),
            AccountMeta::new(associated_user, false),
            AccountMeta::new(*seller, true),
            AccountMeta::new_readonly(system_program::id(), false),
            AccountMeta::new_readonly(spl_associated_token_account::id(), false),
            AccountMeta::new_readonly(spl_token::id(), false),
            AccountMeta::new_readonly(Pubkey::from_str("Ce6TQqeHC9p8KetsN6JsjHK7UTZk7nasjjnr7XxXp9F1").unwrap(), false),
            AccountMeta::new_readonly(Pubkey::from_str(PROGRAM_ID).unwrap(), false),
        ];

        let mut data = Vec::with_capacity(8 + 8 + 8);
        data.extend_from_slice(&[51, 230, 133, 164, 1, 127, 131, 173]); // discriminator
        data.extend_from_slice(&amount.to_le_bytes());
        data.extend_from_slice(&min_sol_output.to_le_bytes());

        Ok(Instruction {
            program_id: Pubkey::from_str(PROGRAM_ID).unwrap(),
            accounts,
            data,
        })
    }

    pub async fn get_global_account(&self) -> InfrastructureResult<GlobalAccount> {
        let seeds: &[&[u8]] = &[b"global"];
        let (global_account_pda, _) = Pubkey::find_program_address(
            seeds,
            &Pubkey::from_str(PROGRAM_ID).unwrap(),
        );

        let account = self.client.get_account(&global_account_pda)
            .map_err(|e| InfrastructureError::Other(e.to_string()))?;
        GlobalAccount::from_buffer(&account.data)
            .map_err(|e| InfrastructureError::Other(e.to_string()))
    }

    pub async fn get_bonding_curve_account(&self, bonding_curve_address: &Pubkey) -> InfrastructureResult<BondingCurveAccount> {
        let account = self.client.get_account(&bonding_curve_address)
            .map_err(|e| InfrastructureError::Other(e.to_string()))?;
        BondingCurveAccount::from_buffer(&account.data)
            .map_err(|e| InfrastructureError::Other(e.to_string()))
    }

    /// 使用 Jito MEV 发送买入交易
    pub async fn buy_via_jito(
        &self,
        buyer: &Keypair,
        mint: Pubkey,
        buy_amount_sol: u64,
        slippage_basis_points: Option<u64>,
    ) -> InfrastructureResult<Signature> {
        let jito_client = self.jito_client.as_ref()
            .ok_or_else(|| InfrastructureError::Other("Jito client not configured".to_string()))?;

        let start_time = std::time::Instant::now();
        println!("Starting Jito buy operation...");

        // 1. 获取 bonding curve 账户
        let bonding_curve_address = self.get_bonding_curve_pda(&mint)?;
        let bonding_curve = self.get_bonding_curve_account(&bonding_curve_address).await?;
        
        // 2. 计算买入数量和滑点
        let buy_amount = bonding_curve.get_buy_price(buy_amount_sol)?;
        let slippage = slippage_basis_points.unwrap_or(DEFAULT_SLIPPAGE);
        let max_sol_cost = calculate_with_slippage_buy(buy_amount_sol, slippage);

        // 3. 获取全局账户
        let global_account = self.get_global_account().await?;

        // 4. 获取优先费用估算
        let priority_fees = jito_client.estimate_priority_fees(&bonding_curve_address).await?;
        
        // 计算每计算单元的优先费用（使用 Extreme 级别）
        let priority_fee_per_cu = priority_fees.per_compute_unit.extreme;
        
        // 完整的单位转换过程
        let total_priority_fee_microlamports = priority_fee_per_cu as u128 * DEFAULT_COMPUTE_UNIT_LIMIT as u128;
        let total_priority_fee_lamports = total_priority_fee_microlamports / 1_000_000;
        let total_priority_fee_sol = total_priority_fee_lamports as f64 / 1_000_000_000.0;
        
        println!("Priority fee details:");
        println!("  Per CU (microlamports): {}", priority_fee_per_cu);
        println!("  Total (lamports): {}", total_priority_fee_lamports);
        println!("  Total (SOL): {:.9}", total_priority_fee_sol);

        // 5. 获取 tip account
        let tip_account = jito_client.get_tip_account().await?;

        // 6. 准备所有指令
        let mut instructions = vec![];

        // 添加计算预算指令，包括优先费用
        instructions.push(
            ComputeBudgetInstruction::set_compute_unit_limit(DEFAULT_COMPUTE_UNIT_LIMIT)
        );

        // 添加 ATA 创建指令
        instructions.push(
            create_associated_token_account(
                &buyer.pubkey(),
                &buyer.pubkey(),
                &mint,
                &spl_token::id(),
            ),
        );

        // 添加买入指令
        instructions.push(
            self.create_buy_instruction(
                &buyer.pubkey(),
                mint,
                global_account.fee_recipient,
                buy_amount,
                max_sol_cost,
                bonding_curve_address,
            )?,
        );

        // 添加 tip 指令
        instructions.push(
            system_instruction::transfer(
                &buyer.pubkey(),
                &tip_account,
                total_priority_fee_lamports as u64,
            ),
        );

        // 7. 创建并发送交易
        let recent_blockhash = self.client.get_latest_blockhash()?;
        let transaction = Transaction::new_signed_with_payer(
            &instructions,
            Some(&buyer.pubkey()),
            &[buyer],
            recent_blockhash,
        );

        // 8. 通过 Jito 发送交易
        let signature = jito_client.send_transaction(&transaction).await?;
        println!("Total Jito buy operation time: {:?}ms", start_time.elapsed().as_millis());

        Ok(signature)
    }

    pub fn get_jito_client(&self) -> Option<&JitoClient> {
        self.jito_client.as_ref()
    }

    // 添加基于 Jito 的卖出方法
    pub async fn sell_via_jito(
        &self,
        seller: &Keypair,
        mint: Pubkey,
        sell_amount: u64,
        slippage_basis_points: Option<u64>,
    ) -> InfrastructureResult<Signature> {
        let jito_client = self.jito_client.as_ref()
            .ok_or_else(|| InfrastructureError::Other("Jito client not configured".to_string()))?;

        let start_time = std::time::Instant::now();
        println!("Starting Jito sell operation...");

        // 1. 获取账户信息
        let bonding_curve_address = self.get_bonding_curve_pda(&mint)?;
        let bonding_curve = self.get_bonding_curve_account(&bonding_curve_address).await?;
        let global_account = self.get_global_account().await?;
        
        // 2. 计算卖出价格和滑点
        let sell_price = bonding_curve.get_sell_price(sell_amount, global_account.fee_basis_points)?;
        let slippage = slippage_basis_points.unwrap_or(DEFAULT_SLIPPAGE);
        let min_out_amount = calculate_with_slippage_sell(sell_price, slippage);

        // 3. 获取优先费用估算
        let priority_fees = jito_client.estimate_priority_fees(&bonding_curve_address).await?;
        let priority_fee_per_cu = priority_fees.per_compute_unit.extreme;
        
        // 完整的单位转换过程
        let total_priority_fee_microlamports = priority_fee_per_cu as u128 * DEFAULT_COMPUTE_UNIT_LIMIT as u128;
        let total_priority_fee_lamports = total_priority_fee_microlamports / 1_000_000;
        let total_priority_fee_sol = total_priority_fee_lamports as f64 / 1_000_000_000.0;
        
        println!("Priority fee details:");
        println!("  Per CU (microlamports): {}", priority_fee_per_cu);
        println!("  Total (lamports): {}", total_priority_fee_lamports);
        println!("  Total (SOL): {:.9}", total_priority_fee_sol);

        // 4. 获取 tip account
        let tip_account = jito_client.get_tip_account().await?;

        // 5. 准备所有指令
        let mut instructions = vec![
            // 添加计算预算指令
            ComputeBudgetInstruction::set_compute_unit_limit(DEFAULT_COMPUTE_UNIT_LIMIT),
            
            // 卖出指令
            self.create_sell_instruction(
                &seller.pubkey(),
                mint,
                global_account.fee_recipient,
                sell_amount,
                min_out_amount,
                bonding_curve_address,
            )?,

            // 添加 tip 指令
            system_instruction::transfer(
                &seller.pubkey(),
                &tip_account,
                total_priority_fee_lamports as u64,
            ),
        ];

        // 6. 创建并发送交易
        let recent_blockhash = self.client.get_latest_blockhash()?;
        let transaction = Transaction::new_signed_with_payer(
            &instructions,
            Some(&seller.pubkey()),
            &[seller],
            recent_blockhash,
        );

        // 7. 通过 Jito 发送交易
        let signature = jito_client.send_transaction(&transaction).await?;
        println!("Total Jito sell operation time: {:?}ms", start_time.elapsed().as_millis());

        Ok(signature)
    }
}

fn calculate_with_slippage_sell(amount: u64, basis_points: u64) -> u64 {
    amount.saturating_sub(
        amount.saturating_mul(basis_points)
            .saturating_div(10000)
    )
}

// 添加滑点计算函数
fn calculate_with_slippage_buy(amount: u64, basis_points: u64) -> u64 {
    amount.saturating_add(
        amount.saturating_mul(basis_points)
            .saturating_div(10000)
    )
}