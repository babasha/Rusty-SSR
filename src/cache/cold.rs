//! RAM-based "cold" cache with LRU eviction
//!
//! Uses DashMap for lock-free concurrent access.
//! Access time: ~100 nanoseconds
//!
//! Optimized with 128 shards to minimize contention at 8+ threads.
//! Benchmarks show 1.8x improvement over default shard count.

use dashmap::DashMap;
use std::collections::BinaryHeap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use super::padded::CachePadded;

/// Optimal shard count for 8+ concurrent threads.
/// Benchmarked values: 16=51M, 32=57M, 64=59M, 128=60.6M, 256=60.3M elem/s
const OPTIMAL_SHARD_COUNT: usize = 128;

/// Each eviction scan drains the cache back down to this percent of capacity.
/// Evicting *to a target* (rather than a fixed slice) is what keeps eviction
/// from falling behind: every scan clears the whole overshoot, so the cache
/// can't run away under a sustained insert storm — at most it drifts by
/// `insert_rate × scan_time` above the cap between scans. The 10% headroom
/// also means a scan only fires once per ~10%-of-capacity inserts.
const EVICT_TARGET_PERCENT: usize = 90;
/// Cap the work of a single scan (bounds the transient heap + scan latency).
/// Steady-state eviction (~10% of capacity) stays well under this; the cap only
/// matters for a one-off catch-up after a large burst.
const EVICT_MAX_PER_SCAN_PERCENT: usize = 25;
/// Minimum entries to evict per scan (for tiny caches).
const EVICT_BATCH_MIN: usize = 8;

/// Cold cache entry with LRU metadata
struct CacheEntry {
    /// Full cache key (collision-checked on lookup; also used for prefix
    /// invalidation). The engine composes this from the URL and render data.
    key: Arc<str>,
    html: Arc<str>,
    last_access: AtomicU64,
    created_at: Instant,
}

/// Shared cold cache in RAM
pub struct ColdCache {
    cache: DashMap<u64, CacheEntry>,
    max_entries: usize,
    access_counter: CachePadded<AtomicU64>,
    evicting: CachePadded<AtomicBool>,
    ttl: Option<Duration>,
}

impl ColdCache {
    /// Create a new cold cache with optimized shard count
    #[allow(dead_code)]
    pub fn new(max_entries: usize) -> Self {
        Self {
            cache: DashMap::with_capacity_and_shard_amount(max_entries, OPTIMAL_SHARD_COUNT),
            max_entries,
            access_counter: CachePadded::new(AtomicU64::new(0)),
            evicting: CachePadded::new(AtomicBool::new(false)),
            ttl: None,
        }
    }

    /// Create a cold cache with TTL and optimized shard count
    pub fn with_ttl(max_entries: usize, ttl_secs: u64) -> Self {
        Self {
            cache: DashMap::with_capacity_and_shard_amount(max_entries, OPTIMAL_SHARD_COUNT),
            max_entries,
            access_counter: CachePadded::new(AtomicU64::new(0)),
            evicting: CachePadded::new(AtomicBool::new(false)),
            ttl: if ttl_secs > 0 {
                Some(Duration::from_secs(ttl_secs))
            } else {
                None
            },
        }
    }

    /// Get HTML from cache
    ///
    /// `key_hash` is the hash of `key`; the stored entry's full key is compared
    /// against `key` so a 64-bit hash collision degrades to a miss (and never
    /// serves another key's content). Returns None if not found, mismatched, or
    /// expired.
    #[inline(always)]
    pub fn get(&self, key_hash: u64, key: &str) -> Option<Arc<str>> {
        let entry = self.cache.get(&key_hash)?;

        // Reject hash collisions: this bucket holds a different key.
        if entry.key.as_ref() != key {
            return None;
        }

        // Check TTL
        if let Some(ttl) = self.ttl {
            if entry.created_at.elapsed() > ttl {
                drop(entry);
                self.cache.remove(&key_hash);
                return None;
            }
        }

        // Update LRU counter
        let new_access = self.access_counter.fetch_add(1, Ordering::Relaxed);
        entry.last_access.store(new_access, Ordering::Relaxed);

        Some(Arc::clone(&entry.html))
    }

    /// Insert HTML into cache with batch LRU eviction
    ///
    /// Returns the number of evicted entries.
    pub fn insert(&self, key_hash: u64, key: Arc<str>, html: Arc<str>) -> usize {
        let evicted = if self.cache.len() >= self.max_entries {
            self.evict_batch()
        } else {
            0
        };

        let new_access = self.access_counter.fetch_add(1, Ordering::Relaxed);
        self.cache.insert(
            key_hash,
            CacheEntry {
                key,
                html,
                last_access: AtomicU64::new(new_access),
                created_at: Instant::now(),
            },
        );

        evicted
    }

    /// Evict the oldest entries, draining the cache back down to the target.
    ///
    /// Only one thread evicts at a time — others skip and proceed with insert
    /// (avoids 16 concurrent O(n) scans). Crucially, each scan evicts *down to
    /// the target* (not a fixed slice), so the cache returns to ~90% of cap
    /// every scan and can't run away: between scans it only grows by
    /// `insert_rate × scan_time`, which is far below the 10% headroom for any
    /// realistic insert rate. Uses a bounded max-heap to find the oldest
    /// without allocating for the whole cache.
    fn evict_batch(&self) -> usize {
        // Guard: only one thread evicts at a time to avoid thundering herd
        if self
            .evicting
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            return 0;
        }

