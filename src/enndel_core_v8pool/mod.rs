// V8 Pool - Пул V8 изолятов для SSR
// Модульная архитектура для улучшенной поддерживаемости

mod adaptive_pool;
mod bundle;
mod renderer;
mod runtime;

pub use adaptive_pool::{AdaptivePoolConfig, AdaptiveV8Pool};

/// Инициализирует V8 пул (загружает SSR бандл)
pub fn init() {
    bundle::init_bundle();
}
