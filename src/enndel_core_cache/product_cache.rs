use dashmap::DashMap;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
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

/// Кэш продуктов с разделением на критичные и ленивые данные
pub struct ProductCache {
    /// Критичные данные (текст) - в памяти постоянно
    critical: Arc<RwLock<Option<CachedCriticalData>>>,

    /// Ленивые данные (изображения) - LRU кэш
    lazy: Arc<DashMap<i32, Arc<LazyProductData>>>,

    /// TTL для критичных данных
    critical_ttl: Duration,
}

impl ProductCache {
    pub fn new() -> Self {
        Self {
            critical: Arc::new(RwLock::new(None)),
            lazy: Arc::new(DashMap::new()),
            critical_ttl: Duration::from_secs(60), // 60 секунд
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
                    return Ok((cached.data.clone(), cached.hash));
                }
            }
        }

        // Кэш пуст или устарел - загружаем
        tracing::debug!("❄️  Critical products cache MISS - fetching from API");
        self.fetch_and_cache_critical().await
    }

    /// Получить ленивые данные по ID
    pub async fn get_lazy(&self, id: i32) -> Option<Arc<LazyProductData>> {
        // Проверяем кэш
        if let Some(data) = self.lazy.get(&id) {
            tracing::debug!("🔥 Lazy data cache HIT for product {}", id);
            return Some(data.clone());
        }

        // Загружаем из API
        tracing::debug!("❄️  Lazy data cache MISS for product {} - fetching", id);
        match self.fetch_lazy_data(id).await {
            Ok(data) => {
                let arc_data = Arc::new(data);
                self.lazy.insert(id, arc_data.clone());
                Some(arc_data)
            }
            Err(e) => {
                tracing::warn!("Failed to fetch lazy data for product {}: {}", id, e);
                None
            }
        }
    }

    /// Загружает критичные данные с API и кэширует
    async fn fetch_and_cache_critical(&self) -> Result<(Vec<Arc<CriticalProductData>>, u64), String> {
        let response = reqwest::get("https://enddel.com/api/products")
            .await
            .map_err(|e| format!("Failed to fetch products: {}", e))?;

        let full_products: serde_json::Value = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse products JSON: {}", e))?;

        // Извлекаем массив продуктов
        let products_array = if let Some(arr) = full_products.get("products").and_then(|v| v.as_array()) {
            arr.clone()
        } else if let Some(arr) = full_products.as_array() {
            arr.clone()
        } else {
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

        Ok((critical_products, hash))
    }

    /// Загружает ленивые данные для одного продукта
    async fn fetch_lazy_data(&self, id: i32) -> Result<LazyProductData, String> {
        let response = reqwest::get(format!("https://enddel.com/api/products/{}", id))
            .await
            .map_err(|e| format!("Failed to fetch product {}: {}", id, e))?;

        let product: serde_json::Value = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse product JSON: {}", e))?;

        // Извлекаем только изображения
        let lazy_data = LazyProductData {
            id,
            image_url: product.get("image_url").and_then(|v| v.as_str()).map(String::from),
            images: product.get("images").and_then(|v| {
                v.as_array().map(|arr| {
                    arr.iter()
                        .filter_map(|item| item.as_str().map(String::from))
                        .collect()
                })
            }),
        };

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
        // Хэшируем только критичные поля
    }
}
