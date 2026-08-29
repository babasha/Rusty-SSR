//! Thread-local "hot" cache optimized for L1/L2 CPU cache
//!
//! Two-tier design:
//! - Ultra-hot: 8 entries in a cache-line aligned array (~1-3ns access)
//! - Hot: a proper LRU map for O(1) lookup on more entries (~5-10ns access)
//!
//! Total capacity: 8 + 128 entries per thread. A key lives in exactly one tier
//! at a time (insert de-duplicates), so accounting never drifts.

use lru::LruCache;
use std::num::NonZeroUsize;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Maximum entries in ultra-hot array (fits in 2 cache lines)
const ULTRA_HOT_SIZE: usize = 8;

/// Maximum entries in the LRU tier
const HOT_MAP_CAPACITY: usize = 128;

/// Thread-local hot cache with two tiers
///
/// Uses `#[repr(align(64))]` to prevent false sharing between threads.
#[repr(align(64))]
pub struct HotCache {
    // Tier 1: Ultra-hot ring buffer for the most recently inserted entries.
    ultra_hot: [Option<HotEntry>; ULTRA_HOT_SIZE],
    ultra_hot_next: usize,

    // Tier 2: true LRU map (least-recently-used evicted) for the larger set.
    lru: LruCache<u64, HotEntry>,

    ttl: Option<Duration>,
}

#[derive(Clone)]
struct HotEntry {
    url_hash: u64,
    /// Full cache key, compared on lookup so a hash collision misses rather
    /// than returning another key's content.
    key: Arc<str>,
    html: Arc<str>,
    /// When it was stored, and `None` when the cache has no TTL.
    ///
    /// Not simply an `Instant` because inserts happen on the read path — every
    /// promotion out of the cold tier is one — and reading the monotonic clock
    /// to fill a field nothing will ever look at is pure cost.
    created_at: Option<Instant>,
}

/// Where a key was found. `get` and `peek` ask the same question and differ
/// only in what they do with the answer, so the search itself lives in one
/// place: a duplicated scan is a duplicated collision check, and the two copies
/// disagreeing about which entries are valid is exactly the bug the full-key
/// comparison exists to prevent.
enum Found {
    /// In the ultra-hot array. Already as hot as it gets.
    UltraHot(Arc<str>),
    /// In the LRU tier: the content, and the key handle a promotion needs.
    Lru(Arc<str>, Arc<str>),
    /// Nothing usable. `stale` marks an entry stored under this exact key that
    /// is present but past its TTL, which a `&mut self` caller can drop.
    Nothing { stale: bool },
}

impl HotCache {
    /// Create a new empty hot cache
    pub fn new() -> Self {
        Self::with_ttl(0)
    }

    /// Create a hot cache with TTL
    pub fn with_ttl(ttl_secs: u64) -> Self {
        Self {
            ultra_hot: Default::default(),
            ultra_hot_next: 0,
            lru: LruCache::new(NonZeroUsize::new(HOT_MAP_CAPACITY).expect("capacity > 0")),
            ttl: if ttl_secs > 0 {
                Some(Duration::from_secs(ttl_secs))
            } else {
                None
            },
        }
    }

    /// Search both tiers without changing anything.
    ///
    /// Ultra-hot first (a linear scan of 8), then the LRU map read with `peek`
    /// so the search itself never disturbs LRU order. The `key` is compared
    /// after the hash matches, so a 64-bit collision misses rather than
    /// returning another key's content.
    #[inline(always)]
    fn find(&self, url_hash: u64, key: &str) -> Found {
        let ttl = self.ttl;

        // Tier 1: ultra-hot linear scan (only 8 entries). A key lives in
        // exactly one tier, so a match here settles the question either way.
        for entry in self.ultra_hot.iter().flatten() {
            if entry.url_hash == url_hash && entry.key.as_ref() == key {
                return if Self::expired(ttl, entry) {
                    Found::Nothing { stale: false }
                } else {
                    Found::UltraHot(Arc::clone(&entry.html))
                };
            }
        }

        // Tier 2: LRU map.
        match self.lru.peek(&url_hash) {
            Some(e) if e.key.as_ref() == key => {
                if Self::expired(ttl, e) {
                    Found::Nothing { stale: true }
                } else {
                    Found::Lru(Arc::clone(&e.html), Arc::clone(&e.key))
                }
            }
            _ => Found::Nothing { stale: false },
        }
    }

    /// Look up HTML by key hash, verifying the full key
    ///
    /// A hit in the LRU tier is promoted to ultra-hot, and an entry found past
    /// its TTL is dropped rather than left to be re-examined on every future
    /// lookup.
    #[inline(always)]
    pub fn get(&mut self, url_hash: u64, key: &str) -> Option<Arc<str>> {
        match self.find(url_hash, key) {
            Found::UltraHot(html) => Some(html),
            Found::Lru(html, key_arc) => {
                // Promote to ultra-hot (insert de-duplicates from the LRU tier).
                self.insert(url_hash, key_arc, Arc::clone(&html));
                Some(html)
            }
            Found::Nothing { stale } => {
                if stale {
                    self.lru.pop(&url_hash);
                }
                None
            }
        }
    }

    /// Look up without promotion (for read-only access)
    #[inline(always)]
    pub fn peek(&self, url_hash: u64, key: &str) -> Option<Arc<str>> {
        match self.find(url_hash, key) {
            Found::UltraHot(html) | Found::Lru(html, _) => Some(html),
            Found::Nothing { .. } => None,
        }
    }

