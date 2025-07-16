# PumpFun Rust SDK

[中文](https://github.com/0xfnzero/pumpfun-sdk/blob/main/README_CN.md) | [English](https://github.com/0xfnzero/pumpfun-sdk/blob/main/README.md) | [Telegram](https://t.me/fnzero_group)

一个用于与 PumpFun Solana 程序无缝交互的全面 Rust SDK。该 SDK 提供了一套强大的工具和接口，方便你将 PumpFun 的功能集成到你的应用程序中。


# 功能说明
1. 为 pump.fun 增加了 `create, buy, sell` 功能。
2. 增加了 `logs_subscribe`，用于订阅 PumpFun 程序的日志。
3. 增加了 `yellowstone grpc`，用于订阅 PumpFun 程序的日志。
4. 增加了 `jito`，可通过 Jito 发送交易。
5. 增加了 `nextblock`，可通过 nextblock 发送交易。
6. 增加了 `0slot`，可通过 0slot 发送交易。
7. 可同时通过 Jito、Nextblock 和 0slot 提交交易，最快成功的会生效，其余的会失败。

## 使用方法
```shell
cd `your project root directory`
git clone https://github.com/0xfnzero/pumpfun-sdk
```

```toml
# 添加到你的 Cargo.toml
pumpfun-sdk = { path = "./pumpfun-sdk", version = "2.4.3" }
```

### 订阅代币创建和交易的日志
```rust
use pumpfun_sdk::{common::logs_events::PumpfunEvent, grpc::YellowstoneGrpc};

// create grpc client
let grpc_url = "http://127.0.0.1:10000";
let client = YellowstoneGrpc::new(grpc_url);

// Define callback function
let callback = |event: PumpfunEvent| {
    match event {
        PumpfunEvent::NewToken(token_info) => {
            println!("Received new token event: {:?}", token_info);
        },
        PumpfunEvent::NewDevTrade(trade_info) => {
            println!("Received dev trade event: {:?}", trade_info);
        },
        PumpfunEvent::NewUserTrade(trade_info) => {
            println!("Received new trade event: {:?}", trade_info);
        },
        PumpfunEvent::NewBotTrade(trade_info) => {
            println!("Received new bot trade event: {:?}", trade_info);
        }
        PumpfunEvent::Error(err) => {
            println!("Received error: {}", err);
        }
    }
};

let payer_keypair = Keypair::from_base58_string("your private key");
client.subscribe_pumpfun(callback, Some(payer_keypair.pubkey())).await?;
```

### 初始化 pumpfun 实例及配置
```rust
use std::sync::Arc;
use pumpfun_sdk::{common::{Cluster, PriorityFee}, PumpFun};
use solana_sdk::{commitment_config::CommitmentConfig, signature::Keypair, signer::Signer};

let priority_fee = PriorityFee{
    unit_limit: 190000,
    unit_price: 1000000,
    buy_tip_fee: 0.001,
    sell_tip_fee: 0.0001,
};

let cluster = Cluster {
    rpc_url: "https://api.mainnet-beta.solana.com".to_string(),
    block_engine_url: "https://block-engine.example.com".to_string(),
    nextblock_url: "https://nextblock.example.com".to_string(),
    nextblock_auth_token: "nextblock_api_token".to_string(),
    zeroslot_url: "https://zeroslot.example.com".to_string(),
    zeroslot_auth_token: "zeroslot_api_token".to_string(),
    use_jito: true,
    use_nextblock: false,
    use_zeroslot: false,
    priority_fee,
    commitment: CommitmentConfig::processed(),
};

// create pumpfun instance
let payer = Keypair::from_base58_string("your private key");
let pumpfun = PumpFun::new(
    Arc::new(payer), 
    &cluster,
).await;
```

### pumpfun 购买代币
```rust
use pumpfun_sdk::PumpFun;
use solana_sdk::{native_token::sol_to_lamports, signature::Keypair, signer::Signer};

// create pumpfun instance
let pumpfun = PumpFun::new(Arc::new(payer), &cluster).await;

// Mint keypair
let mint_pubkey: Keypair = Keypair::new();

// buy token with tip
pumpfun.buy_with_tip(mint_pubkey, 10000, None).await?;

```

### pumpfun 卖出代币
```rust
use pumpfun_sdk::PumpFun;
use solana_sdk::{native_token::sol_to_lamports, signature::Keypair, signer::Signer};

// create pumpfun instance
let pumpfun = PumpFun::new(Arc::new(payer), &cluster).await;

// sell token with tip
pumpfun.sell_with_tip(mint_pubkey, 100000, None).await?;

// sell token by percent with tip
pumpfun.sell_by_percent_with_tip(mint_pubkey, 100, None).await?;

```

### Telegram group:
https://t.me/fnzero_group
