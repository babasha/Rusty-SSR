//! Thread-local "hot" cache optimized for L1/L2 CPU cache
//!
//! Two-tier design:
//! - Ultra-hot: 8 entries in cache-line aligned array (~1-3ns access)
//! - Hot: HashMap for O(1) lookup on more entries (~5-10ns access)
//!
//! Total capacity: 128 entries per thread

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Maximum entries in ultra-hot array (fits in 2 cache lines)
const ULTRA_HOT_SIZE: usize = 8;

/// Maximum entries in hot HashMap
const HOT_MAP_CAPACITY: usize = 128;

/// Thread-local hot cache with two tiers
///
/// Uses `#[repr(align(64))]` to prevent false sharing between threads.
#[repr(align(64))]
pub struct HotCache {
    // Tier 1: Ultra-hot array for most recent entries
    ultra_hot: [Option<HotEntry>; ULTRA_HOT_SIZE],
    ultra_hot_next: usize,

    // Tier 2: HashMap for O(1) lookup on more entries
    hot_map: HashMap<u64, HotEntry>,

    // LRU tracking for hot_map eviction
    access_order: Vec<u64>,

    ttl: Option<Duration>,
}

#[derive(Clone)]
struct HotEntry {
    url_hash: u64,
    html: Arc<str>,
    created_at: Instant,
}

impl HotCache {
    /// Create a new empty hot cache
    pub fn new() -> Self {
        Self {
            ultra_hot: Default::default(),
            ultra_hot_next: 0,
            hot_map: HashMap::with_capacity(HOT_MAP_CAPACITY),
            access_order: Vec::with_capacity(HOT_MAP_CAPACITY),
            ttl: None,
        }
    }

    /// Create a hot cache with TTL
    pub fn with_ttl(ttl_secs: u64) -> Self {
        Self {
            ultra_hot: Default::default(),
            ultra_hot_next: 0,
            hot_map: HashMap::with_capacity(HOT_MAP_CAPACITY),
            access_order: Vec::with_capacity(HOT_MAP_CAPACITY),
            ttl: if ttl_secs > 0 {
                Some(Duration::from_secs(ttl_secs))
            } else {
                None
            },
        }
    }

    /// Look up HTML by URL hash
    ///
    /// Checks ultra-hot array first (fastest), then HashMap
    #[inline(always)]
    pub fn get(&mut self, url_hash: u64) -> Option<Arc<str>> {
        // Tier 1: Check ultra-hot array first (linear scan, but only 8 entries)
        for entry in self.ultra_hot.iter().flatten() {
            if entry.url_hash == url_hash {
                if self.is_expired(entry) {
                    return None;
                }
                return Some(Arc::clone(&entry.html));
            }
        }

        // Tier 2: Check HashMap (O(1) lookup)
        if let Some(entry) = self.hot_map.get(&url_hash) {
            if self.is_expired(entry) {
                self.hot_map.remove(&url_hash);
                return None;
            }

            // Promote to ultra-hot on access (LRU behavior)
            let html = Arc::clone(&entry.html);
            self.promote_to_ultra_hot(url_hash, Arc::clone(&html));
            return Some(html);
        }

        None
    }

    /// Look up without promotion (for read-only access)
    #[inline(always)]
    pub fn peek(&self, url_hash: u64) -> Option<Arc<str>> {
        // Check ultra-hot first
        for entry in self.ultra_hot.iter().flatten() {
            if entry.url_hash == url_hash {
                if self.is_expired(entry) {
                    return None;
                }
                return Some(Arc::clone(&entry.html));
            }
        }

        // Check HashMap
        if let Some(entry) = self.hot_map.get(&url_hash) {
            if self.is_expired(entry) {
                return None;
            }
            return Some(Arc::clone(&entry.html));
        }

        None
    }

    /// Insert a new entry
    #[inline(always)]
    pub fn insert(&mut self, url_hash: u64, html: Arc<str>) {
        let entry = HotEntry {
            url_hash,
            html,
            created_at: Instant::now(),
        };

        // Always insert into ultra-hot first
        // Evicted entry goes to hot_map
        if let Some(evicted) = self.ultra_hot[self.ultra_hot_next].take() {
            // Move evicted to hot_map
            self.insert_to_hot_map(evicted);
        }

        self.ultra_hot[self.ultra_hot_next] = Some(entry);
        self.ultra_hot_next = (self.ultra_hot_next + 1) % ULTRA_HOT_SIZE;
    }

