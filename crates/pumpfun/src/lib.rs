// #![doc = include_str!("../RUSTDOC.md")]

pub mod accounts;
pub mod constants;
pub mod error;
pub mod instruction;
pub mod utils;
pub mod jito;

use anchor_client::{
    solana_client::rpc_client::RpcClient,
    solana_sdk::{
        commitment_config::CommitmentConfig,
        pubkey::Pubkey,
        signature::{Keypair, Signature},
        signer::Signer,
        instruction::Instruction,
        system_instruction,
        compute_budget::ComputeBudgetInstruction,
        transaction::Transaction,
    },
    Client, Cluster, Program,
};
use anchor_spl::associated_token::{
    get_associated_token_address,
    spl_associated_token_account::instruction::create_associated_token_account,
};
use instruction::logs_subscribe;
use instruction::logs_subscribe::SubscriptionHandle;
use instruction::logs_events::DexEvent;

use std::sync::Arc;
use borsh::BorshDeserialize;
use std::time::Instant;
pub use pumpfun_cpi as cpi;

use crate::jito::JitoClient;
use crate::error::ClientError;

// 常量定义
const DEFAULT_SLIPPAGE: u64 = 500; // 10%
const DEFAULT_COMPUTE_UNIT_LIMIT: u32 = 68_000;
const DEFAULT_COMPUTE_UNIT_PRICE: u64 = 400_000;

/// 优先费用配置
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PriorityFee {
    pub limit: Option<u32>,
    pub price: Option<u64>,
}

/// PumpFun 客户端
pub struct PumpFun {
    pub rpc: RpcClient,
    pub payer: Arc<Keypair>,
    pub client: Client<Arc<Keypair>>,
    pub jito_client: Option<JitoClient>,
    pub program: Program<Arc<Keypair>>,
}

impl PumpFun {
    // 创建新实例
    pub fn new(
        cluster: Cluster,
        jito_url: Option<String>,
        payer: Arc<Keypair>,
        options: Option<CommitmentConfig>,
        ws: Option<bool>,
    ) -> Self {
        let rpc = RpcClient::new(if ws.unwrap_or(false) {
            cluster.ws_url()
        } else {
            cluster.url()
        });

        let jito_client = jito_url.map(|url| JitoClient::new(&url));

        let client = if let Some(options) = options {
            Client::new_with_options(cluster.clone(), payer.clone(), options)
        } else {
            Client::new(cluster.clone(), payer.clone())
        };

        let program = client.program(cpi::ID).unwrap();

        Self {
            rpc,
            payer,
            jito_client,
            client,
            program,
        }
    }

    // 创建代币
    pub async fn create(
        &self,
        mint: &Keypair,
        metadata: utils::CreateTokenMetadata,
        priority_fee: Option<PriorityFee>,
    ) -> Result<Signature, error::ClientError> {
        let ipfs = utils::create_token_metadata(metadata)
            .await
            .map_err(error::ClientError::UploadMetadataError)?;

        let mut request = self.program.request();
        request = self.add_priority_fee(request, priority_fee);

        request = request.instruction(instruction::create(
            &self.payer.clone().as_ref(),
            mint,
            cpi::instruction::Create {
                _name: ipfs.metadata.name,
                _symbol: ipfs.metadata.symbol,
                _uri: ipfs.metadata.image,
            },
        ));

        request = request.signer(&self.payer).signer(mint);

        let signature = request
            .send()
            .await
            .map_err(error::ClientError::AnchorClientError)?;

        Ok(signature)
    }

