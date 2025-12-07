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
    hot_cache: ThreadLocal<RefCell<HotCacheState>>,
    cold_cache: Arc<ColdCache>,
    ttl_secs: u64,
    generation: AtomicU64,
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

struct HotCacheState {
    generation: u64,
    cache: HotCache,
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
            generation: AtomicU64::new(0),
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
        let hot = self.get_or_init_hot_cache();
        if let Some(html) = hot.borrow().cache.peek(url_hash) {
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
            hot_ref.cache.insert(url_hash, Arc::clone(&html));
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
        let hot = self.get_or_init_hot_cache();
        let mut hot_ref = hot.borrow_mut();
        hot_ref.cache.insert(url_hash, html);
    }

    /// Clear the cache, including hot caches and metrics
    pub fn clear(&self) {
        self.cold_cache.clear();
        self.generation.fetch_add(1, Ordering::Relaxed);
        self.reset_metrics();
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

    fn reset_metrics(&self) {
        self.metrics.lookups.store(0, Ordering::Relaxed);
        self.metrics.hot_hits.store(0, Ordering::Relaxed);
        self.metrics.cold_hits.store(0, Ordering::Relaxed);
        self.metrics.misses.store(0, Ordering::Relaxed);
        self.metrics.promotions.store(0, Ordering::Relaxed);
        self.metrics.insertions.store(0, Ordering::Relaxed);
        self.metrics.evictions.store(0, Ordering::Relaxed);
        self.metrics
            .last_access_ns
            .store(0, Ordering::Relaxed);
    }

    fn get_or_init_hot_cache(&self) -> &RefCell<HotCacheState> {
        let generation = self.generation.load(Ordering::Relaxed);
        let hot = self.hot_cache.get_or(|| {
            RefCell::new(HotCacheState {
                generation,
                cache: HotCache::with_ttl(self.ttl_secs),
            })
        });

        {
            let mut state = hot.borrow_mut();
            if state.generation != generation {
                state.cache.clear();
                state.generation = generation;
            }
        }

        hot
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

    #[test]
    fn test_clear_removes_hot_and_resets_metrics() {
        let cache = SsrCache::with_ttl(16, 10);

        cache.insert("/hot", Arc::from("html"));
        assert!(cache.try_get("/hot").is_some(), "hot cache should have entry");

        cache.clear();

        // After clearing, both caches should miss and metrics reset
        assert!(cache.try_get("/hot").is_none(), "hot cache should be cleared");
        let metrics = cache.metrics();
        assert_eq!(metrics.insertions, 0);
        assert_eq!(metrics.lookups, 1);
        assert_eq!(metrics.misses, 1);
    }
}
