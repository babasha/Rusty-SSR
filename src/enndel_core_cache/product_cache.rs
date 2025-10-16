use lru::LruCache;
use parking_lot::{Mutex, RwLock};
use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::num::NonZeroUsize;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Критичные данные продукта (для SSR и SEO)
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct CriticalProductData {
    pub id: i32,
    pub name: serde_json::Value, // {ru, en, geo}
    pub price: f64,
    pub unit: String,
    pub step: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stock_quantity: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category_id: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vendor_id: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slug: Option<String>,
}

/// Ленивые данные продукта (изображения, видео)
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct LazyProductData {
    pub id: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub images: Option<Vec<String>>,
}

/// Кэшированные критичные данные с метаданными
struct CachedCriticalData {
    data: Vec<Arc<CriticalProductData>>,
    hash: u64,
    timestamp: Instant,
}

#[derive(Default)]
struct ProductCacheMetrics {
    lazy_hits: AtomicU64,
    lazy_misses: AtomicU64,
    lazy_fetch_success: AtomicU64,
    lazy_fetch_errors: AtomicU64,
    lazy_evictions: AtomicU64,
    lazy_last_fetch_ns: AtomicU64,
    critical_hits: AtomicU64,
    critical_misses: AtomicU64,
    critical_refresh_success: AtomicU64,
    critical_refresh_errors: AtomicU64,
    critical_last_refresh_ns: AtomicU64,
}

#[derive(Clone, Serialize, Debug)]
pub struct ProductCacheMetricsSnapshot {
    pub lazy_hits: u64,
    pub lazy_misses: u64,
    pub lazy_fetch_success: u64,
    pub lazy_fetch_errors: u64,
    pub lazy_evictions: u64,
    pub lazy_last_fetch_ns: u64,
    pub lazy_cache_len: usize,
    pub lazy_cache_capacity: usize,
    pub critical_hits: u64,
    pub critical_misses: u64,
    pub critical_refresh_success: u64,
    pub critical_refresh_errors: u64,
    pub critical_last_refresh_ns: u64,
    pub critical_cached_entries: usize,
    pub critical_cache_age_ms: Option<u128>,
}

/// Кэш продуктов с разделением на критичные и ленивые данные
pub struct ProductCache {
    /// Критичные данные (текст) - в памяти постоянно
    critical: Arc<RwLock<Option<CachedCriticalData>>>,

    /// Ленивые данные (изображения) - LRU кэш
    lazy: Arc<Mutex<LruCache<i32, Arc<LazyProductData>>>>,

    /// TTL для критичных данных
    critical_ttl: Duration,

    metrics: Arc<ProductCacheMetrics>,

    api_base: Arc<str>,
    lazy_capacity: NonZeroUsize,
}

impl ProductCache {
    pub const DEFAULT_LAZY_CACHE_CAPACITY: usize = 256;
    pub const DEFAULT_API_BASE: &'static str = "https://enddel.com/api";

    pub fn new() -> Self {
        Self::with_options(Self::DEFAULT_API_BASE, Self::DEFAULT_LAZY_CACHE_CAPACITY)
    }

    pub fn with_options(base_url: impl Into<String>, lazy_capacity: usize) -> Self {
        let normalized = base_url.into();
        let normalized = normalized.trim_end_matches('/').to_string();
        let api_base: Arc<str> = Arc::from(normalized);
        let capacity = NonZeroUsize::new(lazy_capacity).unwrap_or_else(|| {
            NonZeroUsize::new(Self::DEFAULT_LAZY_CACHE_CAPACITY)
                .expect("default lazy cache capacity must be > 0")
        });

        Self {
            critical: Arc::new(RwLock::new(None)),
            lazy: Arc::new(Mutex::new(LruCache::new(capacity))),
            critical_ttl: Duration::from_secs(60), // 60 секунд
            metrics: Arc::new(ProductCacheMetrics::default()),
            api_base,
            lazy_capacity: capacity,
        }
    }

