# PumpFun Rust SDK

A comprehensive Rust SDK for seamless interaction with the PumpFun Solana program. This SDK provides a robust set of tools and interfaces to integrate PumpFun functionality into your applications.


# Explanation
This repository is forked from [https://github.com/nhuxhr/pumpfun-rs](https://github.com/nhuxhr/pumpfun-rs).  

1. Change `PumpFun<'a>` to `PumpFun`, and `payer: &'a Keypair` to `payer: Arc<Keypair>`.
2. Add `logs_filters` and `logs_paser` to parse the logs of the PumpFun program.
3. Add `logs_data` to define the data structure of the logs.
4. Add `logs_subscribe` to subscribe the logs of the PumpFun program.
5. Add `logs_events` to define the event of the logs.
6. Add `logs_parser` to parse the logs.

## Installation

Add the following to your `Cargo.toml`:

```toml
[dependencies]
mai3-pumpfun-sdk = "2.3.0"
```
