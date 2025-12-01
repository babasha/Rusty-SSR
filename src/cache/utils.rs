//! Cache utility functions

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

/// Compute a hash for a URL to use as cache key
///
/// Uses DefaultHasher for fast string hashing.
#[inline(always)]
pub fn hash_url(url: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    url.hash(&mut hasher);
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_consistency() {
        let hash1 = hash_url("/test");
        let hash2 = hash_url("/test");
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_hash_different() {
        let hash1 = hash_url("/page1");
        let hash2 = hash_url("/page2");
        assert_ne!(hash1, hash2);
    }
}
