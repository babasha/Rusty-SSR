use crate::enndel_core_cache::SSRCache;
use crate::enndel_core_v8pool::AdaptiveV8Pool;

/// Состояние приложения
pub struct AppState {
    pub v8_pool: AdaptiveV8Pool,
    pub ssr_cache: SSRCache,
}

impl AppState {
    /// Создаёт новое состояние приложения
    pub fn new(v8_pool: AdaptiveV8Pool, ssr_cache: SSRCache) -> Self {
        Self { v8_pool, ssr_cache }
    }
}
