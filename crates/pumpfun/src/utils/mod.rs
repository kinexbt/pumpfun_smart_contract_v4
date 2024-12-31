//! Utilities for working with token metadata and IPFS uploads.
//!
//! This module provides functionality for creating and managing token metadata,
//! including uploading image and metadata to IPFS via the Pump.fun API.

use isahc::AsyncReadResponseExt;
use serde::{Deserialize, Serialize};
use std::{fs::File, io::Read};

/// Metadata structure for a token, matching the format expected by Pump.fun.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenMetadata {
    /// Name of the token
    pub name: String,
    /// Token symbol (e.g. "BTC")
    pub symbol: String,
    /// Description of the token
    pub description: String,
    /// IPFS URL of the token's image
    pub image: String,
    /// Whether to display the token's name
    pub show_name: bool,
    /// Creation timestamp/source
    pub created_on: String,
    /// Twitter handle
    pub twitter: Option<String>,
    /// Telegram handle
    pub telegram: Option<String>,
    /// Website URL
    pub website: Option<String>,
}

/// Response received after successfully uploading token metadata.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenMetadataResponse {
    /// The uploaded token metadata
    pub metadata: TokenMetadata,
    /// IPFS URI where the metadata is stored
    pub metadata_uri: String,
}

/// Parameters for creating new token metadata.
#[derive(Debug, Clone)]
pub struct CreateTokenMetadata {
    /// Name of the token
    pub name: String,
    /// Token symbol (e.g. "BTC")
    pub symbol: String,
    /// Description of the token
    pub description: String,
    /// Path to the token's image file
    pub file: String,
    /// Optional Twitter handle
    pub twitter: Option<String>,
    /// Optional Telegram group
    pub telegram: Option<String>,
    /// Optional website URL
    pub website: Option<String>,
}

/// Creates and uploads token metadata to IPFS via the Pump.fun API.
///
/// This function takes token metadata and an image file, constructs a multipart form request,
/// and uploads it to the Pump.fun IPFS API endpoint. The metadata and image are stored on IPFS
/// and the function returns the IPFS locations.
///
/// # Arguments
///
/// * `metadata` - Token metadata and image file information
///
/// # Returns
///
/// Returns a `Result` containing the `TokenMetadataResponse` with IPFS locations on success,
/// or an error if the upload fails.
///
/// # Examples
///
/// ```rust,no_run
/// use pumpfun::utils::{CreateTokenMetadata, create_token_metadata};
///
/// # async fn run() -> Result<(), Box<dyn std::error::Error>> {
/// let metadata = CreateTokenMetadata {
///     name: "My Token".to_string(),
///     symbol: "MT".to_string(),
///     description: "A test token".to_string(),
///     file: "path/to/image.png".to_string(),
///     twitter: None,
///     telegram: None,
///     website: Some("https://example.com".to_string()),
/// };
///
/// let response = create_token_metadata(metadata).await?;
/// println!("Metadata URI: {}", response.metadata_uri);
/// # Ok(())
/// # }
/// ```
pub async fn create_token_metadata(
    metadata: CreateTokenMetadata,
) -> Result<TokenMetadataResponse, Box<dyn std::error::Error>> {
    let boundary = "------------------------f4d9c2e8b7a5310f";
    let mut body = Vec::new();

    // Helper function to append form data
    fn append_text_field(body: &mut Vec<u8>, boundary: &str, name: &str, value: &str) {
        body.extend_from_slice(b"--");
        body.extend_from_slice(boundary.as_bytes());
        body.extend_from_slice(b"\r\n");
        body.extend_from_slice(
            format!("Content-Disposition: form-data; name=\"{}\"\r\n\r\n", name).as_bytes(),
        );
        body.extend_from_slice(value.as_bytes());
        body.extend_from_slice(b"\r\n");
    }

    // Append form fields
    append_text_field(&mut body, boundary, "name", &metadata.name);
    append_text_field(&mut body, boundary, "symbol", &metadata.symbol);
    append_text_field(&mut body, boundary, "description", &metadata.description);
    if let Some(twitter) = metadata.twitter {
        append_text_field(&mut body, boundary, "twitter", &twitter);
    }
    if let Some(telegram) = metadata.telegram {
        append_text_field(&mut body, boundary, "telegram", &telegram);
    }
    if let Some(website) = metadata.website {
        append_text_field(&mut body, boundary, "website", &website);
    }
    append_text_field(&mut body, boundary, "showName", "true");

    // Append file part
    body.extend_from_slice(b"--");
    body.extend_from_slice(boundary.as_bytes());
    body.extend_from_slice(b"\r\n");
    body.extend_from_slice(b"Content-Disposition: form-data; name=\"file\"; filename=\"file\"\r\n");
    body.extend_from_slice(b"Content-Type: application/octet-stream\r\n\r\n");

    // Read the file contents
    let mut file = File::open(&metadata.file)?;
    let mut file_contents = Vec::new();
    file.read_to_end(&mut file_contents)?;
    body.extend_from_slice(&file_contents);

    // Close the boundary
    body.extend_from_slice(b"\r\n--");
    body.extend_from_slice(boundary.as_bytes());
    body.extend_from_slice(b"--\r\n");

    let client = isahc::HttpClient::new()?;
    let request = isahc::Request::builder()
        .method("POST")
        .uri("https://pump.fun/api/ipfs")
        .header(
            "Content-Type",
            format!("multipart/form-data; boundary={}", boundary),
        )
        .header("Content-Length", body.len() as u64)
        .body(isahc::AsyncBody::from(body))?;

    // Send request and print response
    let mut response = client.send_async(request).await?;
    let text = response.text().await?;
    let json: TokenMetadataResponse = serde_json::from_str(&text)?;

    Ok(json)
}