    // 创建并购买代币
    pub async fn create_and_buy(
        &self,
        mint: &Keypair,
        metadata: utils::CreateTokenMetadata,
        amount_sol: u64,
        slippage_basis_points: Option<u64>,
        priority_fee: Option<PriorityFee>,
    ) -> Result<Signature, error::ClientError> {
        let ipfs = utils::create_token_metadata(metadata)
            .await
            .map_err(error::ClientError::UploadMetadataError)?;

        let global_account = self.get_global_account()?;
        let buy_amount = global_account.get_initial_buy_price(amount_sol);
        let buy_amount_with_slippage =
            utils::calculate_with_slippage_buy(amount_sol, slippage_basis_points.unwrap_or(DEFAULT_SLIPPAGE));

        let mut request = self.program.request();

        request = self.add_priority_fee(request, priority_fee);

        request = request.instruction(instruction::create(
            &self.payer.clone().as_ref(),
            mint,
            cpi::instruction::Create {
                _name: ipfs.metadata.name,
                _symbol: ipfs.metadata.symbol,
                _uri: ipfs.metadata.image,
            },
        ));

        let ata = get_associated_token_address(&self.payer.pubkey(), &mint.pubkey());
        if self.rpc.get_account(&ata).is_err() {
            request = request.instruction(create_associated_token_account(
                &self.payer.pubkey(),
                &self.payer.pubkey(),
                &mint.pubkey(),
                &constants::accounts::TOKEN_PROGRAM,
            ));
        }

        request = request.instruction(instruction::buy(
            &self.payer.clone().as_ref(),
            &mint.pubkey(),
            &global_account.fee_recipient,
            cpi::instruction::Buy {
                _amount: buy_amount,
                _max_sol_cost: buy_amount_with_slippage,
            },
        ));

        let signature = request
            .signer(&self.payer)
            .signer(mint)
            .send()
            .await
            .map_err(error::ClientError::AnchorClientError)?;

        Ok(signature)
    }

    // 购买代币
    pub async fn buy(
        &self,
        mint: &Pubkey,
        amount_sol: u64,
        slippage_basis_points: Option<u64>,
        priority_fee: Option<PriorityFee>,
    ) -> Result<Signature, error::ClientError> {
        let global_account = self.get_global_account()?;
        let bonding_curve_account = self.get_bonding_curve_account(mint)?;
        let buy_amount = bonding_curve_account
            .get_buy_price(amount_sol)
            .map_err(error::ClientError::BondingCurveError)?;
        let buy_amount_with_slippage =
            utils::calculate_with_slippage_buy(amount_sol, slippage_basis_points.unwrap_or(DEFAULT_SLIPPAGE));

        let mut request = self.program.request();

        request = self.add_priority_fee(request, priority_fee);

        let ata = get_associated_token_address(&self.payer.pubkey(), mint);
        if self.rpc.get_account(&ata).is_err() {
            request = request.instruction(create_associated_token_account(
                &self.payer.pubkey(),
                &self.payer.pubkey(),
                mint,
                &constants::accounts::TOKEN_PROGRAM,
            ));
        }

        request = request.instruction(instruction::buy(
            &self.payer.clone().as_ref(),
            mint,
            &global_account.fee_recipient,
            cpi::instruction::Buy {
                _amount: buy_amount,
                _max_sol_cost: buy_amount_with_slippage,
            },
        ));

        let signature = request
            .signer(&self.payer)
            .send()
            .await
            .map_err(error::ClientError::AnchorClientError)?;

        Ok(signature)
    }

    // 使用 Jito 购买代币
    pub async fn buy_with_jito(
        &self,
        mint: &Pubkey,
        amount_sol: u64,
        slippage_basis_points: Option<u64>,
        priority_fee: Option<PriorityFee>,
    ) -> Result<Signature, error::ClientError> {
        let start_time = Instant::now();

        let jito_client = self.jito_client.as_ref().ok_or_else(|| 
            ClientError::Other("Jito client not found".to_string())
        )?;

        let global_account = self.get_global_account()?;
        let bonding_curve_pda = Self::get_bonding_curve_pda(mint).unwrap();
        let bonding_curve_account = self.get_bonding_curve_account(mint)?;
        let buy_amount = bonding_curve_account
            .get_buy_price(amount_sol)
            .map_err(error::ClientError::BondingCurveError)?;
        let buy_amount_with_slippage =
            utils::calculate_with_slippage_buy(amount_sol, slippage_basis_points.unwrap_or(DEFAULT_SLIPPAGE));

        let (unit_limit, _unit_price) = self.get_compute_units(priority_fee);
        let mut instructions = self.create_priority_fee_instructions(priority_fee);

        let priority_fees = jito_client.estimate_priority_fees(&bonding_curve_pda).await?;
        let tip_account = jito_client.get_tip_account().await?;

        let ata = get_associated_token_address(&self.payer.pubkey(), mint);
        if self.rpc.get_account(&ata).is_err() {
            instructions.push(create_associated_token_account(
                &self.payer.pubkey(),
                &self.payer.pubkey(),
                mint,
                &constants::accounts::TOKEN_PROGRAM,
            ));
        }

        instructions.push(instruction::buy(
            &self.payer.clone().as_ref(),
            mint,
            &global_account.fee_recipient,
            cpi::instruction::Buy {
                _amount: buy_amount,
                _max_sol_cost: buy_amount_with_slippage,
            },
        ));

        let total_priority_fee = self.calculate_priority_fee(priority_fees.per_compute_unit.extreme, unit_limit);
        
        instructions.push(
            system_instruction::transfer(
                &self.payer.pubkey(),
                &tip_account,
                total_priority_fee,
            ),
        );

        let recent_blockhash = self.rpc.get_latest_blockhash()?;
        let transaction = Transaction::new_signed_with_payer(
            &instructions,
            Some(&self.payer.pubkey()),
            &[&self.payer.clone()],
            recent_blockhash,
        );

        let signature = jito_client.send_transaction(&transaction).await?;
        println!("Total Jito buy operation time: {:?}ms", start_time.elapsed().as_millis());

        Ok(signature)
    }

