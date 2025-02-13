use std::env;

use anyhow::{Context, Result};
use reqwest::Proxy;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::convert::TryFrom;

#[derive(Debug)]
pub struct TipAccountResult {
    pub accounts: Vec<String>,
}

impl TipAccountResult {
    pub fn from(accounts: Vec<String>) -> Result<Self> {
        Ok(TipAccountResult { accounts })
    }
}
