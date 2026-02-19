//! Multi-tier caching system optimized for CPU cache efficiency
//!
//! ## Architecture
//!
//! ```text
//! Request ──► Hot Cache (L1/L2 CPU) ──► Cold Cache (RAM) ──► Miss
//!              │ ~1-3ns latency        │ ~100ns latency
//!              │ 8 entries/thread      │ N entries shared
//!              └───────────────────────┘
//! ```
//!
//! - **Hot Cache**: Thread-local, fits in L1/L2 CPU cache (~4KB per thread)
//! - **Cold Cache**: Shared RAM cache with DashMap for lock-free access
//! - **Auto-promotion**: Cold hits are promoted to hot cache

mod cold;
pub mod hot;  // Public for benchmarking
mod padded;
mod ssr;
mod utils;

pub use ssr::{SsrCache, CacheMetrics};
pub use hot::HotCache;
