//! Configuration for Rusty SSR engine

use std::path::PathBuf;
use std::time::Duration;

use crate::error::{SsrError, SsrResult};

/// Configuration for the SSR engine
#[derive(Debug, Clone)]
pub struct SsrConfig {
    /// Path to the JavaScript SSR bundle
    pub bundle_path: PathBuf,

    /// Number of V8 worker threads (default: number of CPUs)
    pub pool_size: usize,

    /// Size of the task queue for V8 pool
    pub queue_capacity: usize,

    /// Pin V8 workers to specific CPU cores
    pub pin_threads: bool,

    /// Maximum entries in the SSR cache
    pub cache_size: usize,

    /// Cache TTL (None = no expiration)
    pub cache_ttl: Option<Duration>,

    /// Timeout for the whole render request — enqueueing *and* waiting for the
    /// V8 worker's response (None = wait indefinitely)
    pub request_timeout: Option<Duration>,

    /// Name of the global render function in JS bundle
    pub render_function: String,

    /// Path to an HTML template with SSR placeholders (optional)
    ///
    /// When set, the engine injects rendered HTML into the template
    /// instead of returning raw fragments. Supported placeholders:
    /// - `<!--ssr:outlet-->` — rendered app HTML
    /// - `<!--ssr:css-->`    — `<link>` tags from Vite manifest
    /// - `<!--ssr:scripts-->` — `<script>` tags from Vite manifest
    /// - `<!--ssr:head-->`   — extra head content (reserved)
    pub html_template_path: Option<PathBuf>,

    /// Path to Vite manifest.json for hashed asset paths (optional)
    ///
    /// Used together with `html_template_path` to inject correct
    /// `<link>` and `<script>` tags with content-hashed filenames.
    pub assets_manifest_path: Option<PathBuf>,

    /// Prepend the built-in browser polyfills to the bundle (default: true)
    ///
    /// Set to `false` when your bundle already provides every global it
    /// needs (`window`, `document`, `URL`, …). The polyfills are otherwise
    /// non-clobbering, so leaving this on is safe for most bundles.
    pub polyfills: bool,

    /// Cache empty render results (default: true)
    ///
    /// Set to `false` to skip caching renders that produce an empty string.
    /// Useful when a framework returns `""` on a transient condition (e.g.
    /// a suspended component) that you don't want frozen in the cache —
    /// especially with `cache_ttl = None`, where it would persist until
    /// manual invalidation.
    pub cache_empty: bool,

    /// Maximum V8 heap size per worker isolate, in megabytes (default: none)
    ///
    /// When set, each isolate is created with this heap cap. A render that
    /// would exceed it has its execution terminated and returns an error
    /// (uncached) instead of aborting the whole process — useful on
    /// memory-constrained hosts. The cap is approximate: V8 may briefly
    /// exceed it while unwinding the over-budget render. `None` = V8 default
    /// (effectively unbounded).
    pub max_heap_mb: Option<usize>,

    /// Delete globals the bundle did not have at startup, before every render
    /// (default: false)
    ///
    /// The pooled isolate's `globalThis` outlives a render, so whatever one
    /// request hangs on it is readable by the next — which is a different
    /// person. `onSsrRequest` lets a bundle clear its own state, but only a
    /// bundle that knows to define it; the code that actually leaks is usually
    /// a dependency that does not. This closes that half without asking the
    /// bundle for anything: the engine records `globalThis`'s own property
    /// names once the bundle has loaded, and every render starts by removing
    /// anything added since.
    ///
    /// Off by default because it is right for correctness and wrong for a
    /// bundle that caches across renders on purpose — a compiled-template
    /// cache, a warmed lookup table. Those are legitimate, so this is the
    /// caller's decision.
    ///
    /// It does not reach state held in module closures; nothing outside the
    /// bundle can. That is what `onSsrRequest` is for, and the two compose.
    pub seal_globals: bool,