    /// Получить все критичные данные (для SSR)
    pub async fn get_critical_all(&self) -> Result<(Vec<Arc<CriticalProductData>>, u64), String> {
        // Проверяем кэш
        {
            let cache = self.critical.read();
            if let Some(cached) = &*cache {
                // Проверяем TTL
                if cached.timestamp.elapsed() < self.critical_ttl {
                    tracing::debug!("🔥 Critical products cache HIT");
                    self.metrics.critical_hits.fetch_add(1, Ordering::Relaxed);
                    return Ok((cached.data.clone(), cached.hash));
                }
            }
        }

        self.metrics.critical_misses.fetch_add(1, Ordering::Relaxed);

        // Кэш пуст или устарел - загружаем
        tracing::debug!("❄️  Critical products cache MISS - fetching from API");
        self.fetch_and_cache_critical().await
    }

    /// Получить ленивые данные по ID
    pub async fn get_lazy(&self, id: i32) -> Option<Arc<LazyProductData>> {
        // Проверяем кэш
        {
            let mut lazy_cache = self.lazy.lock();
            if let Some(data) = lazy_cache.get(&id) {
                tracing::debug!("🔥 Lazy data cache HIT for product {}", id);
                self.metrics.lazy_hits.fetch_add(1, Ordering::Relaxed);
                return Some(Arc::clone(data));
            }
        }

        self.metrics.lazy_misses.fetch_add(1, Ordering::Relaxed);

        // Загружаем из API
        tracing::debug!("❄️  Lazy data cache MISS for product {} - fetching", id);
        match self.fetch_lazy_data(id).await {
            Ok(data) => {
                let arc_data = Arc::new(data);
                let mut lazy_cache = self.lazy.lock();
                if let Some((evicted_id, _)) = lazy_cache.push(id, Arc::clone(&arc_data)) {
                    tracing::trace!("🗑️  Lazy cache evicted product {}", evicted_id);
                    self.metrics.lazy_evictions.fetch_add(1, Ordering::Relaxed);
                }
                self.metrics
                    .lazy_fetch_success
                    .fetch_add(1, Ordering::Relaxed);
                Some(arc_data)
            }
            Err(e) => {
                tracing::warn!("Failed to fetch lazy data for product {}: {}", id, e);
                self.metrics
                    .lazy_fetch_errors
                    .fetch_add(1, Ordering::Relaxed);
                None
            }
        }
    }

    /// Загружает критичные данные с API и кэширует
    async fn fetch_and_cache_critical(
        &self,
    ) -> Result<(Vec<Arc<CriticalProductData>>, u64), String> {
        let start = Instant::now();
        let url = format!("{}/products", self.api_base);
        let response = reqwest::get(&url).await.map_err(|e| {
            self.metrics
                .critical_refresh_errors
                .fetch_add(1, Ordering::Relaxed);
            format!("Failed to fetch products: {}", e)
        })?;

        let full_products: serde_json::Value = response.json().await.map_err(|e| {
            self.metrics
                .critical_refresh_errors
                .fetch_add(1, Ordering::Relaxed);
            format!("Failed to parse products JSON: {}", e)
        })?;

        // Извлекаем массив продуктов
        let products_array =
            if let Some(arr) = full_products.get("products").and_then(|v| v.as_array()) {
                arr.clone()
            } else if let Some(arr) = full_products.as_array() {
                arr.clone()
            } else {
                self.metrics
                    .critical_refresh_errors
                    .fetch_add(1, Ordering::Relaxed);
                return Err("Invalid products response format".to_string());
            };

        // Преобразуем в критичные данные
        let mut critical_products = Vec::new();
        for product in products_array {
            if let Ok(critical) = serde_json::from_value::<CriticalProductData>(product.clone()) {
                critical_products.push(Arc::new(critical));
            }
        }

        // Вычисляем хэш для версионирования
        let hash = calculate_hash(&critical_products);

        // Сохраняем в кэш
        let cached = CachedCriticalData {
            data: critical_products.clone(),
            hash,
            timestamp: Instant::now(),
        };

        *self.critical.write() = Some(cached);

        tracing::info!(
            "✅ Cached {} critical products (hash: 0x{:X})",
            critical_products.len(),
            hash
        );
        self.metrics
            .critical_refresh_success
            .fetch_add(1, Ordering::Relaxed);
        self.metrics
            .critical_last_refresh_ns
            .store(start.elapsed().as_nanos() as u64, Ordering::Relaxed);

        Ok((critical_products, hash))
    }

