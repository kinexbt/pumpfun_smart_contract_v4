// #![doc = include_str!("../RUSTDOC.md")]

pub mod accounts;
pub mod constants;
pub mod error;
pub mod instruction;
pub mod utils;

use crate::error::ClientError::*;

use anchor_client::{
    solana_client::rpc_client::RpcClient,
    solana_sdk::{
        commitment_config::CommitmentConfig,
        pubkey::Pubkey,
        signature::{Keypair, Signature},
        signer::Signer,
    },
    Client, Cluster, Program,
};
use anchor_spl::associated_token::{
    get_associated_token_address,
    spl_associated_token_account::instruction::create_associated_token_account,
};
use borsh::BorshDeserialize;
pub use pumpfun_cpi as cpi;
use solana_sdk::compute_budget::ComputeBudgetInstruction;
use std::sync::Arc;

/// Configuration for priority fee compute unit parameters
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PriorityFee {
    /// Maximum compute units that can be consumed by the transaction
    pub limit: Option<u32>,
    /// Price in micro-lamports per compute unit
    pub price: Option<u64>,
}

/// Main client for interacting with the Pump.fun program
pub struct PumpFun {
    /// RPC client for Solana network requests
    pub rpc: RpcClient,
    /// Keypair used to sign transactions
    pub payer: Arc<Keypair>,
    /// Anchor client instance
    pub client: Client<Arc<Keypair>>,
    /// Anchor program instance
    pub program: Program<Arc<Keypair>>,
}

impl PumpFun {
    /// Creates a new PumpFun client instance
    ///
    /// # Arguments
    ///
    /// * `cluster` - Solana cluster to connect to (e.g. devnet, mainnet-beta)
    /// * `payer` - Keypair used to sign and pay for transactions
    /// * `options` - Optional commitment config for transaction finality
    /// * `ws` - Whether to use websocket connection instead of HTTP
    ///
    /// # Returns
    ///
    /// Returns a new PumpFun client instance configured with the provided parameters
    pub fn new(
        cluster: Cluster,
        payer: Arc<Keypair>,
        options: Option<CommitmentConfig>,
        ws: Option<bool>,
    ) -> Self {
        // Create Solana RPC Client with either WS or HTTP endpoint
        let rpc: RpcClient = RpcClient::new(if ws.unwrap_or(false) {
            cluster.ws_url()
        } else {
            cluster.url()
        });

        // Create Anchor Client with optional commitment config
        let client: Client<Arc<Keypair>> = if let Some(options) = options {
            Client::new_with_options(cluster.clone(), payer.clone(), options)
        } else {
            Client::new(cluster.clone(), payer.clone())
        };

        // Create Anchor Program instance for Pump.fun
        let program: Program<Arc<Keypair>> = client.program(cpi::ID).unwrap();

        // Return configured PumpFun client
        Self {
            rpc,
            payer,
            client,
            program,
        }
    }

    /// Creates a new token with metadata by uploading metadata to IPFS and initializing on-chain accounts
    ///
    /// # Arguments
    ///
    /// * `mint` - Keypair for the new token mint account that will be created
    /// * `metadata` - Token metadata including name, symbol, description and image file
    /// * `priority_fee` - Optional priority fee configuration for compute units
    ///
    /// # Returns
    ///
    /// Returns the transaction signature if successful, or a ClientError if the operation fails
    pub async fn create(
        &self,
        mint: &Keypair,
        metadata: utils::CreateTokenMetadata,
        priority_fee: Option<PriorityFee>,
    ) -> Result<Signature, error::ClientError> {
        // First upload metadata and image to IPFS
        let ipfs: utils::TokenMetadataResponse = utils::create_token_metadata(metadata)
            .await
            .map_err(error::ClientError::UploadMetadataError)?;

        let mut request = self.program.request();

        // Add priority fee if provided
        if let Some(fee) = priority_fee {
            if let Some(limit) = fee.limit {
                let limit_ix = ComputeBudgetInstruction::set_compute_unit_limit(limit);
                request = request.instruction(limit_ix);
            }

            if let Some(price) = fee.price {
                let price_ix = ComputeBudgetInstruction::set_compute_unit_price(price);
                request = request.instruction(price_ix);
            }
        }

        // Add create token instruction
        request = request.instruction(instruction::create(
            &self.payer.clone().as_ref(),
            mint,
            cpi::instruction::Create {
                _name: ipfs.metadata.name,
                _symbol: ipfs.metadata.symbol,
                _uri: ipfs.metadata.image,
            },
        ));

        // Add signers
        request = request.signer(&self.payer).signer(mint);

        // Send transaction
        let signature: Signature = request
            .send()
            .await
            .map_err(error::ClientError::AnchorClientError)?;

        Ok(signature)
    }