    /// Refuse a render that produced fewer than this many bytes (default: none)
    ///
    /// An empty render is the SSR failure that does not announce itself. Every
    /// real bundle wraps its render in a `try/catch` — frameworks throw for
    /// ordinary reasons, a suspended component being the usual one — and the
    /// catch returns `""`. The engine cannot tell that from a page whose
    /// content is legitimately nothing, so by default it hands the empty string
    /// back and the caller serves a blank page under a 200.
    ///
    /// Set this to the smallest body you would ever call a real page and a
    /// blank render becomes an `Err` the caller can fall back from — to a
    /// client-rendered shell, usually, which is a far better answer than an
    /// empty one. A few hundred bytes is a sensible floor: enough to catch
    /// `""` and a bare wrapper element, not so much that a genuinely small
    /// page trips it.
    ///
    /// Compared against the *trimmed* length, so whitespace is not content.
    pub min_render_bytes: Option<usize>,

    /// Optional cache-key normalizer applied to the URL before lookup/insert
    ///
    /// Return a canonical key so URLs that render identically share one cache
    /// entry — e.g. strip tracking params (`utm_*`, `fbclid`) or sort the
    /// query string. `None` = use the URL verbatim (zero overhead).
    ///
    /// Only affects cache keying; the original URL is still passed to the
    /// render function. `invalidate` normalizes too; `invalidate_prefix`
    /// matches on the (normalized) stored keys.
    pub cache_key_normalizer: Option<fn(&str) -> String>,

    /// Policy for the page cache — finished documents keyed by
    /// [`RenderKey`](crate::cache::RenderKey), with single-flight and
    /// stale-while-revalidate.
    ///
    /// Separate from `cache_size`/`cache_ttl`, which govern the fragment cache
    /// behind `render`/`render_with_data`. The two are different tiers holding
    /// different things; see [`crate::cache::page`].
    #[cfg(feature = "cache")]
    pub page_cache: crate::cache::CachePolicy,
}

impl Default for SsrConfig {
    fn default() -> Self {
        Self {
            bundle_path: PathBuf::from("ssr-bundle.js"),
            pool_size: num_cpus::get(),
            queue_capacity: 512,
            pin_threads: false,
            cache_size: 300,
            cache_ttl: Some(Duration::from_secs(300)), // 5 minutes
            request_timeout: Some(Duration::from_secs(30)),
            render_function: "renderPage".to_string(),
            html_template_path: None,
            assets_manifest_path: None,
            polyfills: true,
            cache_empty: true,
            max_heap_mb: None,
            min_render_bytes: None,
            seal_globals: false,
            cache_key_normalizer: None,
            #[cfg(feature = "cache")]
            page_cache: crate::cache::CachePolicy::default(),
        }
    }
}

impl SsrConfig {
    /// Create a new configuration builder
    pub fn builder() -> SsrConfigBuilder {
        SsrConfigBuilder::default()
    }
}

/// Builder for SsrConfig
#[derive(Debug, Default)]
pub struct SsrConfigBuilder {
    bundle_path: Option<PathBuf>,
    pool_size: Option<usize>,
    queue_capacity: Option<usize>,
    pin_threads: Option<bool>,
    cache_size: Option<usize>,
    cache_ttl: Option<Option<Duration>>,
    request_timeout: Option<Option<Duration>>,
    render_function: Option<String>,
    html_template_path: Option<PathBuf>,
    assets_manifest_path: Option<PathBuf>,
    polyfills: Option<bool>,
    cache_empty: Option<bool>,
    max_heap_mb: Option<usize>,
    min_render_bytes: Option<usize>,
    seal_globals: Option<bool>,
    cache_key_normalizer: Option<fn(&str) -> String>,
    #[cfg(feature = "cache")]
    page_cache: Option<crate::cache::CachePolicy>,
}

