use dashmap::DashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// "Холодный" кэш в RAM (fallback для hot cache)
///
/// Использует DashMap для lock-free concurrent доступа и простую
/// LRU-подобную eviction стратегию с монотонным счётчиком.
///
/// # Performance
/// - Get: O(1) - DashMap lookup
/// - Insert: O(1) amortized - изредка O(n) при eviction
/// - Thread-safe: Lock-free для чтений, минимальная блокировка для записи
pub struct ColdCache {
    /// Основное хранилище: URL hash → (HTML, access_count)
    cache: DashMap<u64, CacheEntry>,
    /// Максимальное количество записей
    max_entries: usize,
    /// Глобальный монотонный счётчик доступов для LRU
    access_counter: AtomicU64,
}

/// Запись в холодном кэше с метаданными для LRU
struct CacheEntry {
    /// Cached HTML content
    html: Arc<str>,
    /// Порядковый номер последнего доступа (для LRU)
    last_access: AtomicU64,
}

impl ColdCache {
    /// Создаёт новый cold cache
    pub fn new(max_entries: usize) -> Self {
        Self {
            cache: DashMap::new(),
            max_entries,
            access_counter: AtomicU64::new(0),
        }
    }

    /// Получает HTML из кэша и обновляет LRU счётчик
    #[inline(always)]
    pub fn get(&self, url_hash: u64) -> Option<Arc<str>> {
        self.cache.get(&url_hash).map(|entry| {
            // Обновляем счётчик последнего доступа (для LRU)
            let new_access = self.access_counter.fetch_add(1, Ordering::Relaxed);
            entry.last_access.store(new_access, Ordering::Relaxed);

            Arc::clone(&entry.html)
        })
    }

    /// Вставляет HTML в кэш с LRU eviction
    pub fn insert(&self, url_hash: u64, html: Arc<str>) -> Option<u64> {
        // Evict least recently used entry if cache is full
        let evicted = if self.cache.len() >= self.max_entries {
            self.evict_lru()
        } else {
            None
        };

        let new_access = self.access_counter.fetch_add(1, Ordering::Relaxed);
        self.cache.insert(
            url_hash,
            CacheEntry {
                html,
                last_access: AtomicU64::new(new_access),
            },
        );

        evicted
    }

    /// Вытесняет наименее недавно использованную запись (LRU)
    fn evict_lru(&self) -> Option<u64> {
        // Находим запись с минимальным last_access
        let mut oldest_key: Option<u64> = None;
        let mut oldest_access = u64::MAX;

        for entry in self.cache.iter() {
            let access = entry.last_access.load(Ordering::Relaxed);
            if access < oldest_access {
                oldest_access = access;
                oldest_key = Some(*entry.key());
            }
        }

        // Удаляем найденную запись
        if let Some(key) = oldest_key {
            self.cache.remove(&key);
        }

        oldest_key
    }

    /// Возвращает количество записей в кэше
    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.cache.len()
    }

    /// Очищает кэш
    #[allow(dead_code)]
    pub fn clear(&self) {
        self.cache.clear();
    }

    pub fn capacity(&self) -> usize {
        self.max_entries
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cold_cache_basic() {
        let cache = ColdCache::new(100);

        let html: Arc<str> = "test".into();
        cache.insert(123, Arc::clone(&html));

        assert!(cache.get(123).is_some());
        assert!(cache.get(456).is_none());
    }

    #[test]
    fn test_cold_cache_eviction() {
        let cache = ColdCache::new(5);

        // Вставляем 10 записей (больше чем max_entries)
        for i in 0..10 {
            let html: Arc<str> = format!("html{}", i).into();
            cache.insert(i, html);
        }

        // Должно остаться только 5 записей
        assert_eq!(cache.len(), 5);
    }

    #[test]
    fn test_cold_cache_concurrent() {
        use std::thread;

        let cache = Arc::new(ColdCache::new(1000));
        let mut handles = vec![];

        // Запускаем 10 потоков, каждый вставляет 100 записей
        for thread_id in 0..10 {
            let cache = Arc::clone(&cache);
            let handle = thread::spawn(move || {
                for i in 0..100 {
                    let hash = (thread_id * 100 + i) as u64;
                    let html: Arc<str> = format!("html{}", hash).into();
                    cache.insert(hash, html);
                }
            });
            handles.push(handle);
        }

        // Ждём завершения всех потоков
        for handle in handles {
            handle.join().unwrap();
        }

        // Должно быть 1000 записей (или меньше если были evictions)
        assert!(cache.len() <= 1000);
        assert!(cache.len() > 0);
    }
}
