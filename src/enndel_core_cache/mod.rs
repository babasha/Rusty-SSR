// SSR Cache - CPU cache-optimized кэширование отрендеренного HTML
// Использует multi-tier стратегию: L1/L2 (hot) → RAM (cold)

pub mod cache_utils;
mod cold_cache;
mod hot_cache;
pub mod product_cache;
mod ssr_cache;

pub use product_cache::{ProductCache, ProductCacheMetricsSnapshot};
pub use ssr_cache::{SSRCache, SSRCacheMetricsSnapshot};