    /// Загружает ленивые данные для одного продукта
    async fn fetch_lazy_data(&self, id: i32) -> Result<LazyProductData, String> {
        let start = Instant::now();
        let response = reqwest::get(format!("{}/products/{}", self.api_base, id))
            .await
            .map_err(|e| format!("Failed to fetch product {}: {}", id, e))?;

        let product: serde_json::Value = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse product JSON: {}", e))?;

        // Извлекаем только изображения
        let lazy_data = LazyProductData {
            id,
            image_url: product
                .get("image_url")
                .and_then(|v| v.as_str())
                .map(String::from),
            images: product.get("images").and_then(|v| {
                v.as_array().map(|arr| {
                    arr.iter()
                        .filter_map(|item| item.as_str().map(String::from))
                        .collect()
                })
            }),
        };

        self.metrics
            .lazy_last_fetch_ns
            .store(start.elapsed().as_nanos() as u64, Ordering::Relaxed);

        Ok(lazy_data)
    }

    /// Предзагрузка при старте сервера
    pub async fn preload(&self) {
        tracing::info!("🔄 Preloading critical product data...");
        match self.fetch_and_cache_critical().await {
            Ok((products, hash)) => {
                tracing::info!(
                    "✅ Preloaded {} products (hash: 0x{:X})",
                    products.len(),
                    hash
                );
            }
            Err(e) => {
                tracing::warn!("⚠️  Failed to preload products: {}", e);
            }
        }
    }

    /// Инвалидация кэша (для ручного обновления)
    pub fn invalidate_critical(&self) {
        *self.critical.write() = None;
        tracing::info!("🗑️  Critical products cache invalidated");
    }

    /// Получить версию данных
    pub fn get_version(&self) -> Option<u64> {
        self.critical.read().as_ref().map(|c| c.hash)
    }

    /// Снимок метрик для анализа (thread-safe)
    pub fn metrics(&self) -> ProductCacheMetricsSnapshot {
        let lazy_snapshot = {
            let lazy = self.lazy.lock();
            (lazy.len(), self.lazy_capacity.get())
        };

        let critical_snapshot = {
            let cache = self.critical.read();
            cache.as_ref().map(|c| {
                (
                    c.data.len(),
                    Instant::now()
                        .checked_duration_since(c.timestamp)
                        .map(|d| d.as_millis()),
                )
            })
        };

        ProductCacheMetricsSnapshot {
            lazy_hits: self.metrics.lazy_hits.load(Ordering::Relaxed),
            lazy_misses: self.metrics.lazy_misses.load(Ordering::Relaxed),
            lazy_fetch_success: self.metrics.lazy_fetch_success.load(Ordering::Relaxed),
            lazy_fetch_errors: self.metrics.lazy_fetch_errors.load(Ordering::Relaxed),
            lazy_evictions: self.metrics.lazy_evictions.load(Ordering::Relaxed),
            lazy_last_fetch_ns: self.metrics.lazy_last_fetch_ns.load(Ordering::Relaxed),
            lazy_cache_len: lazy_snapshot.0,
            lazy_cache_capacity: lazy_snapshot.1,
            critical_hits: self.metrics.critical_hits.load(Ordering::Relaxed),
            critical_misses: self.metrics.critical_misses.load(Ordering::Relaxed),
            critical_refresh_success: self
                .metrics
                .critical_refresh_success
                .load(Ordering::Relaxed),
            critical_refresh_errors: self.metrics.critical_refresh_errors.load(Ordering::Relaxed),
            critical_last_refresh_ns: self
                .metrics
                .critical_last_refresh_ns
                .load(Ordering::Relaxed),
            critical_cached_entries: critical_snapshot
                .as_ref()
                .map(|(len, _)| *len)
                .unwrap_or_default(),
            critical_cache_age_ms: critical_snapshot.and_then(|(_, age)| age),
        }
    }
}

