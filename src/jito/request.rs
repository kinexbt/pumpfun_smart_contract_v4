use std::fmt;

use serde_json::{json, Value};

#[derive(Debug, PartialEq, Eq, Hash, Clone, Copy)]
pub enum RpcRequest {
    Custom { method: &'static str },
    GetBundlesStatuses,
    GetTipAccounts,
    SendBundle,
}

impl fmt::Display for RpcRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let method = match self {
            RpcRequest::Custom { method } => method,
            RpcRequest::GetBundlesStatuses => "getBundleStatuses",
            RpcRequest::GetTipAccounts => "getTipAccounts",
            RpcRequest::SendBundle => "sendBundle",
        };

        write!(f, "{method}")
    }
}

impl RpcRequest {
    pub fn build_request_json(self, id: u64, params: Value) -> Value {
        let jsonrpc = "2.0";
        json!({
           "jsonrpc": jsonrpc,
           "id": id,
           "method": format!("{self}"),
           "params": params,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_request_json() {
        let test_request = RpcRequest::GetTipAccounts;
        let request = test_request.build_request_json(1, json!([]));
        assert_eq!(request["method"], "getTipAccounts");
        assert_eq!(request["params"], json!([]));

        let test_request = RpcRequest::GetBundlesStatuses;
        let addr = json!("deadbeefXjn8o3yroDHxUtKsZZgoy4GPkPPXfouKNHhx");
        let request = test_request.build_request_json(1, json!([addr]));
        assert_eq!(request["method"], "getBundleStatuses");
        assert_eq!(request["params"], json!([addr]));
    }
}
