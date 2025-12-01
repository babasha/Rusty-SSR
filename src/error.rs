//! Error types for Rusty SSR

use std::fmt;

/// Result type alias for SSR operations
pub type SsrResult<T> = Result<T, SsrError>;

/// Errors that can occur during SSR operations
#[derive(Debug)]
pub enum SsrError {
    /// Failed to load the JavaScript bundle
    BundleLoad(String),

    /// V8 runtime initialization failed
    V8Init(String),

    /// JavaScript execution error
    JsExecution(String),

    /// Render timeout
    Timeout,

    /// Cache error
    Cache(String),

    /// Pool is full, request was rejected
    PoolFull,

    /// Configuration error
    Config(String),

    /// IO error
    Io(std::io::Error),
}

impl fmt::Display for SsrError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SsrError::BundleLoad(msg) => write!(f, "Bundle load error: {}", msg),
            SsrError::V8Init(msg) => write!(f, "V8 initialization error: {}", msg),
            SsrError::JsExecution(msg) => write!(f, "JavaScript execution error: {}", msg),
            SsrError::Timeout => write!(f, "Render timeout"),
            SsrError::Cache(msg) => write!(f, "Cache error: {}", msg),
            SsrError::PoolFull => write!(f, "V8 pool is full, request rejected"),
            SsrError::Config(msg) => write!(f, "Configuration error: {}", msg),
            SsrError::Io(err) => write!(f, "IO error: {}", err),
        }
    }
}

impl std::error::Error for SsrError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            SsrError::Io(err) => Some(err),
            _ => None,
        }
    }
}

impl From<std::io::Error> for SsrError {
    fn from(err: std::io::Error) -> Self {
        SsrError::Io(err)
    }
}

impl From<String> for SsrError {
    fn from(msg: String) -> Self {
        SsrError::JsExecution(msg)
    }
}