/// Вычисляет хэш для версионирования данных
fn calculate_hash<T: Hash>(obj: &T) -> u64 {
    let mut hasher = DefaultHasher::new();
    obj.hash(&mut hasher);
    hasher.finish()
}

impl Hash for CriticalProductData {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state);
        self.price.to_bits().hash(state);
        self.unit.hash(state);
        self.step.to_bits().hash(state);
        self.stock_quantity.hash(state);
        self.category_id.hash(state);
        self.vendor_id.hash(state);
        self.slug.hash(state);

        // Имя хранится как произвольный JSON, поэтому сериализуем его в строку
        let name_repr = serde_json::to_string(&self.name).unwrap_or_else(|_| self.name.to_string());
        name_repr.hash(state);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::future::join_all;
    use httpmock::prelude::*;
    use std::sync::Arc;

    #[tokio::test]
    async fn test_product_cache_metrics_and_lru_behavior() {
        let server = MockServer::start_async().await;

        let products: Vec<_> = (0..16)
            .map(|id| {
                serde_json::json!({
                    "id": id,
                    "name": { "ru": format!("Товар {}", id) },
                    "price": 10.0 + id as f64,
                    "unit": "kg",
                    "step": 1.0,
                    "stock_quantity": 5,
                    "category_id": 2,
                    "vendor_id": 3,
                    "slug": format!("product-{}", id)
                })
            })
            .collect();

        let products_mock = server
            .mock_async(|when, then| {
                when.method(GET).path("/api/products");
                then.status(200)
                    .json_body(serde_json::json!({ "products": products }));
            })
            .await;

        let lazy_body = serde_json::json!({
            "id": 0,
            "image_url": "https://cdn.example.com/image.jpg",
            "images": ["https://cdn.example.com/image_1.jpg", "https://cdn.example.com/image_2.jpg"]
        });

        let lazy_mock = server
            .mock_async(move |when, then| {
                when.method(GET)
                    .path_matches(Regex::new(r"^/api/products/\d+$").unwrap());
                then.status(200).json_body(lazy_body.clone());
            })
            .await;

        let cache = Arc::new(ProductCache::with_options(
            format!("{}/api", server.base_url()),
            ProductCache::DEFAULT_LAZY_CACHE_CAPACITY,
        ));

        let (critical_products, _) = cache.get_critical_all().await.unwrap();
        assert_eq!(critical_products.len(), 16);

        // Второй вызов должен попасть в кэш
        cache.get_critical_all().await.unwrap();

        // Массово запрашиваем ленивые данные, чтобы выбить эвикции
        let first_pass = (0..400).map(|id| {
            let cache = Arc::clone(&cache);
            async move { cache.get_lazy(id).await }
        });
        let results = join_all(first_pass).await;
        assert_eq!(results.iter().filter(|d| d.is_some()).count(), 400);

        // Повторный проход по части ключей для подсчёта хитовых метрик
        for id in 350..400 {
            cache.get_lazy(id).await;
        }

        let metrics = cache.metrics();
        assert_eq!(metrics.critical_misses, 1);
        assert_eq!(metrics.critical_hits, 1);
        assert!(metrics.lazy_misses >= 400);
        assert!(metrics.lazy_hits > 0);
        assert!(metrics.lazy_evictions > 0);
        assert!(metrics.lazy_fetch_success >= 400);
        assert!(metrics.lazy_cache_len <= ProductCache::DEFAULT_LAZY_CACHE_CAPACITY);

        products_mock.assert_async().await;
        assert!(lazy_mock.hits_async().await >= 400);
    }
}