    /// Creates a new token and immediately buys an initial amount in a single atomic transaction
    ///
    /// # Arguments
    ///
    /// * `mint` - Keypair for the new token mint
    /// * `metadata` - Token metadata to upload to IPFS
    /// * `amount_sol` - Amount of SOL to spend on initial buy in lamports
    /// * `slippage_basis_points` - Optional maximum acceptable slippage in basis points (1 bp = 0.01%). Defaults to 500
    /// * `priority_fee` - Optional priority fee configuration for compute units
    ///
    /// # Returns
    ///
    /// Returns the transaction signature if successful, or a ClientError if the operation fails
    pub async fn create_and_buy(
        &self,
        mint: &Keypair,
        metadata: utils::CreateTokenMetadata,
        amount_sol: u64,
        slippage_basis_points: Option<u64>,
        priority_fee: Option<PriorityFee>,
    ) -> Result<Signature, error::ClientError> {
        // Upload metadata to IPFS first
        let ipfs: utils::TokenMetadataResponse = utils::create_token_metadata(metadata)
            .await
            .map_err(error::ClientError::UploadMetadataError)?;

        // Get accounts and calculate buy amounts
        let global_account = self.get_global_account()?;
        let buy_amount = global_account.get_initial_buy_price(amount_sol);
        let buy_amount_with_slippage =
            utils::calculate_with_slippage_buy(amount_sol, slippage_basis_points.unwrap_or(500));

        let mut request = self.program.request();

        // Add priority fee if provided
        if let Some(fee) = priority_fee {
            if let Some(limit) = fee.limit {
                let limit_ix = ComputeBudgetInstruction::set_compute_unit_limit(limit);
                request = request.instruction(limit_ix);
            }

            if let Some(price) = fee.price {
                let price_ix = ComputeBudgetInstruction::set_compute_unit_price(price);
                request = request.instruction(price_ix);
            }
        }

        // Add create token instruction
        request = request.instruction(instruction::create(
            &self.payer.clone().as_ref(),
            mint,
            cpi::instruction::Create {
                _name: ipfs.metadata.name,
                _symbol: ipfs.metadata.symbol,
                _uri: ipfs.metadata.image,
            },
        ));

        // Create Associated Token Account if needed
        let ata: Pubkey = get_associated_token_address(&self.payer.pubkey(), &mint.pubkey());
        if self.rpc.get_account(&ata).is_err() {
            request = request.instruction(create_associated_token_account(
                &self.payer.pubkey(),
                &self.payer.pubkey(),
                &mint.pubkey(),
                &constants::accounts::TOKEN_PROGRAM,
            ));
        }

        // Add buy instruction
        request = request.instruction(instruction::buy(
            &self.payer.clone().as_ref(),
            &mint.pubkey(),
            &global_account.fee_recipient,
            cpi::instruction::Buy {
                _amount: buy_amount,
                _max_sol_cost: buy_amount_with_slippage,
            },
        ));

        // Add signers and send transaction
        let signature: Signature = request
            .signer(&self.payer)
            .signer(mint)
            .send()
            .await
            .map_err(error::ClientError::AnchorClientError)?;

        Ok(signature)
    }