impl SsrConfigBuilder {
    /// Set the path to the JavaScript SSR bundle
    ///
    /// # Example
    /// ```rust
    /// use rusty_ssr::SsrConfig;
    ///
    /// let config = SsrConfig::builder()
    ///     .bundle_path("dist/ssr-bundle.js")
    ///     .build();
    /// ```
    pub fn bundle_path<P: Into<PathBuf>>(mut self, path: P) -> Self {
        self.bundle_path = Some(path.into());
        self
    }

    /// Set the number of V8 worker threads
    ///
    /// Default: number of CPU cores
    pub fn pool_size(mut self, size: usize) -> Self {
        self.pool_size = Some(size);
        self
    }

    /// Set the task queue capacity
    ///
    /// Default: 512
    pub fn queue_capacity(mut self, capacity: usize) -> Self {
        self.queue_capacity = Some(capacity);
        self
    }

    /// Enable CPU core pinning for V8 workers
    ///
    /// This can improve cache locality but may reduce flexibility
    pub fn pin_threads(mut self, pin: bool) -> Self {
        self.pin_threads = Some(pin);
        self
    }

    /// Set the maximum number of cached SSR results
    ///
    /// Default: 300
    pub fn cache_size(mut self, size: usize) -> Self {
        self.cache_size = Some(size);
        self
    }

    /// Set cache TTL (time-to-live)
    ///
    /// Default: 5 minutes. Use `None` for no expiration.
    pub fn cache_ttl(mut self, ttl: Option<Duration>) -> Self {
        self.cache_ttl = Some(ttl);
        self
    }

    /// Set cache TTL in seconds
    ///
    /// Convenience method. Use 0 for no expiration.
    pub fn cache_ttl_secs(mut self, secs: u64) -> Self {
        self.cache_ttl = Some(if secs > 0 {
            Some(Duration::from_secs(secs))
        } else {
            None
        });
        self
    }

    /// Set request timeout
    ///
    /// Default: 30 seconds. Use `None` for no timeout.
    pub fn request_timeout(mut self, timeout: Option<Duration>) -> Self {
        self.request_timeout = Some(timeout);
        self
    }

    /// Set the HTML template path for SSR output
    ///
    /// The template should contain `<!--ssr:outlet-->` where the rendered
    /// app HTML will be injected. Optionally use `<!--ssr:css-->` and
    /// `<!--ssr:scripts-->` for Vite manifest-based asset injection.
    ///
    /// # Example
    /// ```rust
    /// use rusty_ssr::SsrConfig;
    ///
    /// let config = SsrConfig::builder()
    ///     .bundle_path("dist-ssr/bundle.js")
    ///     .html_template("dist-web/index.html")
    ///     .assets_manifest("dist-web/.vite/manifest.json")
    ///     .build();
    /// ```
    pub fn html_template<P: Into<PathBuf>>(mut self, path: P) -> Self {
        self.html_template_path = Some(path.into());
        self
    }

    /// Set the Vite manifest.json path for asset resolution
    ///
    /// Used with `html_template` to inject hashed CSS and JS paths.
    pub fn assets_manifest<P: Into<PathBuf>>(mut self, path: P) -> Self {
        self.assets_manifest_path = Some(path.into());
        self
    }

    /// Set the name of the global render function
    ///
    /// Default: "renderPage"
    ///
    /// Your JS bundle should expose: `globalThis.{render_function}(url, data)`
    pub fn render_function<S: Into<String>>(mut self, name: S) -> Self {
        self.render_function = Some(name.into());
        self
    }

    /// Enable or disable the built-in browser polyfills (default: true)
    ///
    /// When `false`, the bundle is loaded verbatim with no polyfills
    /// prepended. Use this if your bundle already provides every global it
    /// needs — otherwise leave it on (the polyfills are non-clobbering, so
    /// a bundle can still override any of them).
    pub fn polyfills(mut self, enabled: bool) -> Self {
        self.polyfills = Some(enabled);
        self
    }

    /// Cache empty render results (default: true)
    ///
    /// Set to `false` to avoid caching renders that produce an empty
    /// string. With `cache_ttl = None`, a once-empty render would otherwise
    /// persist in the cache until manual invalidation.
    pub fn cache_empty(mut self, enabled: bool) -> Self {
        self.cache_empty = Some(enabled);
        self
    }