/// Calculates the maximum amount to pay when buying tokens, accounting for slippage tolerance
///
/// # Arguments
/// * `amount` - The base amount in lamports (1 SOL = 1,000,000,000 lamports)
/// * `basis_points` - The slippage tolerance in basis points (1% = 100 basis points)
///
/// # Returns
/// The maximum amount to pay, including slippage tolerance
///
/// # Example
/// ```rust
/// use pumpfun::utils;
///
/// let amount = 1_000_000_000; // 1 SOL in lamports
/// let slippage = 100; // 1% slippage tolerance
///
/// let max_amount = utils::calculate_with_slippage_buy(amount, slippage);
/// assert_eq!(max_amount, 1_010_000_000); // 1.01 SOL
/// ```
pub fn calculate_with_slippage_buy(amount: u64, basis_points: u64) -> u64 {
    amount + (amount * basis_points) / 10000
}

/// Calculates the minimum amount to receive when selling tokens, accounting for slippage tolerance
///
/// # Arguments
/// * `amount` - The base amount in lamports (1 SOL = 1,000,000,000 lamports)
/// * `basis_points` - The slippage tolerance in basis points (1% = 100 basis points)
///
/// # Returns
/// The minimum amount to receive, accounting for slippage tolerance
///
/// # Example
/// ```rust
/// use pumpfun::utils;
///
/// let amount = 1_000_000_000; // 1 SOL in lamports
/// let slippage = 100; // 1% slippage tolerance
///
/// let min_amount = utils::calculate_with_slippage_sell(amount, slippage);
/// assert_eq!(min_amount, 990_000_000); // 0.99 SOL
/// ```
pub fn calculate_with_slippage_sell(amount: u64, basis_points: u64) -> u64 {
    amount - (amount * basis_points) / 10000
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::write;

    #[tokio::test]
    async fn test_create_token_metadata() {
        // Create a temporary file
        let temp_dir = std::env::temp_dir();
        let file_path = temp_dir.join("test_image.png");
        write(&file_path, b"fake image data").unwrap();

        // Create test metadata
        let metadata = CreateTokenMetadata {
            name: "Test Token".to_string(),
            symbol: "TEST".to_string(),
            description: "Test Description".to_string(),
            file: file_path.to_str().unwrap().to_string(),
            twitter: None,
            telegram: None,
            website: Some("https://example.com".to_string()),
        };

        // Call the function
        let result = create_token_metadata(metadata).await;

        // Assert the result
        assert!(result.is_ok());
        let response = result.unwrap();

        // Verify response fields
        assert_eq!(response.metadata.name, "Test Token");
        assert_eq!(response.metadata.symbol, "TEST");
        assert_eq!(response.metadata.description, "Test Description");
        assert!(response.metadata.image.starts_with("https://ipfs.io/ipfs/"));
        assert!(response.metadata.show_name);
        assert_eq!(response.metadata.created_on, "https://pump.fun");
        assert!(response.metadata_uri.starts_with("https://ipfs.io/ipfs/"));
    }

    #[test]
    fn test_calculate_with_slippage_buy() {
        let amount = 1_000_000_000; // 1 SOL in lamports
        let slippage = 100; // 1% slippage tolerance
        let max_amount = calculate_with_slippage_buy(amount, slippage);
        assert_eq!(max_amount, 1_010_000_000); // 1.01 SOL
    }

    #[test]
    fn test_calculate_with_slippage_sell() {
        let amount = 1_000_000_000; // 1 SOL in lamports
        let slippage = 100; // 1% slippage tolerance
        let min_amount = calculate_with_slippage_sell(amount, slippage);
        assert_eq!(min_amount, 990_000_000); // 0.99 SOL
    }
}