    /// Buys tokens from a bonding curve by spending SOL
    ///
    /// # Arguments
    ///
    /// * `mint` - Public key of the token mint to buy
    /// * `amount_sol` - Amount of SOL to spend in lamports
    /// * `slippage_basis_points` - Optional maximum acceptable slippage in basis points (1 bp = 0.01%). Defaults to 500
    /// * `priority_fee` - Optional priority fee configuration for compute units
    ///
    /// # Returns
    ///
    /// Returns the transaction signature if successful, or a ClientError if the operation fails
    pub async fn buy(
        &self,
        mint: &Pubkey,
        amount_sol: u64,
        slippage_basis_points: Option<u64>,
        priority_fee: Option<PriorityFee>,
    ) -> Result<Signature, error::ClientError> {
        // Get accounts and calculate buy amounts
        let global_account = self.get_global_account()?;
        let bonding_curve_account = self.get_bonding_curve_account(mint)?;
        let buy_amount = bonding_curve_account
            .get_buy_price(amount_sol)
            .map_err(error::ClientError::BondingCurveError)?;
        let buy_amount_with_slippage =
            utils::calculate_with_slippage_buy(amount_sol, slippage_basis_points.unwrap_or(500));

        let mut request = self.program.request();

        // Add priority fee if provided
        if let Some(fee) = priority_fee {
            if let Some(limit) = fee.limit {
                let limit_ix = ComputeBudgetInstruction::set_compute_unit_limit(limit);
                request = request.instruction(limit_ix);
            }

            if let Some(price) = fee.price {
                let price_ix = ComputeBudgetInstruction::set_compute_unit_price(price);
                request = request.instruction(price_ix);
            }
        }

        // Create Associated Token Account if needed
        let ata: Pubkey = get_associated_token_address(&self.payer.pubkey(), mint);
        if self.rpc.get_account(&ata).is_err() {
            request = request.instruction(create_associated_token_account(
                &self.payer.pubkey(),
                &self.payer.pubkey(),
                mint,
                &constants::accounts::TOKEN_PROGRAM,
            ));
        }

        // Add buy instruction
        request = request.instruction(instruction::buy(
            &self.payer.clone().as_ref(),
            mint,
            &global_account.fee_recipient,
            cpi::instruction::Buy {
                _amount: buy_amount,
                _max_sol_cost: buy_amount_with_slippage,
            },
        ));

        // Add signer
        request = request.signer(&self.payer);

        // Send transaction
        let signature: Signature = request
            .send()
            .await
            .map_err(error::ClientError::AnchorClientError)?;

        Ok(signature)
    }

    /// Sells tokens back to the bonding curve in exchange for SOL
    ///
    /// # Arguments
    ///
    /// * `mint` - Public key of the token mint to sell
    /// * `amount_token` - Optional amount of tokens to sell in base units. If None, sells entire balance
    /// * `slippage_basis_points` - Optional maximum acceptable slippage in basis points (1 bp = 0.01%). Defaults to 500
    /// * `priority_fee` - Optional priority fee configuration for compute units
    ///
    /// # Returns
    ///
    /// Returns the transaction signature if successful, or a ClientError if the operation fails
    pub async fn sell(
        &self,
        mint: &Pubkey,
        amount_token: Option<u64>,
        slippage_basis_points: Option<u64>,
        priority_fee: Option<PriorityFee>,
    ) -> Result<Signature, error::ClientError> {
        // Get accounts and calculate sell amounts
        let ata: Pubkey = get_associated_token_address(&self.payer.pubkey(), mint);
        let balance = self.rpc.get_token_account_balance(&ata).unwrap();
        let balance_u64: u64 = balance.amount.parse::<u64>().unwrap();
        let _amount = amount_token.unwrap_or(balance_u64);
        let global_account = self.get_global_account()?;
        let bonding_curve_account = self.get_bonding_curve_account(mint)?;
        let min_sol_output = bonding_curve_account
            .get_sell_price(_amount, global_account.fee_basis_points)
            .map_err(error::ClientError::BondingCurveError)?;
        let _min_sol_output = utils::calculate_with_slippage_sell(
            min_sol_output,
            slippage_basis_points.unwrap_or(500),
        );

        let mut request = self.program.request();

        // Add priority fee if provided
        if let Some(fee) = priority_fee {
            if let Some(limit) = fee.limit {
                let limit_ix = ComputeBudgetInstruction::set_compute_unit_limit(limit);
                request = request.instruction(limit_ix);
            }

            if let Some(price) = fee.price {
                let price_ix = ComputeBudgetInstruction::set_compute_unit_price(price);
                request = request.instruction(price_ix);
            }
        }

        // Add sell instruction
        request = request.instruction(instruction::sell(
            &self.payer.clone().as_ref(),
            mint,
            &global_account.fee_recipient,
            cpi::instruction::Sell {
                _amount,
                _min_sol_output,
            },
        ));

        // Add signer
        request = request.signer(&self.payer);

        // Send transaction
        let signature: Signature = request
            .send()
            .await
            .map_err(error::ClientError::AnchorClientError)?;

        Ok(signature)
    }

