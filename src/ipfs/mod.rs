use std::time::Duration;

use serde_json::Value;
use tokio::fs::File;
use tokio::io::AsyncReadExt;
use reqwest::Client;
use reqwest::multipart::{Form, Part};
use base64::{Engine as _, engine::general_purpose};
use serde::{Deserialize, Serialize};

/// Metadata structure for a token, matching the format expected by Pump.fun.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenMetadataIPFS {
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

    pub metadata_uri: Option<String>,
}

pub async fn create_token_metadata(metadata: CreateTokenMetadata, jwt_token: &str) -> Result<TokenMetadataIPFS, anyhow::Error> {
    let ipfs_url = if metadata.file.starts_with("http") || metadata.metadata_uri.is_some() {
        metadata.file
    } else {
        let base64_string = file_to_base64(&metadata.file).await?;
        upload_base64_file(&base64_string, jwt_token).await?
    };

    let token_metadata = TokenMetadata {
        name: metadata.name,
        symbol: metadata.symbol,
        description: metadata.description,
        image: ipfs_url,
        show_name: true,
        created_on: "https://pump.fun".to_string(),
        twitter: metadata.twitter,
        telegram: metadata.telegram,
        website: metadata.website,
    };

    if metadata.metadata_uri.is_some() {
        let token_metadata_ipfs = TokenMetadataIPFS {
            metadata: token_metadata,
            metadata_uri: metadata.metadata_uri.unwrap(),
        };  
        Ok(token_metadata_ipfs)
    } else {
        let client = Client::new();
        let response = client
            .post("https://api.pinata.cloud/pinning/pinJSONToIPFS")
            .header("Content-Type", "application/json")
            .header("Authorization", format!("Bearer {}", jwt_token))
            .json(&token_metadata)
        .send()
        .await?;

        // 确保请求成功
        if response.status().is_success() {
            let res_data: serde_json::Value = response.json().await?;
            let ipfs_hash = res_data["IpfsHash"].as_str().unwrap();
            let ipfs_url = format!("https://ipfs.io/ipfs/{}", ipfs_hash);
            let token_metadata_ipfs = TokenMetadataIPFS {
                metadata: token_metadata,
                metadata_uri: ipfs_url,
            };  
            Ok(token_metadata_ipfs)
        } else {
            eprintln!("Error: {:?}", response.status());
            Err(anyhow::anyhow!("Failed to create token metadata"))
        }
    }
}

pub async fn upload_base64_file(base64_string: &str, jwt_token: &str) -> Result<String, anyhow::Error> {
    let decoded_bytes = general_purpose::STANDARD.decode(base64_string)?;

    let client = Client::builder()
        .timeout(Duration::from_secs(120))  // 增加超时时间到120秒
        .pool_max_idle_per_host(0) // 禁用连接池
        .pool_idle_timeout(None) // 禁用空闲超时
        .build()?;

    let part = Part::bytes(decoded_bytes)
        .file_name("file.png") // 添加文件扩展名
        .mime_str("image/png")?; // 指定正确的MIME类型

    let form = Form::new().part("file", part);

    let response = client
        .post("https://api.pinata.cloud/pinning/pinFileToIPFS")
        .header("Authorization", format!("Bearer {}", jwt_token))
        .header("Accept", "application/json")
        .multipart(form)
        .send()
        .await?;

    if response.status().is_success() {
        let response_json: Value = response.json().await.map_err(|e| anyhow::anyhow!("Failed to parse JSON: {}", e))?;  
        println!("{:#?}", response_json);
        let ipfs_hash = response_json["IpfsHash"].as_str().unwrap();
        let ipfs_url = format!("https://ipfs.io/ipfs/{}", ipfs_hash);
        Ok(ipfs_url)
    } else {
        let error_text = response.text().await?;
        eprintln!("Error: {:?}", error_text);
        Err(anyhow::anyhow!("Failed to upload file to IPFS: {}", error_text))
    }
}

async fn file_to_base64(file_path: &str) -> Result<String, anyhow::Error> {
    let mut file = File::open(file_path).await?;
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer).await?;
    let base64_string = general_purpose::STANDARD.encode(&buffer);
    Ok(base64_string)
}
