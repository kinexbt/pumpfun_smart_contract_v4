# PumpFun Rust SDK

A comprehensive Rust SDK for seamless interaction with the PumpFun Solana program. This SDK provides a robust set of tools and interfaces to integrate PumpFun functionality into your applications.


# Explanation
This repository is forked from [https://github.com/nhuxhr/pumpfun-rs](https://github.com/nhuxhr/pumpfun-rs).  

1. Change `PumpFun<'a>` to `PumpFun`, and `payer: &'a Keypair` to `payer: Arc<Keypair>`.
2. Add `logs_filter` and `logs_paser` to parse the logs of the PumpFun program.
3. Add `logs_data` to define the data structure of the logs.

## Table of Contents

- [Crates](#crates)
- [Features](#features)
- [Installation](#installation)
- [Usage](#usage)
- [API Reference](#api-reference)
- [Contributing](#contributing)

## Crates

| Name                                  | Description                                                                        | Version |
| ------------------------------------- | ---------------------------------------------------------------------------------- | ------- |
| [`pumpfun`](./crates/pumpfun)         | Main client library for interacting with the PumpFun program                       | 2.0.0   |
| [`pumpfun-cpi`](./crates/pumpfun-cpi) | CPI (Cross-Program Invocation) interfaces for integrating with the PumpFun program | 1.1.0   |

## Features

- **Easy-to-use API**: Simplified interfaces for interacting with the PumpFun Solana program.
- **Cross-Program Invocation**: Seamless integration with other Solana programs.
- **Comprehensive Documentation**: Detailed guides and API references for all functionalities.

## Installation

Add the following to your `Cargo.toml`:

```toml
[dependencies]
mai3-pumpfun-sdk = "2.1.0"
```

## Usage

For detailed usage instructions, please refer to the documentation of each crate.

## API Reference

For detailed API documentation, run:

```
cargo doc --open
```

This will generate and open the API documentation in your default web browser.

## Contributing

We welcome contributions to the PumpFun Rust SDK! Please see our [Contributing Guide](CONTRIBUTING.md) for more details on how to get started.
