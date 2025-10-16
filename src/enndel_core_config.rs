use crate::enndel_core_cache::product_cache::ProductCache;
use std::env;

/// Конфигурация сервера
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub v8_pool_size: usize,
    pub product_api_base: String,
    pub product_lazy_cache_capacity: usize,
    pub v8_queue_capacity: usize,
    pub v8_pin_threads: bool,
    pub tcp_backlog: u32,
    pub tokio_max_blocking_threads: usize,
}

impl ServerConfig {
    /// Создаёт конфигурацию по умолчанию
    pub fn default() -> Self {
        let num_cpus = num_cpus::get();
        let num_physical = num_cpus::get_physical();

        tracing::info!(
            "🖥️  Detected {} logical CPUs ({} physical cores)",
            num_cpus,
            num_physical
        );

        Self {
            host: "0.0.0.0".to_string(),
            port: 3000,
            v8_pool_size: num_cpus,
            product_api_base: env::var("PRODUCT_API_BASE")
                .unwrap_or_else(|_| ProductCache::DEFAULT_API_BASE.to_string()),
            product_lazy_cache_capacity: env::var("PRODUCT_LAZY_CACHE_CAPACITY")
                .ok()
                .and_then(|value| value.parse::<usize>().ok())
                .filter(|capacity| *capacity > 0)
                .unwrap_or(ProductCache::DEFAULT_LAZY_CACHE_CAPACITY),
            v8_queue_capacity: env::var("V8_QUEUE_CAPACITY")
                .ok()
                .and_then(|value| value.parse::<usize>().ok())
                .filter(|capacity| *capacity > 0)
                .unwrap_or(512),
            v8_pin_threads: env::var("V8_PIN_THREADS")
                .ok()
                .map(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
                .unwrap_or(false),
            tcp_backlog: env::var("TCP_BACKLOG")
                .ok()
                .and_then(|value| value.parse::<u32>().ok())
                .filter(|backlog| *backlog > 0)
                .unwrap_or(1024),
            tokio_max_blocking_threads: env::var("TOKIO_MAX_BLOCKING_THREADS")
                .ok()
                .and_then(|value| value.parse::<usize>().ok())
                .filter(|threads| *threads > 0)
                .unwrap_or(num_cpus * 2),
        }
    }

    /// Возвращает адрес для bind
    pub fn bind_address(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    #[test]
    fn test_default_config_uses_env_overrides() {
        let _guard = env_lock().lock().unwrap();

        std::env::set_var("PRODUCT_API_BASE", "https://mocked.example/api");
        std::env::set_var("PRODUCT_LAZY_CACHE_CAPACITY", "512");
        std::env::set_var("V8_QUEUE_CAPACITY", "1024");
        std::env::set_var("V8_PIN_THREADS", "true");
        std::env::set_var("TCP_BACKLOG", "2048");
        std::env::set_var("TOKIO_MAX_BLOCKING_THREADS", "321");

        let cfg = ServerConfig::default();

        assert_eq!(cfg.product_api_base, "https://mocked.example/api");
        assert_eq!(cfg.product_lazy_cache_capacity, 512);
        assert_eq!(cfg.v8_queue_capacity, 1024);
        assert!(cfg.v8_pin_threads);
        assert_eq!(cfg.tcp_backlog, 2048);
        assert_eq!(cfg.tokio_max_blocking_threads, 321);

        std::env::remove_var("PRODUCT_API_BASE");
        std::env::remove_var("PRODUCT_LAZY_CACHE_CAPACITY");
        std::env::remove_var("V8_QUEUE_CAPACITY");
        std::env::remove_var("V8_PIN_THREADS");
        std::env::remove_var("TCP_BACKLOG");
        std::env::remove_var("TOKIO_MAX_BLOCKING_THREADS");
        std::env::remove_var("V8_QUEUE_CAPACITY");
        std::env::remove_var("V8_PIN_THREADS");
        std::env::remove_var("TCP_BACKLOG");
        std::env::remove_var("TOKIO_MAX_BLOCKING_THREADS");
    }

    #[test]
    fn test_default_config_fallbacks() {
        let _guard = env_lock().lock().unwrap();

        std::env::remove_var("PRODUCT_API_BASE");
        std::env::remove_var("PRODUCT_LAZY_CACHE_CAPACITY");
        std::env::remove_var("V8_QUEUE_CAPACITY");
        std::env::remove_var("V8_PIN_THREADS");
        std::env::remove_var("TCP_BACKLOG");
        std::env::remove_var("TOKIO_MAX_BLOCKING_THREADS");

        let cfg = ServerConfig::default();

        assert_eq!(cfg.product_api_base, ProductCache::DEFAULT_API_BASE);
        assert_eq!(
            cfg.product_lazy_cache_capacity,
            ProductCache::DEFAULT_LAZY_CACHE_CAPACITY
        );
        assert_eq!(cfg.v8_queue_capacity, 512);
        assert!(!cfg.v8_pin_threads);
        assert_eq!(cfg.tcp_backlog, 1024);
        assert_eq!(cfg.tokio_max_blocking_threads, num_cpus::get() * 2);
    }
}
