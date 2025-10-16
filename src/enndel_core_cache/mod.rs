// SSR Cache - CPU cache-optimized кэширование отрендеренного HTML
// Использует multi-tier стратегию: L1/L2 (hot) → RAM (cold)

mod hot_cache;
mod cold_cache;
mod ssr_cache;
pub mod product_cache;
pub mod cache_utils;

pub use ssr_cache::SSRCache;
pub use product_cache::ProductCache;
