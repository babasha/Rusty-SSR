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
//!
//! All three describe the **fragment** cache ([`SsrCache`](crate::cache::SsrCache)): keyed on the
//! render, storing what the render returned. Above it sits [`PageCache`](crate::cache::PageCache) — the
//! **finished-document** cache, keyed on whatever the caller says decides the
//! page ([`RenderKey`](crate::cache::RenderKey)), storing status and bytes, with single-flight and
//! stale-while-revalidate. An application serving HTTP almost always wants the
//! second one; see [`page`](crate::cache::page) for the three reasons why.

mod cold;
pub mod hot;  // Public for benchmarking
mod padded;
pub mod page;
mod ssr;
mod utils;

pub use hot::HotCache;
pub use page::{BuiltPage, CachePolicy, CachedPage, PageCache, RenderKey};
pub use ssr::{SsrCache, CacheMetrics};