    /// Set a maximum V8 heap size per worker isolate, in megabytes
    ///
    /// A render exceeding the cap is terminated and returns an error
    /// (uncached) rather than aborting the process. Omit for no limit.
    /// Delete globals the bundle did not have at startup, before every render.
    ///
    /// See [`SsrConfig::seal_globals`]. Turn it on unless your bundle
    /// deliberately caches something on `globalThis` across renders.
    ///
    /// # Example
    /// ```rust
    /// use rusty_ssr::SsrConfig;
    ///
    /// let config = SsrConfig::builder().seal_globals(true).build();
    /// ```
    pub fn seal_globals(mut self, seal: bool) -> Self {
        self.seal_globals = Some(seal);
        self
    }

    /// Treat a render shorter than `bytes` as a failure.
    ///
    /// See [`SsrConfig::min_render_bytes`]. Without it, a bundle that swallows
    /// its own exception and returns `""` produces a blank page served with a
    /// 200, and nothing anywhere says so.
    ///
    /// # Example
    /// ```rust
    /// use rusty_ssr::SsrConfig;
    ///
    /// let config = SsrConfig::builder().min_render_bytes(200).build();
    /// ```
    pub fn min_render_bytes(mut self, bytes: usize) -> Self {
        self.min_render_bytes = Some(bytes);
        self
    }

    /// Set a maximum V8 heap size per worker isolate, in megabytes
    ///
    /// A render exceeding the cap is terminated and returns an error
    /// (uncached) rather than aborting the process. Omit for no limit.
    pub fn max_heap_mb(mut self, mb: usize) -> Self {
        self.max_heap_mb = Some(mb);
        self
    }

    /// Set a cache-key normalizer applied to the URL before lookup/insert
    ///
    /// Use it to collapse URLs that render identically onto one cache entry
    /// (e.g. strip `utm_*`/`fbclid`, sort query params). The original URL is
    /// still passed to the render function.
    ///
    /// # Example
    /// ```rust
    /// use rusty_ssr::SsrConfig;
    ///
    /// fn strip_query(url: &str) -> String {
    ///     url.split('?').next().unwrap_or(url).to_string()
    /// }
    ///
    /// let config = SsrConfig::builder()
    ///     .cache_key_normalizer(strip_query)
    ///     .build()
    ///     .unwrap();
    /// ```
    pub fn cache_key_normalizer(mut self, f: fn(&str) -> String) -> Self {
        self.cache_key_normalizer = Some(f);
        self
    }

    /// Set the policy for the **page** cache — finished documents, keyed by
    /// [`RenderKey`](crate::cache::RenderKey).
    ///
    /// Independent of `cache_size`/`cache_ttl`, which govern the older fragment
    /// cache. An application serving HTTP usually wants this one and can leave
    /// the fragment cache at [`CachePolicy::Off`](crate::cache::CachePolicy::Off)-equivalent by simply never
    /// calling `render`/`render_with_data`.
    ///
    /// # Example
    /// ```rust
    /// use rusty_ssr::{SsrConfig, cache::CachePolicy};
    /// use std::time::Duration;
    ///
    /// let config = SsrConfig::builder()
    ///     .page_cache(
    ///         CachePolicy::ttl(500, Duration::from_secs(300))
    ///             .stale_while_revalidate(Duration::from_secs(600)),
    ///     )
    ///     .build();
    /// ```
    #[cfg(feature = "cache")]
    pub fn page_cache(mut self, policy: crate::cache::CachePolicy) -> Self {
        self.page_cache = Some(policy);
        self
    }

