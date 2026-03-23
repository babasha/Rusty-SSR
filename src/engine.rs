//! Main SSR Engine

use std::sync::Arc;

use crate::config::{SsrConfig, SsrConfigBuilder};
use crate::error::{SsrError, SsrResult};

#[cfg(feature = "v8-pool")]
use crate::v8_pool::{PoolError, V8Pool};

#[cfg(feature = "cache")]
use crate::cache::SsrCache;

// ── HTML Template System ─────────────────────────────────────────────────────

/// Parsed Vite manifest entry
#[derive(Debug)]
struct ManifestEntry {
    file: String,
    css: Vec<String>,
}

/// Pre-loaded HTML template with asset tags injected
#[derive(Debug, Clone)]
struct HtmlTemplate {
    /// Template string with `<!--ssr:outlet-->` still present (replaced per-request)
    content: String,
}

impl HtmlTemplate {
    /// Load template from file, parse manifest, and inject asset tags
    fn load(config: &SsrConfig) -> SsrResult<Option<Self>> {
        let template_path = match &config.html_template_path {
            Some(p) => p,
            None => return Ok(None),
        };

        tracing::info!("📄 Loading HTML template from {:?}", template_path);

        let mut content = std::fs::read_to_string(template_path).map_err(|e| {
            SsrError::Template(format!(
                "Failed to read HTML template {:?}: {}",
                template_path, e
            ))
        })?;

        if !content.contains("<!--ssr:outlet-->") {
            return Err(SsrError::Template(
                "HTML template must contain <!--ssr:outlet--> placeholder".into(),
            ));
        }

        // Parse Vite manifest and inject asset tags
        if let Some(manifest_path) = &config.assets_manifest_path {
            let entry = Self::parse_manifest(manifest_path)?;

            // Build CSS link tags
            let css_tags: String = entry
                .css
                .iter()
                .map(|p| format!(r#"<link rel="stylesheet" href="/{}" />"#, p))
                .collect::<Vec<_>>()
                .join("\n    ");

            // Build script tag
            let script_tag = format!(r#"<script type="module" src="/{}"></script>"#, entry.file);

            content = content.replace("<!--ssr:css-->", &css_tags);
            content = content.replace("<!--ssr:scripts-->", &script_tag);

            tracing::info!(
                "✅ Manifest parsed: JS={}, CSS files={}",
                entry.file,
                entry.css.len()
            );
        }

        // Replace ssr:head with empty (reserved for future use)
        content = content.replace("<!--ssr:head-->", "");

        Ok(Some(HtmlTemplate { content }))
    }

    /// Parse Vite manifest.json and find the entry chunk
    fn parse_manifest(path: &std::path::Path) -> SsrResult<ManifestEntry> {
        let raw = std::fs::read_to_string(path).map_err(|e| {
            SsrError::Template(format!("Failed to read manifest {:?}: {}", path, e))
        })?;

        let manifest: serde_json::Value = serde_json::from_str(&raw).map_err(|e| {
            SsrError::Template(format!("Failed to parse manifest JSON: {}", e))
        })?;

        let obj = manifest
            .as_object()
            .ok_or_else(|| SsrError::Template("Manifest is not a JSON object".into()))?;

        // Find entry with isEntry: true, or fall back to first entry
        let entry_value = obj
            .values()
            .find(|v| v.get("isEntry").and_then(|e| e.as_bool()).unwrap_or(false))
            .or_else(|| obj.values().next())
            .ok_or_else(|| SsrError::Template("Manifest has no entries".into()))?;

        let file = entry_value
            .get("file")
            .and_then(|f| f.as_str())
            .ok_or_else(|| SsrError::Template("Manifest entry missing 'file' field".into()))?
            .to_string();

        let css = entry_value
            .get("css")
            .and_then(|c| c.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();

        Ok(ManifestEntry { file, css })
    }

    /// Inject rendered HTML fragment into the template
    fn inject(&self, fragment: &str) -> String {
        self.content.replace("<!--ssr:outlet-->", fragment)
    }
}

// ── SSR Engine ───────────────────────────────────────────────────────────────

/// The main SSR engine that coordinates V8 pool and caching
pub struct SsrEngine {
    config: SsrConfig,

    /// Pre-loaded HTML template (None = return raw fragments)
    template: Option<HtmlTemplate>,

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

        // Load HTML template + manifest (if configured)
        let template = HtmlTemplate::load(&config)?;
        if template.is_some() {
            tracing::info!("📄 HTML template system enabled");
        }

        #[cfg(feature = "v8-pool")]
        let v8_pool = {
            // Initialize the V8 bundle
            crate::v8_pool::init_bundle(&config.bundle_path)?;

            V8Pool::new(crate::v8_pool::V8PoolConfig {
                num_threads: config.pool_size,
                queue_capacity: config.queue_capacity,
                pin_threads: config.pin_threads,
                request_timeout: config.request_timeout,
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
            template,
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
            .map_err(Self::map_pool_error)?;

        let html: Arc<str> = Arc::from(html.as_str());

        // Store in cache
        self.cache.insert(url, Arc::clone(&html));

        Ok(html)
    }

    /// Render a URL with JSON data (serde_json::Value)
    ///
    /// Convenience method that serializes the Value to a string.
    ///
    /// # Example
    /// ```rust,no_run
    /// # use rusty_ssr::SsrEngine;
    /// # async fn example(engine: SsrEngine) {
    /// use serde_json::json;
    ///
    /// let data = json!({
    ///     "user": { "name": "John" },
    ///     "products": [1, 2, 3]
    /// });
    /// let html = engine.render_json("/products", data).await.unwrap();
    /// # }
    /// ```
    #[cfg(all(feature = "v8-pool", feature = "cache"))]
    pub async fn render_json(
        &self,
        url: &str,
        data: serde_json::Value,
    ) -> SsrResult<Arc<str>> {
        let data_str = data.to_string();
        self.render_with_data(url, &data_str).await
    }

    /// Render a URL and inject result into the HTML template
    ///
    /// If no template is configured, behaves identically to `render()`.
    /// When a template is set, the V8-rendered fragment is injected into
    /// `<!--ssr:outlet-->` and a complete HTML document is returned.
    ///
    /// # Example
    /// ```rust,no_run
    /// # use rusty_ssr::SsrEngine;
    /// # async fn example(engine: SsrEngine) {
    /// // Returns complete HTML document if template is configured
    /// let html = engine.render_to_html("/home").await.unwrap();
    /// // html = "<!doctype html><html>...<div id='root'>...app...</div>...</html>"
    /// # }
    /// ```
    #[cfg(all(feature = "v8-pool", feature = "cache"))]
    pub async fn render_to_html(&self, url: &str) -> SsrResult<String> {
        self.render_to_html_with_data(url, "{}").await
    }

    /// Render a URL with data and inject into the HTML template
    #[cfg(all(feature = "v8-pool", feature = "cache"))]
    pub async fn render_to_html_with_data(&self, url: &str, data: &str) -> SsrResult<String> {
        let fragment = self.render_with_data(url, data).await?;

        match &self.template {
            Some(tmpl) => Ok(tmpl.inject(&fragment)),
            None => Ok(fragment.to_string()),
        }
    }

    /// Check if the HTML template system is enabled
    pub fn has_template(&self) -> bool {
        self.template.is_some()
    }

    /// Render without caching (always hits V8)
    #[cfg(feature = "v8-pool")]
    pub async fn render_uncached(&self, url: &str, data: &str) -> SsrResult<String> {
        self.v8_pool
            .render_with_data(url.to_string(), data.to_string())
            .await
            .map_err(Self::map_pool_error)
    }

    /// Render without caching with JSON data
    #[cfg(feature = "v8-pool")]
    pub async fn render_uncached_json(
        &self,
        url: &str,
        data: serde_json::Value,
    ) -> SsrResult<String> {
        self.render_uncached(url, &data.to_string()).await
    }

    /// Invalidate a single cached URL
    ///
    /// Use after content updates for a specific page.
    #[cfg(feature = "cache")]
    pub fn invalidate(&self, url: &str) {
        self.cache.invalidate(url);
        tracing::debug!("Cache invalidated: {}", url);
    }

    /// Invalidate all cached URLs matching a prefix
    ///
    /// Example: `engine.invalidate_prefix("/products")` clears all product pages.
    /// Returns the number of removed entries.
    #[cfg(feature = "cache")]
    pub fn invalidate_prefix(&self, prefix: &str) -> usize {
        let removed = self.cache.invalidate_prefix(prefix);
        tracing::info!("Cache invalidated {} entries with prefix: {}", removed, prefix);
        removed
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
        SsrEngine::new(self.build()?)
    }
}

impl SsrEngine {
    #[cfg(feature = "v8-pool")]
    fn map_pool_error(err: PoolError) -> SsrError {
        match err {
            PoolError::Timeout => SsrError::Timeout,
            PoolError::Disconnected => SsrError::PoolFull,
            PoolError::WorkerCrashed => {
                SsrError::JsExecution("V8 worker crashed".to_string())
            }
            PoolError::Render(msg) => SsrError::JsExecution(msg),
        }
    }
}