    /// Insert a new entry
    ///
    /// De-duplicates by hash across both tiers first, so a key is never present
    /// in more than one place (no stale duplicates, no accounting drift).
    #[inline(always)]
    pub fn insert(&mut self, url_hash: u64, key: Arc<str>, html: Arc<str>) {
        // Remove any existing copy of this hash from both tiers.
        for slot in self.ultra_hot.iter_mut() {
            if slot.as_ref().is_some_and(|e| e.url_hash == url_hash) {
                *slot = None;
            }
        }
        self.lru.pop(&url_hash);

        let entry = HotEntry {
            url_hash,
            key,
            html,
            created_at: self.ttl.map(|_| Instant::now()),
        };

        // Place into the ultra-hot ring; demote the slot's previous occupant to
        // the LRU tier (its hash differs from ours — we just cleared ours).
        if let Some(evicted) = self.ultra_hot[self.ultra_hot_next].take() {
            self.lru.put(evicted.url_hash, evicted);
        }
        self.ultra_hot[self.ultra_hot_next] = Some(entry);
        self.ultra_hot_next = (self.ultra_hot_next + 1) % ULTRA_HOT_SIZE;
    }

    /// Check if entry is expired
    #[inline(always)]
    fn expired(ttl: Option<Duration>, entry: &HotEntry) -> bool {
        match ttl {
            // `created_at` is only recorded when there is a TTL to check it
            // against, so an entry without one predates the TTL being set and
            // is treated as fresh rather than as instantly stale.
            Some(t) => entry.created_at.is_some_and(|at| at.elapsed() > t),
            None => false,
        }
    }

    /// Get total number of cached entries
    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.ultra_hot.iter().flatten().count() + self.lru.len()
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
        self.lru.clear();
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

    fn k(i: u64) -> Arc<str> {
        Arc::from(format!("k{}", i))
    }

    #[test]
    fn test_basic_operations() {
        let mut cache = HotCache::new();
        let html: Arc<str> = "test".into();

        cache.insert(123, k(123), Arc::clone(&html));

        assert!(cache.get(123, "k123").is_some());
        assert!(cache.get(456, "k456").is_none());
    }

    #[test]
    fn test_get_rejects_hash_collision() {
        let mut cache = HotCache::new();
        cache.insert(7, Arc::from("/real"), "real".into());

        // Same hash, different key → miss, not the wrong content.
        assert!(cache.peek(7, "/attacker").is_none());
        assert!(cache.peek(7, "/real").is_some());
    }

    #[test]
    fn test_ultra_hot_eviction() {
        let mut cache = HotCache::new();

        // Insert more than 8 entries
        for i in 0..10u64 {
            let html: Arc<str> = format!("html{}", i).into();
            cache.insert(i, k(i), html);
        }

        // First 2 spilled to the LRU tier, but still accessible via get().
        assert!(cache.get(0, "k0").is_some(), "Entry 0 should be in LRU tier");
        assert!(cache.get(1, "k1").is_some(), "Entry 1 should be in LRU tier");
        assert!(cache.get(9, "k9").is_some(), "Entry 9 should be in ultra_hot");
    }

    #[test]
    fn test_promotion() {
        let mut cache = HotCache::new();

        // Fill ultra_hot
        for i in 0..8u64 {
            cache.insert(i, k(i), format!("html{}", i).into());
        }

        // Add more to push to the LRU tier
        for i in 8..16u64 {
            cache.insert(i, k(i), format!("html{}", i).into());
        }

        // Entry 0 is in the LRU tier now; accessing it promotes it to ultra_hot.
        let _ = cache.get(0, "k0");

        // Verify it's accessible
        assert!(cache.peek(0, "k0").is_some());
    }

    #[test]
    fn test_capacity() {
        let mut cache = HotCache::new();

        // Insert 200 entries (more than 128 capacity)
        for i in 0..200u64 {
            cache.insert(i, k(i), format!("html{}", i).into());
        }

        // Should have at most 128 + 8 = 136 entries
        assert!(cache.len() <= ULTRA_HOT_SIZE + HOT_MAP_CAPACITY);
    }

    #[test]
    fn test_hashmap_lookup_speed() {
        let mut cache = HotCache::new();

        // Fill with 100 entries
        for i in 0..100u64 {
            cache.insert(i, k(i), format!("html{}", i).into());
        }

        // Access entry that's definitely in the LRU tier — O(1), not O(n).
        assert!(cache.peek(50, "k50").is_some());
    }

    #[test]
    fn test_reinsert_no_duplicate_drift() {
        let mut cache = HotCache::new();

        // Re-insert the same key many times: it must occupy exactly one slot,
        // so len() stays 1 (the old hot_map+access_order design drifted here).
        for _ in 0..50 {
            cache.insert(42, k(42), "v".into());
        }
        assert_eq!(cache.len(), 1, "re-inserting one key must not create duplicates");
        assert!(cache.peek(42, "k42").is_some());

        // Re-inserting a key that has spilled to the LRU tier also stays unique.
        for i in 0..20u64 {
            cache.insert(i, k(i), "v".into());
        }
        let before = cache.len();
        cache.insert(0, k(0), "v2".into()); // 0 likely in LRU tier by now
        assert!(
            cache.len() <= before,
            "re-insert must not increase count (was {}, now {})",
            before,
            cache.len()
        );
    }
}
