pub use reqwest;
use solana_rpc_client_api::{client_error::ErrorKind, request};
use solana_sdk::{
    signature::SignerError, transaction::TransactionError, transport::TransportError,
};
use thiserror::Error as ThisError;

use crate::jito::request::RpcRequest;

#[derive(ThisError, Debug)]
#[error("{kind}")]
pub struct Error {
    pub request: Option<RpcRequest>,

    #[source]
    pub kind: ErrorKind,
}

impl Error {
    pub fn new_with_request(kind: ErrorKind, request: RpcRequest) -> Self {
        Self {
            request: Some(request),
            kind,
        }
    }

    pub fn into_with_request(self, request: RpcRequest) -> Self {
        Self {
            request: Some(request),
            ..self
        }
    }

    pub fn request(&self) -> Option<&RpcRequest> {
        self.request.as_ref()
    }

    pub fn kind(&self) -> &ErrorKind {
        &self.kind
    }

    pub fn get_transaction_error(&self) -> Option<TransactionError> {
        self.kind.get_transaction_error()
    }
}

impl From<ErrorKind> for Error {
    fn from(kind: ErrorKind) -> Self {
        Self {
            request: None,
            kind,
        }
    }
}

impl From<TransportError> for Error {
    fn from(err: TransportError) -> Self {
        Self {
            request: None,
            kind: err.into(),
        }
    }
}

impl From<Error> for TransportError {
    fn from(client_error: Error) -> Self {
        client_error.kind.into()
    }
}

impl From<std::io::Error> for Error {
    fn from(err: std::io::Error) -> Self {
        Self {
            request: None,
            kind: err.into(),
        }
    }
}

impl From<reqwest::Error> for Error {
    fn from(err: reqwest::Error) -> Self {
        Self {
            request: None,
            kind: ErrorKind::Custom(format!("Reqwest error: {}", err)),
        }
    }
}

impl From<request::RpcError> for Error {
    fn from(err: request::RpcError) -> Self {
        Self {
            request: None,
            kind: err.into(),
        }
    }
}

impl From<serde_json::error::Error> for Error {
    fn from(err: serde_json::error::Error) -> Self {
        Self {
            request: None,
            kind: err.into(),
        }
    }
}

impl From<SignerError> for Error {
    fn from(err: SignerError) -> Self {
        Self {
            request: None,
            kind: err.into(),
        }
    }
}

impl From<TransactionError> for Error {
    fn from(err: TransactionError) -> Self {
        Self {
            request: None,
            kind: err.into(),
        }
    }
}

pub type Result<T> = std::result::Result<T, Error>;
