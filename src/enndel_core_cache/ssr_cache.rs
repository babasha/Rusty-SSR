use std::cell::RefCell;
use std::sync::Arc;
use thread_local::ThreadLocal;

use super::cache_utils::hash_url;
use super::cold_cache::ColdCache;
use super::hot_cache::HotCache;

/// Multi-tier SSR cache с CPU cache optimization
///
/// Архитектура:
/// 1. Hot cache (L1/L2): Thread-local, 8 записей × 512 bytes = 4KB на поток
/// 2. Cold cache (RAM): Shared, DashMap для concurrent access
pub struct SSRCache {
    /// Thread-local hot cache (L1/L2 CPU cache)
    hot_cache: ThreadLocal<RefCell<HotCache>>,

    /// Shared cold cache (RAM)
    cold_cache: Arc<ColdCache>,
}

impl SSRCache {
    /// Создаёт новый SSR cache
    ///
    /// # Arguments
    /// * `max_cold_entries` - Максимальное количество записей в cold cache
    pub fn new(max_cold_entries: usize) -> Self {
        tracing::info!("📦 Creating SSR cache (max_cold_entries={})", max_cold_entries);

        Self {
            hot_cache: ThreadLocal::new(),
            cold_cache: Arc::new(ColdCache::new(max_cold_entries)),
        }
    }

    /// Получает закэшированный HTML или вызывает render_fn
    ///
    /// # Arguments
    /// * `url` - URL страницы
    /// * `render_fn` - Функция для рендеринга (если нет в кэше)
    ///
    /// # Returns
    /// HTML string
    pub async fn get_or_render<F, Fut>(
        &self,
        url: &str,
        render_fn: F,
    ) -> Result<Arc<str>, String>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<Arc<str>, String>>,
    {
        let url_hash = hash_url(url);

        // 1. Проверяем hot cache (L1/L2 - ~1-3ns)
        let hot = self.hot_cache.get_or(|| RefCell::new(HotCache::new()));
        let mut hot = hot.borrow_mut();

        if let Some(html) = hot.get(url_hash) {
            tracing::debug!("🔥 Hot cache hit: {}", url);
            return Ok(html);
        }

        // 2. Проверяем cold cache (RAM - ~100ns)
        if let Some(html) = self.cold_cache.get(url_hash) {
            tracing::debug!("❄️  Cold cache hit (promoting to hot): {}", url);

            // Промотируем в hot cache
            hot.insert(url_hash, Arc::clone(&html));

            return Ok(html);
        }

        // 3. Cache miss - рендерим
        drop(hot); // Освобождаем RefCell перед async

        tracing::debug!("💨 Cache miss (rendering): {}", url);
        let html = render_fn().await?;

        // 4. Сохраняем в оба кэша
        self.cold_cache.insert(url_hash, Arc::clone(&html));

        let hot = self.hot_cache.get_or(|| RefCell::new(HotCache::new()));
        let mut hot = hot.borrow_mut();
        hot.insert(url_hash, Arc::clone(&html));

        Ok(html)
    }

    /// Пробует получить HTML из кэша (sync, без рендеринга)
    pub fn try_get(&self, url: &str) -> Option<Arc<str>> {
        let url_hash = hash_url(url);

        // 1. Проверяем hot cache (L1/L2)
        let hot = self.hot_cache.get_or(|| RefCell::new(HotCache::new()));
        let hot = hot.borrow();

        if let Some(html) = hot.get(url_hash) {
            tracing::debug!("🔥 Hot cache hit: {}", url);
            return Some(html);
        }

        // 2. Проверяем cold cache (RAM)
        if let Some(html) = self.cold_cache.get(url_hash) {
            tracing::debug!("❄️  Cold cache hit (promoting to hot): {}", url);
            drop(hot);

            // Промотируем в hot cache
            let hot = self.hot_cache.get_or(|| RefCell::new(HotCache::new()));
            let mut hot = hot.borrow_mut();
            hot.insert(url_hash, Arc::clone(&html));

            return Some(html);
        }

        None
    }

    /// Вставляет HTML в кэш
    pub fn insert(&self, url: &str, html: Arc<str>) {
        let url_hash = hash_url(url);

        // Сохраняем в cold cache
        self.cold_cache.insert(url_hash, Arc::clone(&html));

        // И в hot cache текущего потока
        let hot = self.hot_cache.get_or(|| RefCell::new(HotCache::new()));
        let mut hot = hot.borrow_mut();
        hot.insert(url_hash, html);
    }

    /// Очищает cold cache (hot cache очистится автоматически)
    #[allow(dead_code)]
    pub fn clear(&self) {
        self.cold_cache.clear();
    }

    /// Возвращает количество записей в cold cache
    #[allow(dead_code)]
    pub fn cold_cache_size(&self) -> usize {
        self.cold_cache.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_ssr_cache_basic() {
        let cache = SSRCache::new(100);
        let mut call_count = 0;

        // Первый вызов - должен рендерить
        let html1 = cache
            .get_or_render("/test", || async {
                call_count += 1;
                let data: Arc<str> = "test html".into();
                Ok(data)
            })
            .await
            .unwrap();

        assert_eq!(call_count, 1);
        assert_eq!(&*html1, "test html");

        // Второй вызов - должен взять из кэша
        let html2 = cache
            .get_or_render("/test", || async {
                call_count += 1;
                let data: Arc<str> = "new html".into();
                Ok(data)
            })
            .await
            .unwrap();

        assert_eq!(call_count, 1); // Не должен вызваться
        assert_eq!(&*html2, "test html"); // Должен вернуть старое значение
    }

    #[tokio::test]
    async fn test_ssr_cache_different_urls() {
        let cache = SSRCache::new(100);

        let html1 = cache
            .get_or_render("/page1", || async {
                let data: Arc<str> = "page1".into();
                Ok(data)
            })
            .await
            .unwrap();

        let html2 = cache
            .get_or_render("/page2", || async {
                let data: Arc<str> = "page2".into();
                Ok(data)
            })
            .await
            .unwrap();

        assert_eq!(&*html1, "page1");
        assert_eq!(&*html2, "page2");
    }
}
