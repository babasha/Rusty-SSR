use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

/// Вычисляет хэш URL для использования в качестве ключа кэша
///
/// Использует DefaultHasher для быстрого хэширования строк.
/// Результат используется как ключ в HotCache и ColdCache.
///
/// # Arguments
/// * `url` - URL страницы для хэширования
///
/// # Returns
/// 64-битный хэш URL
///
/// # Examples
/// ```
/// use enndel_core_cache::cache_utils::hash_url;
///
/// let hash1 = hash_url("/home");
/// let hash2 = hash_url("/home");
/// let hash3 = hash_url("/about");
///
/// assert_eq!(hash1, hash2); // Одинаковые URL → одинаковый хэш
/// assert_ne!(hash1, hash3); // Разные URL → разные хэши
/// ```
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
    fn test_hash_url_consistency() {
        let url = "/test/page";
        let hash1 = hash_url(url);
        let hash2 = hash_url(url);

        assert_eq!(hash1, hash2, "Same URL should produce same hash");
    }

    #[test]
    fn test_hash_url_different() {
        let hash1 = hash_url("/page1");
        let hash2 = hash_url("/page2");

        assert_ne!(hash1, hash2, "Different URLs should produce different hashes");
    }

    #[test]
    fn test_hash_url_empty() {
        let hash1 = hash_url("");
        let hash2 = hash_url("");

        assert_eq!(hash1, hash2, "Empty strings should hash consistently");
    }
}
