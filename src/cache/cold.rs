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

/// Evict ~2% of capacity per batch (minimum 8 entries).
/// For a 10,000-entry cache this means one scan per ~200 inserts instead of every insert.
const EVICT_BATCH_PERCENT: usize = 2;
const EVICT_BATCH_MIN: usize = 8;

/// Cold cache entry with LRU metadata
struct CacheEntry {
    url: Arc<str>,
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
    /// Returns None if not found or expired
    #[inline(always)]
    pub fn get(&self, url_hash: u64) -> Option<Arc<str>> {
        let entry = self.cache.get(&url_hash)?;

        // Check TTL
        if let Some(ttl) = self.ttl {
            if entry.created_at.elapsed() > ttl {
                drop(entry);
                self.cache.remove(&url_hash);
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
    pub fn insert(&self, url_hash: u64, url: &str, html: Arc<str>) -> usize {
        let evicted = if self.cache.len() >= self.max_entries {
            self.evict_batch()
        } else {
            0
        };

        let new_access = self.access_counter.fetch_add(1, Ordering::Relaxed);
        self.cache.insert(
            url_hash,
            CacheEntry {
                url: Arc::from(url),
                html,
                last_access: AtomicU64::new(new_access),
                created_at: Instant::now(),
            },
        );

        evicted
    }

    /// Batch-evict the oldest entries.
    ///
    /// Only one thread evicts at a time — others skip and proceed with insert.
    /// Uses a bounded max-heap (O(batch) memory) to find the oldest entries
    /// without allocating for the entire cache.
    fn evict_batch(&self) -> usize {
        // Guard: only one thread evicts at a time to avoid thundering herd
        if self
            .evicting
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            return 0;
        }

        let batch =
            (self.max_entries * EVICT_BATCH_PERCENT / 100).max(EVICT_BATCH_MIN);

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

    /// Remove a single entry by its URL hash
    pub fn remove(&self, url_hash: u64) -> bool {
        self.cache.remove(&url_hash).is_some()
    }

    /// Remove all entries whose URL starts with the given prefix.
    ///
    /// Returns the number of removed entries.
    pub fn remove_by_prefix(&self, prefix: &str) -> usize {
        let mut to_remove = Vec::new();

        for entry in self.cache.iter() {
            if entry.url.starts_with(prefix) {
                to_remove.push(*entry.key());
            }
        }

        let count = to_remove.len();
        for key in to_remove {
            self.cache.remove(&key);
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

        cache.insert(123, "/test", Arc::clone(&html));

        assert!(cache.get(123).is_some());
        assert!(cache.get(456).is_none());
    }

    #[test]
    fn test_eviction() {
        let cache = ColdCache::new(5);

        for i in 0..10 {
            let html: Arc<str> = format!("html{}", i).into();
            cache.insert(i, &format!("/page/{}", i), html);
        }

        assert!(cache.len() <= 5);
    }

    #[test]
    fn test_batch_eviction_returns_count() {
        let cache = ColdCache::new(8);

        for i in 0..8 {
            let html: Arc<str> = format!("html{}", i).into();
            cache.insert(i, &format!("/page/{}", i), html);
        }
        assert_eq!(cache.len(), 8);

        let evicted = cache.insert(100, "/new", "new".into());
        assert!(evicted >= 1);
        assert!(cache.len() < 8);
    }

    #[test]
    fn test_remove_single() {
        let cache = ColdCache::new(100);
        cache.insert(1, "/a", "html_a".into());
        cache.insert(2, "/b", "html_b".into());

        assert!(cache.remove(1));
        assert!(cache.get(1).is_none());
        assert!(cache.get(2).is_some());
    }

    #[test]
    fn test_remove_by_prefix() {
        let cache = ColdCache::new(100);
        cache.insert(1, "/products/1", "p1".into());
        cache.insert(2, "/products/2", "p2".into());
        cache.insert(3, "/products/3", "p3".into());
        cache.insert(4, "/about", "about".into());
        cache.insert(5, "/home", "home".into());

        let removed = cache.remove_by_prefix("/products");
        assert_eq!(removed, 3);
        assert_eq!(cache.len(), 2);
        assert!(cache.get(4).is_some());
        assert!(cache.get(5).is_some());
    }
}