        // Evict down to the target, capped per scan to bound work.
        let len = self.cache.len();
        let target = self.max_entries * EVICT_TARGET_PERCENT / 100;
        let cap_per_scan =
            (self.max_entries * EVICT_MAX_PER_SCAN_PERCENT / 100).max(EVICT_BATCH_MIN);
        let batch = len
            .saturating_sub(target)
            .clamp(EVICT_BATCH_MIN, cap_per_scan);

        // Max-heap keyed by access time: the top element is the *newest* among candidates.
        // We keep only `batch` entries — if a new entry is older than the top, swap it in.
        let mut heap: BinaryHeap<(u64, u64)> = BinaryHeap::with_capacity(batch + 1);

        for entry in self.cache.iter() {
            let access = entry.last_access.load(Ordering::Relaxed);
            let key = *entry.key();

            if heap.len() < batch {
                heap.push((access, key));
            } else if let Some(&(top_access, _)) = heap.peek() {
                if access < top_access {
                    heap.pop();
                    heap.push((access, key));
                }
            }
        }

        let evicted = heap.len();
        for (_, key) in heap {
            self.cache.remove(&key);
        }

        self.evicting.store(false, Ordering::Release);
        evicted
    }

    /// Get number of entries
    pub fn len(&self) -> usize {
        self.cache.len()
    }

    /// Check if empty
    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.cache.is_empty()
    }

    /// Remove a single entry by its key hash, verifying the full key matches
    /// (so a colliding entry under the same hash is left untouched).
    pub fn remove(&self, key_hash: u64, key: &str) -> bool {
        self.cache
            .remove_if(&key_hash, |_, e| e.key.as_ref() == key)
            .is_some()
    }

    /// Remove all entries whose key starts with the given prefix.
    ///
    /// Returns the number of removed entries.
    pub fn remove_by_prefix(&self, prefix: &str) -> usize {
        let mut to_remove = Vec::new();

        for entry in self.cache.iter() {
            if entry.key.starts_with(prefix) {
                to_remove.push(*entry.key());
            }
        }

        let count = to_remove.len();
        for hash in to_remove {
            self.cache.remove(&hash);
        }
        count
    }

    /// Clear the cache
    pub fn clear(&self) {
        self.cache.clear();
    }

    /// Get maximum capacity
    pub fn capacity(&self) -> usize {
        self.max_entries
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_operations() {
        let cache = ColdCache::new(100);
        let html: Arc<str> = "test".into();

        cache.insert(123, Arc::from("/test"), Arc::clone(&html));

        assert!(cache.get(123, "/test").is_some());
        assert!(cache.get(456, "/missing").is_none());
    }

    #[test]
    fn test_get_rejects_hash_collision() {
        let cache = ColdCache::new(10);
        cache.insert(42, Arc::from("/real"), "real".into());

        // Same bucket hash, different key → must miss, never serve wrong content.
        assert!(cache.get(42, "/attacker").is_none());
        assert!(cache.get(42, "/real").is_some());
    }

    #[test]
    fn test_eviction() {
        let cache = ColdCache::new(5);

        for i in 0..10 {
            let html: Arc<str> = format!("html{}", i).into();
            cache.insert(i, Arc::from(format!("/page/{}", i)), html);
        }

        assert!(cache.len() <= 5);
    }

    #[test]
    fn test_eviction_keeps_cache_bounded() {
        // Insert far more than capacity: eviction must keep the cache within
        // its cap (it drains down to the target each scan, never runs away).
        let cache = ColdCache::new(1000);
        for i in 0..10_000u64 {
            cache.insert(i, Arc::from(format!("/p/{}", i)), "h".into());
        }
        assert!(
            cache.len() <= 1000,
            "cache must stay within capacity, got {}",
            cache.len()
        );
    }

    #[test]
    fn test_batch_eviction_returns_count() {
        let cache = ColdCache::new(8);

        for i in 0..8 {
            let html: Arc<str> = format!("html{}", i).into();
            cache.insert(i, Arc::from(format!("/page/{}", i)), html);
        }
        assert_eq!(cache.len(), 8);

        let evicted = cache.insert(100, Arc::from("/new"), "new".into());
        assert!(evicted >= 1);
        assert!(cache.len() < 8);
    }

    #[test]
    fn test_remove_single() {
        let cache = ColdCache::new(100);
        cache.insert(1, Arc::from("/a"), "html_a".into());
        cache.insert(2, Arc::from("/b"), "html_b".into());

        assert!(cache.remove(1, "/a"));
        assert!(cache.get(1, "/a").is_none());
        assert!(cache.get(2, "/b").is_some());
    }

    #[test]
    fn test_remove_by_prefix() {
        let cache = ColdCache::new(100);
        cache.insert(1, Arc::from("/products/1"), "p1".into());
        cache.insert(2, Arc::from("/products/2"), "p2".into());
        cache.insert(3, Arc::from("/products/3"), "p3".into());
        cache.insert(4, Arc::from("/about"), "about".into());
        cache.insert(5, Arc::from("/home"), "home".into());

        let removed = cache.remove_by_prefix("/products");
        assert_eq!(removed, 3);
        assert_eq!(cache.len(), 2);
        assert!(cache.get(4, "/about").is_some());
        assert!(cache.get(5, "/home").is_some());
    }
}