    // 出售代币
    pub async fn sell(
        &self,
        mint: &Pubkey,
        amount_token: Option<u64>,
        slippage_basis_points: Option<u64>,
        priority_fee: Option<PriorityFee>,
    ) -> Result<Signature, error::ClientError> {
        let ata = get_associated_token_address(&self.payer.pubkey(), mint);
        let balance = self.rpc.get_token_account_balance(&ata)?;
        let balance_u64 = balance.amount.parse::<u64>().unwrap();
        let amount = amount_token.unwrap_or(balance_u64);
        
        if amount == 0 {
            return Err(ClientError::Other("Balance is 0".to_string()));
        }

        let global_account = self.get_global_account()?;
        let bonding_curve_account = self.get_bonding_curve_account(mint)?;
        let min_sol_output = bonding_curve_account
            .get_sell_price(amount, global_account.fee_basis_points)
            .map_err(error::ClientError::BondingCurveError)?;
        let min_sol_output_with_slippage = utils::calculate_with_slippage_sell(
            min_sol_output,
            slippage_basis_points.unwrap_or(DEFAULT_SLIPPAGE),
        );

        let mut request = self.program.request();

        request = self.add_priority_fee(request, priority_fee);

        request = request.instruction(instruction::sell(
            &self.payer.clone().as_ref(),
            mint,
            &global_account.fee_recipient,
            cpi::instruction::Sell {
                _amount: amount,
                _min_sol_output: min_sol_output_with_slippage,
            },
        ));

        let signature = request
            .signer(&self.payer)
            .send()
            .await
            .map_err(error::ClientError::AnchorClientError)?;

        Ok(signature)
    }

    // 按百分比出售代币
    pub async fn sell_by_percent(
        &self,
        mint: &Pubkey,
        percent: u64,
        slippage_basis_points: Option<u64>,
        priority_fee: Option<PriorityFee>,
    ) -> Result<Signature, error::ClientError> {
        let ata = get_associated_token_address(&self.payer.pubkey(), mint);
        let balance = self.rpc.get_token_account_balance(&ata)?;
        let balance_u64 = balance.amount.parse::<u64>().unwrap();
        
        if balance_u64 == 0 {
            return Err(ClientError::Other("Balance is 0".to_string()));
        }

        let amount = balance_u64 * percent / 100;
        self.sell(mint, Some(amount), slippage_basis_points, priority_fee).await
    }