    /// Gets the Program Derived Address (PDA) for the global state account
    ///
    /// # Returns
    ///
    /// Returns the PDA public key derived from the GLOBAL_SEED
    pub fn get_global_pda() -> Pubkey {
        let seeds: &[&[u8]; 1] = &[constants::seeds::GLOBAL_SEED];
        let program_id: &Pubkey = &cpi::ID;
        Pubkey::find_program_address(seeds, program_id).0
    }

    /// Gets the Program Derived Address (PDA) for the mint authority
    ///
    /// # Returns
    ///
    /// Returns the PDA public key derived from the MINT_AUTHORITY_SEED
    pub fn get_mint_authority_pda() -> Pubkey {
        let seeds: &[&[u8]; 1] = &[constants::seeds::MINT_AUTHORITY_SEED];
        let program_id: &Pubkey = &cpi::ID;
        Pubkey::find_program_address(seeds, program_id).0
    }

    /// Gets the Program Derived Address (PDA) for a token's bonding curve account
    ///
    /// # Arguments
    ///
    /// * `mint` - Public key of the token mint
    ///
    /// # Returns
    ///
    /// Returns Some(PDA) if derivation succeeds, or None if it fails
    pub fn get_bonding_curve_pda(mint: &Pubkey) -> Option<Pubkey> {
        let seeds: &[&[u8]; 2] = &[constants::seeds::BONDING_CURVE_SEED, mint.as_ref()];
        let program_id: &Pubkey = &cpi::ID;
        let pda: Option<(Pubkey, u8)> = Pubkey::try_find_program_address(seeds, program_id);
        pda.map(|pubkey| pubkey.0)
    }

    /// Gets the Program Derived Address (PDA) for a token's metadata account
    ///
    /// # Arguments
    ///
    /// * `mint` - Public key of the token mint
    ///
    /// # Returns
    ///
    /// Returns the PDA public key for the token's metadata account
    pub fn get_metadata_pda(mint: &Pubkey) -> Pubkey {
        let seeds: &[&[u8]; 3] = &[
            constants::seeds::METADATA_SEED,
            constants::accounts::MPL_TOKEN_METADATA.as_ref(),
            mint.as_ref(),
        ];
        let program_id: &Pubkey = &constants::accounts::MPL_TOKEN_METADATA;
        Pubkey::find_program_address(seeds, program_id).0
    }

    /// Gets the global state account data containing program-wide configuration
    ///
    /// # Returns
    ///
    /// Returns the deserialized GlobalAccount if successful, or a ClientError if the operation fails
    pub fn get_global_account(&self) -> Result<accounts::GlobalAccount, error::ClientError> {
        let global: Pubkey = Self::get_global_pda();

        let account = self
            .rpc
            .get_account(&global)
            .map_err(error::ClientError::SolanaClientError)?;

        accounts::GlobalAccount::try_from_slice(&account.data)
            .map_err(error::ClientError::BorshError)
    }

    /// Gets a token's bonding curve account data containing pricing parameters
    ///
    /// # Arguments
    ///
    /// * `mint` - Public key of the token mint
    ///
    /// # Returns
    ///
    /// Returns the deserialized BondingCurveAccount if successful, or a ClientError if the operation fails
    pub fn get_bonding_curve_account(
        &self,
        mint: &Pubkey,
    ) -> Result<accounts::BondingCurveAccount, error::ClientError> {
        let bonding_curve_pda =
            Self::get_bonding_curve_pda(mint).ok_or(error::ClientError::BondingCurveNotFound)?;

        let account = self
            .rpc
            .get_account(&bonding_curve_pda)
            .map_err(error::ClientError::SolanaClientError)?;

        accounts::BondingCurveAccount::try_from_slice(&account.data)
            .map_err(error::ClientError::BorshError)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anchor_client::solana_sdk::signer::keypair::Keypair;

    #[test]
    fn test_new_client() {
        let payer = Arc::new(Keypair::new());
        let client = PumpFun::new(Cluster::Devnet, Arc::clone(&payer), None, None);
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
