use serde::Serialize;
use std::cell::RefCell;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;
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

    metrics: Arc<SSRCacheMetrics>,
}

#[derive(Default)]
struct SSRCacheMetrics {
    lookups: AtomicU64,
    hot_hits: AtomicU64,
    cold_hits: AtomicU64,
    misses: AtomicU64,
    promotions: AtomicU64,
    renders: AtomicU64,
    render_errors: AtomicU64,
    cold_insertions: AtomicU64,
    cold_evictions: AtomicU64,
    hot_insertions: AtomicU64,
    last_render_ns: AtomicU64,
}

#[derive(Clone, Serialize, Debug)]
pub struct SSRCacheMetricsSnapshot {
    pub lookups: u64,
    pub hot_hits: u64,
    pub cold_hits: u64,
    pub misses: u64,
    pub promotions: u64,
    pub renders: u64,
    pub render_errors: u64,
    pub cold_insertions: u64,
    pub cold_evictions: u64,
    pub hot_insertions: u64,
    pub last_render_ns: u64,
    pub cold_cache_size: usize,
    pub cold_cache_capacity: usize,
}

impl SSRCache {
    /// Создаёт новый SSR cache
    ///
    /// # Arguments
    /// * `max_cold_entries` - Максимальное количество записей в cold cache
    pub fn new(max_cold_entries: usize) -> Self {
        tracing::info!(
            "📦 Creating SSR cache (max_cold_entries={})",
            max_cold_entries
        );

        Self {
            hot_cache: ThreadLocal::new(),
            cold_cache: Arc::new(ColdCache::new(max_cold_entries)),
            metrics: Arc::new(SSRCacheMetrics::default()),
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
    pub async fn get_or_render<F, Fut>(&self, url: &str, render_fn: F) -> Result<Arc<str>, String>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<Arc<str>, String>>,
    {
        let url_hash = hash_url(url);
        self.metrics.lookups.fetch_add(1, Ordering::Relaxed);

        // 1. Проверяем hot cache (L1/L2 - ~1-3ns)
        let hot = self.hot_cache.get_or(|| RefCell::new(HotCache::new()));
        let mut hot = hot.borrow_mut();

        if let Some(html) = hot.get(url_hash) {
            tracing::debug!("🔥 Hot cache hit: {}", url);
            self.metrics.hot_hits.fetch_add(1, Ordering::Relaxed);
            return Ok(html);
        }

        // 2. Проверяем cold cache (RAM - ~100ns)
        if let Some(html) = self.cold_cache.get(url_hash) {
            tracing::debug!("❄️  Cold cache hit (promoting to hot): {}", url);
            self.metrics.cold_hits.fetch_add(1, Ordering::Relaxed);

            // Промотируем в hot cache
            hot.insert(url_hash, Arc::clone(&html));
            self.metrics.promotions.fetch_add(1, Ordering::Relaxed);

            return Ok(html);
        }

        // 3. Cache miss - рендерим
        drop(hot); // Освобождаем RefCell перед async

        tracing::debug!("💨 Cache miss (rendering): {}", url);
        self.metrics.misses.fetch_add(1, Ordering::Relaxed);
        let render_start = Instant::now();
        let html = render_fn().await.map_err(|e| {
            tracing::error!("SSR render error (url={}): {}", url, e);
            self.metrics.render_errors.fetch_add(1, Ordering::Relaxed);
            e
        })?;
        self.metrics.renders.fetch_add(1, Ordering::Relaxed);
        self.metrics
            .last_render_ns
            .store(render_start.elapsed().as_nanos() as u64, Ordering::Relaxed);

        // 4. Сохраняем в оба кэша
        let evicted = self.cold_cache.insert(url_hash, Arc::clone(&html));
        self.metrics.cold_insertions.fetch_add(1, Ordering::Relaxed);
        if evicted.is_some() {
            self.metrics.cold_evictions.fetch_add(1, Ordering::Relaxed);
        }

        let hot = self.hot_cache.get_or(|| RefCell::new(HotCache::new()));
        let mut hot = hot.borrow_mut();
        hot.insert(url_hash, Arc::clone(&html));
        self.metrics.hot_insertions.fetch_add(1, Ordering::Relaxed);

        Ok(html)
    }

    /// Пробует получить HTML из кэша (sync, без рендеринга)
    pub fn try_get(&self, url: &str) -> Option<Arc<str>> {
        let url_hash = hash_url(url);
        self.metrics.lookups.fetch_add(1, Ordering::Relaxed);

        // 1. Проверяем hot cache (L1/L2)
        let hot = self.hot_cache.get_or(|| RefCell::new(HotCache::new()));
        let hot = hot.borrow();

        if let Some(html) = hot.get(url_hash) {
            tracing::debug!("🔥 Hot cache hit: {}", url);
            self.metrics.hot_hits.fetch_add(1, Ordering::Relaxed);
            return Some(html);
        }

        // 2. Проверяем cold cache (RAM)
        if let Some(html) = self.cold_cache.get(url_hash) {
            tracing::debug!("❄️  Cold cache hit (promoting to hot): {}", url);
            drop(hot);
            self.metrics.cold_hits.fetch_add(1, Ordering::Relaxed);

            // Промотируем в hot cache
            let hot = self.hot_cache.get_or(|| RefCell::new(HotCache::new()));
            let mut hot = hot.borrow_mut();
            hot.insert(url_hash, Arc::clone(&html));
            self.metrics.promotions.fetch_add(1, Ordering::Relaxed);

            return Some(html);
        }

        self.metrics.misses.fetch_add(1, Ordering::Relaxed);
        None
    }

    /// Пробует получить HTML из кэша с версионированием
    pub fn try_get_versioned(&self, url: &str, version: u64) -> Option<Arc<str>> {
        // Добавляем версию в ключ кэша
        let versioned_url = format!("{}@v{:x}", url, version);
        self.try_get(&versioned_url)
    }

    /// Вставляет HTML в кэш
    pub fn insert(&self, url: &str, html: Arc<str>) {
        let url_hash = hash_url(url);

        // Сохраняем в cold cache
        let evicted = self.cold_cache.insert(url_hash, Arc::clone(&html));
        self.metrics.cold_insertions.fetch_add(1, Ordering::Relaxed);
        if evicted.is_some() {
            self.metrics.cold_evictions.fetch_add(1, Ordering::Relaxed);
        }

        // И в hot cache текущего потока
        let hot = self.hot_cache.get_or(|| RefCell::new(HotCache::new()));
        let mut hot = hot.borrow_mut();
        hot.insert(url_hash, html);
        self.metrics.hot_insertions.fetch_add(1, Ordering::Relaxed);
    }

    /// Вставляет HTML в кэш с версией
    pub fn insert_versioned(&self, url: &str, html: Arc<str>, version: u64) {
        // Добавляем версию в ключ кэша
        let versioned_url = format!("{}@v{:x}", url, version);
        self.insert(&versioned_url, html);
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

    pub fn metrics(&self) -> SSRCacheMetricsSnapshot {
        SSRCacheMetricsSnapshot {
            lookups: self.metrics.lookups.load(Ordering::Relaxed),
            hot_hits: self.metrics.hot_hits.load(Ordering::Relaxed),
            cold_hits: self.metrics.cold_hits.load(Ordering::Relaxed),
            misses: self.metrics.misses.load(Ordering::Relaxed),
            promotions: self.metrics.promotions.load(Ordering::Relaxed),
            renders: self.metrics.renders.load(Ordering::Relaxed),
            render_errors: self.metrics.render_errors.load(Ordering::Relaxed),
            cold_insertions: self.metrics.cold_insertions.load(Ordering::Relaxed),
            cold_evictions: self.metrics.cold_evictions.load(Ordering::Relaxed),
            hot_insertions: self.metrics.hot_insertions.load(Ordering::Relaxed),
            last_render_ns: self.metrics.last_render_ns.load(Ordering::Relaxed),
            cold_cache_size: self.cold_cache.len(),
            cold_cache_capacity: self.cold_cache.capacity(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::time::Duration;

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

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn test_ssr_cache_query_load() {
        let cache = Arc::new(SSRCache::new(300));
        let render_calls = Arc::new(AtomicUsize::new(0));
        let mut futures = Vec::new();

        for i in 0..1_000usize {
            let cache = Arc::clone(&cache);
            let render_calls = Arc::clone(&render_calls);
            let url = Arc::new(format!(
                "/catalog?page={}&sort={}&filter={}",
                i % 50,
                i % 5,
                i % 7
            ));
            let url_for_render = Arc::clone(&url);

            futures.push(async move {
                cache
                    .get_or_render(url.as_str(), || {
                        let render_calls = Arc::clone(&render_calls);
                        let url_inner = Arc::clone(&url_for_render);
                        async move {
                            let seq = render_calls.fetch_add(1, Ordering::Relaxed);
                            if seq % 25 == 0 {
                                tokio::time::sleep(Duration::from_millis(2)).await;
                            }
                            let html: Arc<str> =
                                Arc::from(format!("<html data=\"{}\"></html>", url_inner.as_str()));
                            Ok(html)
                        }
                    })
                    .await
                    .unwrap();
            });
        }

        futures::future::join_all(futures).await;

        let snapshot = cache.metrics();
        println!("SSR cache metrics snapshot: {:?}", snapshot);
        assert!(snapshot.lookups >= 1_000);
        assert!(snapshot.misses > 0);
        assert!(snapshot.renders > 0);
        assert_eq!(
            snapshot.renders,
            render_calls.load(Ordering::Relaxed) as u64
        );
        assert_eq!(snapshot.cold_cache_capacity, 300);
    }
}