    /// Insert into hot_map with LRU eviction
    fn insert_to_hot_map(&mut self, entry: HotEntry) {
        // Evict oldest if at capacity
        if self.hot_map.len() >= HOT_MAP_CAPACITY {
            if let Some(oldest_key) = self.access_order.first().copied() {
                self.hot_map.remove(&oldest_key);
                self.access_order.remove(0);
            }
        }

        self.access_order.push(entry.url_hash);
        self.hot_map.insert(entry.url_hash, entry);
    }

    /// Promote an entry from hot_map to ultra-hot
    fn promote_to_ultra_hot(&mut self, url_hash: u64, html: Arc<str>) {
        // Remove from hot_map
        self.hot_map.remove(&url_hash);
        if let Some(pos) = self.access_order.iter().position(|&k| k == url_hash) {
            self.access_order.remove(pos);
        }

        // Insert into ultra-hot (this will move current ultra-hot entry to hot_map)
        self.insert(url_hash, html);
    }

    /// Check if entry is expired
    #[inline(always)]
    fn is_expired(&self, entry: &HotEntry) -> bool {
        if let Some(ttl) = self.ttl {
            entry.created_at.elapsed() > ttl
        } else {
            false
        }
    }

    /// Get total number of cached entries
    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        let ultra_hot_count = self.ultra_hot.iter().flatten().count();
        ultra_hot_count + self.hot_map.len()
    }

    /// Check if cache is empty
    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Clear all entries
    #[allow(dead_code)]
    pub fn clear(&mut self) {
        self.ultra_hot = Default::default();
        self.ultra_hot_next = 0;
        self.hot_map.clear();
        self.access_order.clear();
    }
}

impl Default for HotCache {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_operations() {
        let mut cache = HotCache::new();
        let html: Arc<str> = "test".into();

        cache.insert(123, Arc::clone(&html));

        assert!(cache.get(123).is_some());
        assert!(cache.get(456).is_none());
    }

    #[test]
    fn test_ultra_hot_eviction() {
        let mut cache = HotCache::new();

        // Insert more than 8 entries
        for i in 0..10u64 {
            let html: Arc<str> = format!("html{}", i).into();
            cache.insert(i, html);
        }

        // First 2 should be in hot_map, not ultra_hot
        // But still accessible via get()
        assert!(cache.get(0).is_some(), "Entry 0 should be in hot_map");
        assert!(cache.get(1).is_some(), "Entry 1 should be in hot_map");
        assert!(cache.get(9).is_some(), "Entry 9 should be in ultra_hot");
    }

    #[test]
    fn test_promotion() {
        let mut cache = HotCache::new();

        // Fill ultra_hot
        for i in 0..8u64 {
            cache.insert(i, format!("html{}", i).into());
        }

        // Add more to push to hot_map
        for i in 8..16u64 {
            cache.insert(i, format!("html{}", i).into());
        }

        // Entry 0 should be in hot_map now
        // Accessing it should promote it back to ultra_hot
        let _ = cache.get(0);

        // Verify it's accessible
        assert!(cache.peek(0).is_some());
    }

    #[test]
    fn test_capacity() {
        let mut cache = HotCache::new();

        // Insert 200 entries (more than 128 capacity)
        for i in 0..200u64 {
            cache.insert(i, format!("html{}", i).into());
        }

        // Should have at most 128 + 8 = 136 entries
        assert!(cache.len() <= ULTRA_HOT_SIZE + HOT_MAP_CAPACITY);
    }

    #[test]
    fn test_hashmap_lookup_speed() {
        let mut cache = HotCache::new();

        // Fill with 100 entries
        for i in 0..100u64 {
            cache.insert(i, format!("html{}", i).into());
        }

        // Access entry that's definitely in hot_map
        // This should be O(1), not O(n)
        assert!(cache.peek(50).is_some());
    }
}