    // 使用 Jito 出售代币
    pub async fn sell_with_jito(
        &self,
        mint: &Pubkey,
        amount_token: Option<u64>,
        slippage_basis_points: Option<u64>,
        priority_fee: Option<PriorityFee>,
    ) -> Result<Signature, error::ClientError> {
        let start_time = Instant::now();

        let jito_client = self.jito_client.as_ref().ok_or_else(|| 
            ClientError::Other("Jito client not found".to_string())
        )?;

        let ata = get_associated_token_address(&self.payer.pubkey(), mint);
        let balance = self.rpc.get_token_account_balance(&ata)?;
        let balance_u64 = balance.amount.parse::<u64>().unwrap();
        let amount = amount_token.unwrap_or(balance_u64);

        let global_account = self.get_global_account()?;
        let bonding_curve_pda = Self::get_bonding_curve_pda(mint).unwrap();
        let bonding_curve_account = self.get_bonding_curve_account(mint)?;
        let min_sol_output = bonding_curve_account
            .get_sell_price(amount, global_account.fee_basis_points)
            .map_err(error::ClientError::BondingCurveError)?;
        let min_sol_output_with_slippage = utils::calculate_with_slippage_sell(
            min_sol_output,
            slippage_basis_points.unwrap_or(DEFAULT_SLIPPAGE),
        );

        let (unit_limit, _unit_price) = self.get_compute_units(priority_fee);
        let mut instructions = self.create_priority_fee_instructions(priority_fee);

        let priority_fees = jito_client.estimate_priority_fees(&bonding_curve_pda).await?;
        let tip_account = jito_client.get_tip_account().await?;

        instructions.push(instruction::sell(
            &self.payer.clone().as_ref(),
            mint,
            &global_account.fee_recipient,
            cpi::instruction::Sell {
                _amount: amount,
                _min_sol_output: min_sol_output_with_slippage,
            },
        ));

        let total_priority_fee = self.calculate_priority_fee(priority_fees.per_compute_unit.extreme, unit_limit);

        instructions.push(
            system_instruction::transfer(
                &self.payer.pubkey(),
                &tip_account,
                total_priority_fee,
            ),
        );

        let recent_blockhash = self.rpc.get_latest_blockhash()?;
        let transaction = Transaction::new_signed_with_payer(
            &instructions,
            Some(&self.payer.pubkey()),
            &[&self.payer.clone()],
            recent_blockhash,
        );

        let signature = jito_client.send_transaction(&transaction).await?;
        println!("Total Jito sell operation time: {:?}ms", start_time.elapsed().as_millis());

        Ok(signature)
    }

