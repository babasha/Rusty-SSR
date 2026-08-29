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
use super::padded::CachePadded;
use super::utils::hash_url;

/// One lookup in this many is timed for [`CacheMetrics::last_access_ns`].
///
/// Reading the monotonic clock costs more than the lookup it would be timing —
/// tens of nanoseconds against the single-digit nanoseconds a hot hit is
/// supposed to take — so timing every lookup made the measurement the most
/// expensive thing on the hit path, and on a miss the reading was taken and
/// then thrown away. Sampling keeps the metric (it is a latency gauge, not a
/// counter, so a sample is exactly as informative as the whole population)
/// while taking the clock out of the other 255 lookups.
const LATENCY_SAMPLE_EVERY: u64 = 256;

/// The counters behind [`CacheMetrics`].
///
/// Generated so the list exists once. It used to be written out three times —
/// the fields, the snapshot, and the reset — and a counter missing from the
/// reset is invisible until somebody clears a cache and the numbers do not go
/// to zero.
macro_rules! cache_counters {
    ($($field:ident),+ $(,)?) => {
        #[derive(Default)]
        struct CacheMetricsInner {
            $($field: CachePadded<AtomicU64>,)+
        }

        impl CacheMetricsInner {
            /// Back to zero, every counter, no exceptions.
            fn reset(&self) {
                $(self.$field.store(0, Ordering::Relaxed);)+
            }
        }
    };
}

cache_counters!(
    lookups,
    hot_hits,
    cold_hits,
    misses,
    promotions,
    insertions,
    evictions,
    last_access_ns,
);

