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
//! Two numbers decide what this can serve, and they differ by fifty times, so
//! quoting one without the other says nothing. Measured on an 8-core Ryzen 7
//! 260 against a real application — a 2.5 MB Preact bundle producing 200–320 kB
//! pages, with database queries behind them:
//!
//! | | pages/s | p50 |
//! |---|---|---|
//! | Served from the page cache | ~48,000 | 1.0 ms |
//! | **Actually rendered** | **~950** | 17 ms |
//!
//! **The rendered figure is the one that sizes a deployment.** A pool serves at
//! most `pool_size / render_time` pages per second — here 16 workers at 16.8 ms
//! each — and throughput stops rising the moment every worker is busy. Past
//! that, added concurrency becomes queue delay and nothing else: at 128
//! concurrent callers throughput was 9% higher than at 16, and p50 was seven
//! times worse.
//!
//! So capacity is `requests/s ≤ 950 / (1 - hit_rate)`: about 9,500 req/s at a
//! 90% hit rate, 95,000 at 99%. Watch [`SsrEngine::pool_metrics`] rather than
//! guessing — `saturation` at 100% with a filling queue is the shape of a pool
//! that needs more workers or fewer callers.
//!
//! ## What it is faster than, and what it is not
//!
//! Rendering is V8 executing your bundle's bytecode, and that is the same V8
//! Node embeds. Per render thread, with the identical bundle, this engine
//! measured **1.1× Node** — which is to say parity, and there is no version of
//! this that renders JavaScript faster than Node does.
//!
//! What it does instead is run `pool_size` isolates **inside one process**,
//! sharing one cache and one copy of the bundle. Equalling ~19,000 renders/s
//! with Node means running about nine processes, each with its own heap, its
//! own compiled bundle and its own fragmented cache. That is the trade this
//! crate exists for: the same rendering speed, at roughly a tenth of the memory,
//! with a cache all the workers share and per-request isolation Node has no
//! answer for. It is a memory and isolation story, not a speed story.
//!
//! See `BENCHMARK.md` for the full method, the raw figures and their caveats.

#![warn(missing_docs)]
#![warn(rustdoc::missing_crate_level_docs)]

// Re-export commonly used types
pub use config::{SsrConfig, SsrConfigBuilder};
pub use engine::SsrEngine;
pub use error::{SsrError, SsrResult};

/// Configuration types and builder
pub mod assets;

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

pub mod prelude {
    //! Prelude module for convenient imports
    //!
    //! Convenient re-exports for common usage
    //!
    //! ```rust
    //! use rusty_ssr::prelude::*;
    //! ```

    pub use crate::config::{SsrConfig, SsrConfigBuilder};
    pub use crate::engine::SsrEngine;
    pub use crate::error::{SsrError, SsrResult};

    #[cfg(feature = "cache")]
    pub use crate::cache::{
        CacheMetrics, CachePolicy, CachedPage, PageCache, RenderKey, SsrCache,
    };

    #[cfg(feature = "v8-pool")]
    pub use crate::v8_pool::{Rendered, V8Pool, V8PoolConfig};

    pub use crate::assets::ViteManifest;
}
