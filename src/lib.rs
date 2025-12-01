//! # Rusty SSR
//!
//! High-performance Server-Side Rendering engine for Rust with V8 isolate pool
//! and multi-tier CPU-optimized caching.
//!
//! ## Features
//!
//! - **V8 Isolate Pool**: Thread pool with dedicated V8 isolates for parallel SSR
//! - **Multi-tier Cache**: L1/L2 CPU cache (hot) + RAM (cold) with LRU eviction
//! - **Axum Integration**: Ready-to-use middleware for Axum web framework
//! - **Brotli Compression**: Static and dynamic Brotli compression
//!
//! ## Quick Start
//!
//! ```rust,ignore
//! use rusty_ssr::prelude::*;
//!
//! #[tokio::main]
//! async fn main() {
//!     // Initialize the SSR engine
//!     let engine = SsrEngine::builder()
//!         .bundle_path("ssr-bundle.js")
//!         .pool_size(num_cpus::get())
//!         .cache_size(300)
//!         .build_engine()
//!         .expect("Failed to create SSR engine");
//!
//!     // Render a page
//!     let html = engine.render("/home").await.unwrap();
//!     println!("{}", html);
//! }
//! ```
//!
//! ## Architecture
//!
//! ```text
//! Request → SSR Cache (L1 hot → L2 cold) → V8 Pool → Response
//!                  ↑                            ↓
//!                  └──────── cache result ──────┘
//! ```
//!
//! ## Performance
//!
//! - **73,000+ req/s** with caching enabled
//! - **Sub-millisecond** cache hit latency (~0.2ms)
//! - **10-15x faster** than Node.js SSR solutions

#![warn(missing_docs)]
#![warn(rustdoc::missing_crate_level_docs)]

// Re-export commonly used types
pub use config::{SsrConfig, SsrConfigBuilder};
pub use engine::SsrEngine;
pub use error::{SsrError, SsrResult};

/// Configuration types and builder
pub mod config;

/// Main SSR engine
pub mod engine;

/// Error types
pub mod error;

/// V8 thread pool for parallel rendering
#[cfg(feature = "v8-pool")]
pub mod v8_pool;

/// Multi-tier caching system
#[cfg(feature = "cache")]
pub mod cache;

/// Axum middleware (brotli, etc.)
#[cfg(feature = "axum-integration")]
pub mod middleware;

/// Prelude module for convenient imports
pub mod prelude {
    //! Convenient re-exports for common usage
    //!
    //! ```rust
    //! use rusty_ssr::prelude::*;
    //! ```

    pub use crate::config::{SsrConfig, SsrConfigBuilder};
    pub use crate::engine::SsrEngine;
    pub use crate::error::{SsrError, SsrResult};

    #[cfg(feature = "cache")]
    pub use crate::cache::{SsrCache, CacheMetrics};

    #[cfg(feature = "v8-pool")]
    pub use crate::v8_pool::{V8Pool, V8PoolConfig};
}
