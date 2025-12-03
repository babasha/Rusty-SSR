//! Multi-tier SSR cache
//!
//! Combines hot (L1/L2 CPU) and cold (RAM) caches for optimal performance.

use serde::Serialize;
use std::cell::RefCell;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;
use thread_local::ThreadLocal;

use super::cold::ColdCache;
use super::hot::HotCache;
use super::utils::hash_url;

/// Multi-tier SSR cache
///
/// ## Architecture
/// 1. **Hot cache** (L1/L2): Thread-local, 8 entries per thread
/// 2. **Cold cache** (RAM): Shared DashMap with LRU eviction
///
/// Entries found in cold cache are automatically promoted to hot cache.
pub struct SsrCache {
    hot_cache: ThreadLocal<RefCell<HotCache>>,
    cold_cache: Arc<ColdCache>,
    ttl_secs: u64,
    metrics: Arc<CacheMetricsInner>,
}

#[derive(Default)]
struct CacheMetricsInner {
    lookups: AtomicU64,
    hot_hits: AtomicU64,
    cold_hits: AtomicU64,
    misses: AtomicU64,
    promotions: AtomicU64,
    insertions: AtomicU64,
    evictions: AtomicU64,
    last_access_ns: AtomicU64,
}

/// Cache metrics snapshot
#[derive(Clone, Debug, Serialize)]
pub struct CacheMetrics {
    /// Total cache lookups
    pub lookups: u64,
    /// Hot cache hits (L1/L2)
    pub hot_hits: u64,
    /// Cold cache hits (RAM)
    pub cold_hits: u64,
    /// Cache misses
    pub misses: u64,
    /// Promotions from cold to hot
    pub promotions: u64,
    /// Total insertions
    pub insertions: u64,
    /// LRU evictions
    pub evictions: u64,
    /// Last access time in nanoseconds
    pub last_access_ns: u64,
    /// Current cold cache size
    pub cold_size: usize,
    /// Cold cache capacity
    pub cold_capacity: usize,
    /// Hit rate percentage
    pub hit_rate: f64,
}

impl SsrCache {
    /// Create a new SSR cache
    ///
    /// # Arguments
    /// * `max_cold_entries` - Maximum entries in the cold cache
    pub fn new(max_cold_entries: usize) -> Self {
        Self::with_ttl(max_cold_entries, 0)
    }

    /// Create a new SSR cache with TTL
    ///
    /// # Arguments
    /// * `max_cold_entries` - Maximum entries in the cold cache
    /// * `ttl_secs` - Time-to-live in seconds (0 = no expiration)
    pub fn with_ttl(max_cold_entries: usize, ttl_secs: u64) -> Self {
        tracing::info!(
            "📦 Creating SSR cache (size={}, ttl={}s)",
            max_cold_entries,
            if ttl_secs > 0 {
                ttl_secs.to_string()
            } else {
                "∞".to_string()
            }
        );

        Self {
            hot_cache: ThreadLocal::new(),
            cold_cache: Arc::new(ColdCache::with_ttl(max_cold_entries, ttl_secs)),
            ttl_secs,
            metrics: Arc::new(CacheMetricsInner::default()),
        }
    }

    /// Try to get cached HTML
    ///
    /// Checks hot cache first, then cold cache.
    /// Cold hits are promoted to hot cache.
    pub fn try_get(&self, url: &str) -> Option<Arc<str>> {
        let url_hash = hash_url(url);
        let start = Instant::now();
        self.metrics.lookups.fetch_add(1, Ordering::Relaxed);

        // 1. Check hot cache (L1/L2) - use peek() for read-only access
        let hot = self
            .hot_cache
            .get_or(|| RefCell::new(HotCache::with_ttl(self.ttl_secs)));

        if let Some(html) = hot.borrow().peek(url_hash) {
            self.metrics.hot_hits.fetch_add(1, Ordering::Relaxed);
            self.metrics
                .last_access_ns
                .store(start.elapsed().as_nanos() as u64, Ordering::Relaxed);
            return Some(html);
        }

        // 2. Check cold cache (RAM)
        if let Some(html) = self.cold_cache.get(url_hash) {
            self.metrics.cold_hits.fetch_add(1, Ordering::Relaxed);

            // Promote to hot cache
            let mut hot_ref = hot.borrow_mut();
            hot_ref.insert(url_hash, Arc::clone(&html));
            self.metrics.promotions.fetch_add(1, Ordering::Relaxed);

            self.metrics
                .last_access_ns
                .store(start.elapsed().as_nanos() as u64, Ordering::Relaxed);
            return Some(html);
        }

        self.metrics.misses.fetch_add(1, Ordering::Relaxed);
        None
    }

    /// Insert HTML into cache
    pub fn insert(&self, url: &str, html: Arc<str>) {
        let url_hash = hash_url(url);

        // Insert into cold cache
        let evicted = self.cold_cache.insert(url_hash, Arc::clone(&html));
        self.metrics.insertions.fetch_add(1, Ordering::Relaxed);
        if evicted.is_some() {
            self.metrics.evictions.fetch_add(1, Ordering::Relaxed);
        }

        // Insert into hot cache
        let hot = self
            .hot_cache
            .get_or(|| RefCell::new(HotCache::with_ttl(self.ttl_secs)));
        let mut hot_ref = hot.borrow_mut();
        hot_ref.insert(url_hash, html);
    }

    /// Clear the cache
    pub fn clear(&self) {
        self.cold_cache.clear();
    }

    /// Get current cold cache size
    pub fn size(&self) -> usize {
        self.cold_cache.len()
    }

    /// Get cache metrics
    pub fn metrics(&self) -> CacheMetrics {
        let lookups = self.metrics.lookups.load(Ordering::Relaxed);
        let hot_hits = self.metrics.hot_hits.load(Ordering::Relaxed);
        let cold_hits = self.metrics.cold_hits.load(Ordering::Relaxed);
        let total_hits = hot_hits + cold_hits;

        CacheMetrics {
            lookups,
            hot_hits,
            cold_hits,
            misses: self.metrics.misses.load(Ordering::Relaxed),
            promotions: self.metrics.promotions.load(Ordering::Relaxed),
            insertions: self.metrics.insertions.load(Ordering::Relaxed),
            evictions: self.metrics.evictions.load(Ordering::Relaxed),
            last_access_ns: self.metrics.last_access_ns.load(Ordering::Relaxed),
            cold_size: self.cold_cache.len(),
            cold_capacity: self.cold_cache.capacity(),
            hit_rate: if lookups > 0 {
                (total_hits as f64 / lookups as f64) * 100.0
            } else {
                0.0
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_caching() {
        let cache = SsrCache::new(100);

        cache.insert("/test", Arc::from("html"));
        assert!(cache.try_get("/test").is_some());
        assert!(cache.try_get("/other").is_none());
    }

    #[test]
    fn test_metrics() {
        let cache = SsrCache::new(100);

        cache.insert("/test", Arc::from("html"));
        let _ = cache.try_get("/test");
        let _ = cache.try_get("/missing");

        let metrics = cache.metrics();
        assert_eq!(metrics.insertions, 1);
        assert_eq!(metrics.lookups, 2);
        assert_eq!(metrics.misses, 1);
    }
}
