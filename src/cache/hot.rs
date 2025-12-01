//! Thread-local "hot" cache optimized for L1/L2 CPU cache
//!
//! Size: 8 entries per thread (~4KB with typical HTML)
//! Access time: ~1-3 nanoseconds

use std::sync::Arc;
use std::time::{Duration, Instant};

/// Thread-local hot cache aligned to cache line
///
/// Uses `#[repr(align(64))]` to prevent false sharing between threads.
#[repr(align(64))]
pub struct HotCache {
    entries: [Option<HotEntry>; 8],
    next_insert: usize,
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
            entries: Default::default(),
            next_insert: 0,
            ttl: None,
        }
    }

    /// Create a hot cache with TTL
    pub fn with_ttl(ttl_secs: u64) -> Self {
        Self {
            entries: Default::default(),
            next_insert: 0,
            ttl: if ttl_secs > 0 {
                Some(Duration::from_secs(ttl_secs))
            } else {
                None
            },
        }
    }

    /// Look up HTML by URL hash
    ///
    /// Returns None if not found or expired
    #[inline(always)]
    pub fn get(&self, url_hash: u64) -> Option<Arc<str>> {
        for entry in self.entries.iter().flatten() {
            if entry.url_hash == url_hash {
                // Check TTL
                if let Some(ttl) = self.ttl {
                    if entry.created_at.elapsed() > ttl {
                        return None;
                    }
                }
                return Some(Arc::clone(&entry.html));
            }
        }
        None
    }

    /// Insert a new entry (evicts oldest by round-robin)
    #[inline(always)]
    pub fn insert(&mut self, url_hash: u64, html: Arc<str>) {
        self.entries[self.next_insert] = Some(HotEntry {
            url_hash,
            html,
            created_at: Instant::now(),
        });
        self.next_insert = (self.next_insert + 1) % self.entries.len();
    }

    /// Get number of cached entries
    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.entries.iter().flatten().count()
    }

    /// Check if cache is empty
    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
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
    fn test_eviction() {
        let mut cache = HotCache::new();

        // Insert more than 8 entries
        for i in 0..10 {
            let html: Arc<str> = format!("html{}", i).into();
            cache.insert(i, html);
        }

        // First 2 should be evicted
        assert!(cache.get(0).is_none());
        assert!(cache.get(1).is_none());
        assert!(cache.get(9).is_some());
    }
}
