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

    /// Request timeout for enqueueing render jobs
    pub request_timeout: Option<Duration>,

    /// Name of the global render function in JS bundle
    pub render_function: String,
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

    /// Set the name of the global render function
    ///
    /// Default: "renderPage"
    ///
    /// Your JS bundle should expose: `globalThis.{render_function}(url, data)`
    pub fn render_function<S: Into<String>>(mut self, name: S) -> Self {
        self.render_function = Some(name.into());
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
    fn test_dotted_render_function_ok() {
        let config = SsrConfig::builder()
            .render_function("module.renderPage")
            .build()
            .unwrap();
        assert_eq!(config.render_function, "module.renderPage");
    }
}