/// Multi-tier SSR cache
///
/// ## Architecture
/// 1. **Hot cache** (L1/L2): Thread-local, 8 entries per thread
/// 2. **Cold cache** (RAM): Shared DashMap with LRU eviction
///
/// Entries found in cold cache are automatically promoted to hot cache.
pub struct SsrCache {
    hot_cache: ThreadLocal<RefCell<HotCacheState>>,
    // Held directly rather than behind an `Arc`. Nothing ever cloned either
    // handle, so the indirection bought nothing and cost a pointer chase on
    // every single lookup — on the hit path, which is the path that matters.
    cold_cache: ColdCache,
    ttl_secs: u64,
    generation: AtomicU64,
    metrics: CacheMetricsInner,
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
    /// How long the last *sampled* lookup took, in nanoseconds.
    ///
    /// A gauge, not a total: one lookup in 256 is timed, and this holds the
    /// most recent of those readings. See the note on sampling in this
    /// module — timing every lookup cost more than the lookups did.
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
            cold_cache: ColdCache::with_ttl(max_cold_entries, ttl_secs),
            ttl_secs,
            generation: AtomicU64::new(0),
            metrics: CacheMetricsInner::default(),
        }
    }

    /// Try to get cached HTML
    ///
    /// Checks hot cache first, then cold cache.
    /// Cold hits are promoted to hot cache.
    pub fn try_get(&self, key: &str) -> Option<Arc<str>> {
        let key_hash = hash_url(key);
        // `fetch_add` hands back the previous value, so the counter doubles as
        // the sampling clock and no second atomic is needed to decide.
        let n = self.metrics.lookups.fetch_add(1, Ordering::Relaxed);
        let start = n.is_multiple_of(LATENCY_SAMPLE_EVERY).then(Instant::now);

        // 1. Check hot cache (L1/L2) - use peek() for read-only access
        let hot = self.get_or_init_hot_cache();
        if let Some(html) = hot.borrow().cache.peek(key_hash, key) {
            self.metrics.hot_hits.fetch_add(1, Ordering::Relaxed);
            self.record_latency(start);
            return Some(html);
        }

        // 2. Check cold cache (RAM)
        if let Some(html) = self.cold_cache.get(key_hash, key) {
            self.metrics.cold_hits.fetch_add(1, Ordering::Relaxed);

            // Promote to hot cache
            let mut hot_ref = hot.borrow_mut();
            hot_ref.cache.insert(key_hash, Arc::from(key), Arc::clone(&html));
            self.metrics.promotions.fetch_add(1, Ordering::Relaxed);

            self.record_latency(start);
            return Some(html);
        }

        self.metrics.misses.fetch_add(1, Ordering::Relaxed);
        None
    }

    /// Record how long a sampled lookup took. A no-op for the lookups that
    /// were not sampled, which is all but one in [`LATENCY_SAMPLE_EVERY`].
    #[inline(always)]
    fn record_latency(&self, start: Option<Instant>) {
        if let Some(start) = start {
            self.metrics
                .last_access_ns
                .store(start.elapsed().as_nanos() as u64, Ordering::Relaxed);
        }
    }

    /// Insert HTML into cache
    pub fn insert(&self, key: &str, html: Arc<str>) {
        let key_hash = hash_url(key);
        // Allocate the key once and share it between the cold and hot tiers.
        let key_arc: Arc<str> = Arc::from(key);

        // Insert into cold cache
        let evicted = self
            .cold_cache
            .insert(key_hash, Arc::clone(&key_arc), Arc::clone(&html));
        self.metrics.insertions.fetch_add(1, Ordering::Relaxed);
        if evicted > 0 {
            self.metrics.evictions.fetch_add(evicted as u64, Ordering::Relaxed);
        }

        // Insert into hot cache
        let hot = self.get_or_init_hot_cache();
        let mut hot_ref = hot.borrow_mut();
        hot_ref.cache.insert(key_hash, key_arc, html);
    }

    /// Invalidate a single cached URL
    ///
    /// Removes from cold cache and bumps generation to clear hot caches.
    /// Other hot-cached entries will be re-promoted from cold on next access.
    pub fn invalidate(&self, key: &str) {
        let key_hash = hash_url(key);
        if self.cold_cache.remove(key_hash, key) {
            self.generation.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Invalidate all cached URLs that start with the given prefix
    ///
    /// Example: `cache.invalidate_prefix("/products")` removes
    /// `/products`, `/products/123`, `/products/foo/bar`, etc.
    ///
    /// Also bumps the generation counter to clear all hot caches,
    /// ensuring stale entries don't survive in thread-local caches.
    pub fn invalidate_prefix(&self, prefix: &str) -> usize {
        let removed = self.cold_cache.remove_by_prefix(prefix);
        if removed > 0 {
            // Bump generation to invalidate hot caches that may hold stale entries
            self.generation.fetch_add(1, Ordering::Relaxed);
        }
        removed
    }

    /// Clear the cache, including hot caches and metrics
    pub fn clear(&self) {
        self.cold_cache.clear();
        self.generation.fetch_add(1, Ordering::Relaxed);
        self.metrics.reset();
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

    fn get_or_init_hot_cache(&self) -> &RefCell<HotCacheState> {
        let generation = self.generation.load(Ordering::Relaxed);
        let hot = self.hot_cache.get_or(|| {
            RefCell::new(HotCacheState {
                generation,
                cache: HotCache::with_ttl(self.ttl_secs),
            })
        });

        // Fast path: a shared borrow is enough to see the generation is current,
        // and it is current for every lookup that is not racing an
        // invalidation — which is essentially all of them. Taking the exclusive
        // borrow unconditionally meant every hit paid for a write to the
        // `RefCell` flag, and then the caller immediately took a second borrow.
        if hot.borrow().generation != generation {
            let mut state = hot.borrow_mut();
            // Re-check: another borrow could have caught up in between.
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
    fn test_invalidate_single() {
        let cache = SsrCache::new(100);

        cache.insert("/a", Arc::from("html_a"));
        cache.insert("/b", Arc::from("html_b"));

        cache.invalidate("/a");

        assert!(cache.try_get("/a").is_none());
        assert!(cache.try_get("/b").is_some());
    }

    #[test]
    fn test_invalidate_prefix() {
        let cache = SsrCache::new(100);

        cache.insert("/products/1", Arc::from("p1"));
        cache.insert("/products/2", Arc::from("p2"));
        cache.insert("/about", Arc::from("about"));

        let removed = cache.invalidate_prefix("/products");
        assert_eq!(removed, 2);

        assert!(cache.try_get("/products/1").is_none());
        assert!(cache.try_get("/products/2").is_none());
        assert!(cache.try_get("/about").is_some());
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
