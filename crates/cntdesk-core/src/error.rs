//! Core error types for CNT Desk

use std::fmt;

/// Core error enum for protocol, crypto, and serialization errors
#[derive(Debug)]
pub enum CoreError {
    Serialization(String),
    Crypto(String),
    Protocol(String),
}

impl fmt::Display for CoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CoreError::Serialization(msg) => write!(f, "Serialization error: {}", msg),
            CoreError::Crypto(msg) => write!(f, "Crypto error: {}", msg),
            CoreError::Protocol(msg) => write!(f, "Protocol error: {}", msg),
        }
    }
}

impl std::error::Error for CoreError {}

impl From<bincode::Error> for CoreError {
    fn from(err: bincode::Error) -> Self {
        CoreError::Serialization(err.to_string())
    }
}

impl From<ed25519_dalek::SignatureError> for CoreError {
    fn from(err: ed25519_dalek::SignatureError) -> Self {
        CoreError::Crypto(err.to_string())
    }
}