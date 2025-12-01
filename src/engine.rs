//! Main SSR Engine

use std::sync::Arc;

use crate::config::{SsrConfig, SsrConfigBuilder};
use crate::error::{SsrError, SsrResult};

#[cfg(feature = "v8-pool")]
use crate::v8_pool::V8Pool;

#[cfg(feature = "cache")]
use crate::cache::SsrCache;

/// The main SSR engine that coordinates V8 pool and caching
pub struct SsrEngine {
    config: SsrConfig,

    #[cfg(feature = "v8-pool")]
    v8_pool: V8Pool,

    #[cfg(feature = "cache")]
    cache: SsrCache,
}

impl SsrEngine {
    /// Create a new configuration builder
    ///
    /// # Example
    /// ```rust,ignore
    /// use rusty_ssr::SsrEngine;
    ///
    /// let engine = SsrEngine::builder()
    ///     .bundle_path("ssr-bundle.js")
    ///     .pool_size(4)
    ///     .build_engine()
    ///     .expect("Failed to create engine");
    /// ```
    pub fn builder() -> SsrConfigBuilder {
        SsrConfigBuilder::default()
    }

    /// Create a new SSR engine with the given configuration
    pub fn new(config: SsrConfig) -> SsrResult<Self> {
        tracing::info!(
            "🚀 Initializing Rusty SSR engine (pool_size={}, cache_size={})",
            config.pool_size,
            config.cache_size
        );

        #[cfg(feature = "v8-pool")]
        let v8_pool = {
            // Initialize the V8 bundle
            crate::v8_pool::init_bundle(&config.bundle_path)?;

            V8Pool::new(crate::v8_pool::V8PoolConfig {
                num_threads: config.pool_size,
                queue_capacity: config.queue_capacity,
                pin_threads: config.pin_threads,
                render_function: config.render_function.clone(),
            })
        };

        #[cfg(feature = "cache")]
        let cache = {
            let ttl_secs = config.cache_ttl.map(|d| d.as_secs()).unwrap_or(0);
            SsrCache::with_ttl(config.cache_size, ttl_secs)
        };

        Ok(Self {
            config,
            #[cfg(feature = "v8-pool")]
            v8_pool,
            #[cfg(feature = "cache")]
            cache,
        })
    }

    /// Render a URL to HTML
    ///
    /// This will first check the cache, and if not found, render via V8.
    ///
    /// # Arguments
    /// * `url` - The URL path to render (e.g., "/home", "/products/123")
    ///
    /// # Example
    /// ```rust,no_run
    /// # use rusty_ssr::SsrEngine;
    /// # async fn example(engine: SsrEngine) {
    /// let html = engine.render("/home").await.unwrap();
    /// # }
    /// ```
    #[cfg(all(feature = "v8-pool", feature = "cache"))]
    pub async fn render(&self, url: &str) -> SsrResult<Arc<str>> {
        self.render_with_data(url, "{}").await
    }

    /// Render a URL to HTML with custom data
    ///
    /// # Arguments
    /// * `url` - The URL path to render
    /// * `data` - JSON string with data to pass to the render function
    #[cfg(all(feature = "v8-pool", feature = "cache"))]
    pub async fn render_with_data(&self, url: &str, data: &str) -> SsrResult<Arc<str>> {
        // Check cache first
        if let Some(cached) = self.cache.try_get(url) {
            tracing::debug!("Cache hit: {}", url);
            return Ok(cached);
        }

        // Cache miss - render via V8
        tracing::debug!("Cache miss, rendering: {}", url);

        let html = self
            .v8_pool
            .render_with_data(url.to_string(), data.to_string())
            .await
            .map_err(SsrError::JsExecution)?;

        let html: Arc<str> = Arc::from(html.as_str());

        // Store in cache
        self.cache.insert(url, Arc::clone(&html));

        Ok(html)
    }

    /// Render without caching (always hits V8)
    #[cfg(feature = "v8-pool")]
    pub async fn render_uncached(&self, url: &str, data: &str) -> SsrResult<String> {
        self.v8_pool
            .render_with_data(url.to_string(), data.to_string())
            .await
            .map_err(SsrError::JsExecution)
    }

    /// Clear the SSR cache
    #[cfg(feature = "cache")]
    pub fn clear_cache(&self) {
        self.cache.clear();
        tracing::info!("SSR cache cleared");
    }

    /// Get cache metrics
    #[cfg(feature = "cache")]
    pub fn cache_metrics(&self) -> crate::cache::CacheMetrics {
        self.cache.metrics()
    }

    /// Get the number of active V8 workers
    #[cfg(feature = "v8-pool")]
    pub fn worker_count(&self) -> usize {
        self.v8_pool.worker_count()
    }

    /// Get a reference to the configuration
    pub fn config(&self) -> &SsrConfig {
        &self.config
    }

    /// Get a reference to the cache (if enabled)
    #[cfg(feature = "cache")]
    pub fn cache(&self) -> &SsrCache {
        &self.cache
    }

    /// Get a reference to the V8 pool (if enabled)
    #[cfg(feature = "v8-pool")]
    pub fn v8_pool(&self) -> &V8Pool {
        &self.v8_pool
    }
}

/// Builder extension to create SsrEngine directly
impl SsrConfigBuilder {
    /// Build the configuration and create an SsrEngine
    pub fn build_engine(self) -> SsrResult<SsrEngine> {
        SsrEngine::new(self.build())
    }
}
