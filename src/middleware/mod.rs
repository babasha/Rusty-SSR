//! Axum middleware for SSR applications
//!
//! Provides compression and caching middleware for optimal performance.

#[cfg(feature = "brotli-compression")]
mod brotli;

#[cfg(feature = "brotli-compression")]
pub use brotli::{brotli_compress, brotli_static};