    // 辅助方法
    fn add_priority_fee<'a>(
         &self,
        request: anchor_client::RequestBuilder<'a, Arc<Keypair>>,
        priority_fee: Option<PriorityFee>,
    ) -> anchor_client::RequestBuilder<'a, Arc<Keypair>> {
        let mut request = request;
        if let Some(fee) = priority_fee {
            if let Some(limit) = fee.limit {
                request = request.instruction(ComputeBudgetInstruction::set_compute_unit_limit(limit));
            }
            if let Some(price) = fee.price {
                request = request.instruction(ComputeBudgetInstruction::set_compute_unit_price(price));
            }
        }
        request
    }

    fn get_compute_units(&self, priority_fee: Option<PriorityFee>) -> (u32, u64) {
        let unit_limit = priority_fee
            .and_then(|fee| fee.limit)
            .unwrap_or(DEFAULT_COMPUTE_UNIT_LIMIT);
        let unit_price = priority_fee
            .and_then(|fee| fee.price)
            .unwrap_or(DEFAULT_COMPUTE_UNIT_PRICE);
        (unit_limit, unit_price)
    }

    fn create_priority_fee_instructions(&self, priority_fee: Option<PriorityFee>) -> Vec<Instruction> {
        let mut instructions = Vec::new();
        if let Some(fee) = priority_fee {
            if let Some(limit) = fee.limit {
                instructions.push(ComputeBudgetInstruction::set_compute_unit_limit(limit));
            }
            if let Some(price) = fee.price {
                instructions.push(ComputeBudgetInstruction::set_compute_unit_price(price));
            }
        }
        instructions
    }

    fn calculate_priority_fee(&self, priority_fee_per_cu: u64, unit_limit: u32) -> u64 {
        let total_priority_fee_microlamports = priority_fee_per_cu as u128 * unit_limit as u128;
        (total_priority_fee_microlamports / 1_000_000) as u64
    }

    // 公共接口方法
    pub fn get_payer_pubkey(&self) -> Pubkey {
        self.payer.pubkey()
    }

    pub fn get_token_balance(&self, account: &Pubkey, mint: &Pubkey) -> Result<u64, error::ClientError> {
        let ata = get_associated_token_address(account, mint);
        let balance = self.rpc.get_token_account_balance(&ata)?;
        Ok(balance.amount.parse::<u64>().unwrap())
    }

    pub fn get_sol_balance(&self, account: &Pubkey) -> Result<u64, error::ClientError> {
        self.rpc.get_balance(account).map_err(error::ClientError::SolanaClientError)
    }

    pub fn get_payer_token_balance(&self, mint: &Pubkey) -> Result<u64, error::ClientError> {
        self.get_token_balance(&self.payer.pubkey(), mint)
    }

    pub fn get_payer_sol_balance(&self) -> Result<u64, error::ClientError> {
        self.get_sol_balance(&self.payer.pubkey())
    }

    // PDA 相关方法
    pub fn get_global_pda() -> Pubkey {
        Pubkey::find_program_address(&[constants::seeds::GLOBAL_SEED], &cpi::ID).0
    }

    pub fn get_mint_authority_pda() -> Pubkey {
        Pubkey::find_program_address(&[constants::seeds::MINT_AUTHORITY_SEED], &cpi::ID).0
    }

    pub fn get_bonding_curve_pda(mint: &Pubkey) -> Option<Pubkey> {
        Pubkey::try_find_program_address(
            &[constants::seeds::BONDING_CURVE_SEED, mint.as_ref()],
            &cpi::ID
        ).map(|(pubkey, _)| pubkey)
    }

    pub fn get_metadata_pda(mint: &Pubkey) -> Pubkey {
        Pubkey::find_program_address(
            &[
                constants::seeds::METADATA_SEED,
                constants::accounts::MPL_TOKEN_METADATA.as_ref(),
                mint.as_ref(),
            ],
            &constants::accounts::MPL_TOKEN_METADATA
        ).0
    }

    // 账户相关方法
    pub fn get_global_account(&self) -> Result<accounts::GlobalAccount, error::ClientError> {
        let global = Self::get_global_pda();
        let account = self.rpc.get_account(&global)
            .map_err(error::ClientError::SolanaClientError)?;
        accounts::GlobalAccount::try_from_slice(&account.data)
            .map_err(error::ClientError::BorshError)
    }

    pub fn get_bonding_curve_account(
        &self,
        mint: &Pubkey,
    ) -> Result<accounts::BondingCurveAccount, error::ClientError> {
        let bonding_curve_pda = Self::get_bonding_curve_pda(mint)
            .ok_or(error::ClientError::BondingCurveNotFound)?;
        let account = self.rpc.get_account(&bonding_curve_pda)
            .map_err(error::ClientError::SolanaClientError)?;
        accounts::BondingCurveAccount::try_from_slice(&account.data)
            .map_err(error::ClientError::BorshError)
    }

    // 订阅相关方法
    pub async fn tokens_subscription<F>(
        &self,
        ws_url: &str,
        commitment: CommitmentConfig,
        callback: F,
        bot_wallet: Option<Pubkey>,
    ) -> Result<SubscriptionHandle, Box<dyn std::error::Error>>
    where
        F: Fn(DexEvent) + Send + Sync + 'static,
    {
        logs_subscribe::tokens_subscription(ws_url, commitment, callback, bot_wallet).await
    }

    pub async fn stop_subscription(&self, subscription_handle: SubscriptionHandle) {
        subscription_handle.shutdown().await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_client() {
        let payer = Arc::new(Keypair::new());
        let client = PumpFun::new(Cluster::Devnet, None, Arc::clone(&payer), None, None);
        assert_eq!(client.payer.pubkey(), payer.pubkey());
    }

    #[test]
    fn test_get_pdas() {
        let mint = Keypair::new();
        let global_pda = PumpFun::get_global_pda();
        let mint_authority_pda = PumpFun::get_mint_authority_pda();
        let bonding_curve_pda = PumpFun::get_bonding_curve_pda(&mint.pubkey());
        let metadata_pda = PumpFun::get_metadata_pda(&mint.pubkey());

        assert!(global_pda != Pubkey::default());
        assert!(mint_authority_pda != Pubkey::default());
        assert!(bonding_curve_pda.is_some());
        assert!(metadata_pda != Pubkey::default());
    }
}