    /// Build the configuration
    ///
    /// # Errors
    /// Returns `SsrError::Config` if any parameter is invalid:
    /// - `pool_size` must be > 0
    /// - `cache_size` must be > 0
    /// - `queue_capacity` must be > 0
    /// - `render_function` must be a valid JS identifier (alphanumeric, `_`, `.`)
    pub fn build(self) -> SsrResult<SsrConfig> {
        let default = SsrConfig::default();

        let config = SsrConfig {
            bundle_path: self.bundle_path.unwrap_or(default.bundle_path),
            pool_size: self.pool_size.unwrap_or(default.pool_size),
            queue_capacity: self.queue_capacity.unwrap_or(default.queue_capacity),
            pin_threads: self.pin_threads.unwrap_or(default.pin_threads),
            cache_size: self.cache_size.unwrap_or(default.cache_size),
            cache_ttl: self.cache_ttl.unwrap_or(default.cache_ttl),
            request_timeout: self.request_timeout.unwrap_or(default.request_timeout),
            render_function: self.render_function.unwrap_or(default.render_function),
            html_template_path: self.html_template_path,
            assets_manifest_path: self.assets_manifest_path,
            polyfills: self.polyfills.unwrap_or(default.polyfills),
            cache_empty: self.cache_empty.unwrap_or(default.cache_empty),
            max_heap_mb: self.max_heap_mb,
            min_render_bytes: self.min_render_bytes,
            seal_globals: self.seal_globals.unwrap_or(default.seal_globals),
            cache_key_normalizer: self.cache_key_normalizer,
            #[cfg(feature = "cache")]
            page_cache: self.page_cache.unwrap_or(default.page_cache),
        };

        if config.pool_size == 0 {
            return Err(SsrError::Config("pool_size must be > 0".into()));
        }
        if config.cache_size == 0 {
            return Err(SsrError::Config("cache_size must be > 0".into()));
        }
        if config.queue_capacity == 0 {
            return Err(SsrError::Config("queue_capacity must be > 0".into()));
        }
        if config.render_function.is_empty()
            || !config
                .render_function
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.')
        {
            return Err(SsrError::Config(format!(
                "render_function must be a valid JS identifier, got: {:?}",
                config.render_function
            )));
        }

        Ok(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = SsrConfig::default();
        assert_eq!(config.pool_size, num_cpus::get());
        assert_eq!(config.cache_size, 300);
        assert!(!config.pin_threads);
    }

    #[test]
    fn test_builder() {
        let config = SsrConfig::builder()
            .bundle_path("custom.js")
            .pool_size(4)
            .cache_size(100)
            .pin_threads(true)
            .build()
            .unwrap();

        assert_eq!(config.bundle_path, PathBuf::from("custom.js"));
        assert_eq!(config.pool_size, 4);
        assert_eq!(config.cache_size, 100);
        assert!(config.pin_threads);
    }

    #[test]
    fn test_zero_pool_size_rejected() {
        let result = SsrConfig::builder().pool_size(0).build();
        assert!(result.is_err());
    }

    #[test]
    fn test_zero_cache_size_rejected() {
        let result = SsrConfig::builder().cache_size(0).build();
        assert!(result.is_err());
    }

    #[test]
    fn test_empty_render_function_rejected() {
        let result = SsrConfig::builder().render_function("").build();
        assert!(result.is_err());
    }

    #[test]
    fn test_invalid_render_function_rejected() {
        let result = SsrConfig::builder()
            .render_function("foo; evil()")
            .build();
        assert!(result.is_err());
    }

    #[test]
    fn test_polyfills_and_cache_empty_defaults() {
        let config = SsrConfig::default();
        assert!(config.polyfills);
        assert!(config.cache_empty);
    }

    #[test]
    fn test_polyfills_and_cache_empty_overrides() {
        let config = SsrConfig::builder()
            .polyfills(false)
            .cache_empty(false)
            .build()
            .unwrap();
        assert!(!config.polyfills);
        assert!(!config.cache_empty);
    }

    #[test]
    fn test_dotted_render_function_ok() {
        let config = SsrConfig::builder()
            .render_function("module.renderPage")
            .build()
            .unwrap();
        assert_eq!(config.render_function, "module.renderPage");
    }
}
