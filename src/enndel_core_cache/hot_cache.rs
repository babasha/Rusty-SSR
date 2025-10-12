use std::sync::Arc;

/// Thread-local "горячий" кэш для L1/L2 CPU cache
/// Размер: 8 записей (thread-local, нет false sharing)
///
/// Cache-line aligned (64 bytes) для предотвращения false sharing
/// между потоками при использовании в ThreadLocal<RefCell<HotCache>>
#[repr(align(64))]
pub struct HotCache {
    /// Фиксированный массив горячих записей
    entries: [Option<HotEntry>; 8],
    /// Round-robin индекс для замены
    next_insert: usize,
}

/// Одна запись в hot cache
#[derive(Clone)]
struct HotEntry {
    /// Хэш URL (для быстрого сравнения)
    url_hash: u64,
    /// Rendered HTML string
    html: Arc<str>,
}

impl HotCache {
    /// Создаёт новый пустой hot cache
    pub fn new() -> Self {
        Self {
            entries: Default::default(),
            next_insert: 0,
        }
    }

    /// Ищет HTML по хэшу URL
    #[inline(always)] // Инлайним для лучшего performance
    pub fn get(&self, url_hash: u64) -> Option<Arc<str>> {
        // Линейный поиск по 8 элементам - очень быстро (все в L1 cache)
        for entry in self.entries.iter().flatten() {
            if entry.url_hash == url_hash {
                return Some(Arc::clone(&entry.html));
            }
        }
        None
    }

    /// Вставляет новую запись (вытесняет старую по round-robin)
    #[inline(always)]
    pub fn insert(&mut self, url_hash: u64, html: Arc<str>) {
        self.entries[self.next_insert] = Some(HotEntry { url_hash, html });
        self.next_insert = (self.next_insert + 1) % self.entries.len();
    }

    /// Возвращает количество закэшированных записей
    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.entries.iter().flatten().count()
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
    fn test_hot_cache_basic() {
        let mut cache = HotCache::new();

        let html: Arc<str> = "test".into();
        cache.insert(123, Arc::clone(&html));

        assert!(cache.get(123).is_some());
        assert!(cache.get(456).is_none());
    }

    #[test]
    fn test_hot_cache_eviction() {
        let mut cache = HotCache::new();

        // Вставляем 10 записей (больше чем 8)
        for i in 0..10 {
            let html: Arc<str> = format!("html{}", i).into();
            cache.insert(i, html);
        }

        // Первые 2 должны быть вытеснены
        assert!(cache.get(0).is_none());
        assert!(cache.get(1).is_none());

        // Последние 8 должны остаться
        assert!(cache.get(2).is_some());
        assert!(cache.get(9).is_some());
    }
}
