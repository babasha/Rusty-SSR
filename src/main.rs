mod enndel_core_brotli;
mod enndel_core_cache;
mod enndel_core_config;
mod enndel_core_handlers;
mod enndel_core_state;
mod enndel_core_v8pool;

use axum::{middleware, routing::get, Router};
use std::sync::Arc;
use tower_http::{services::ServeDir, trace::TraceLayer};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use enndel_core_cache::SSRCache;
use enndel_core_config::ServerConfig;
use enndel_core_handlers::{api_proxy_handler, ssr_handler};
use enndel_core_state::AppState;
use enndel_core_v8pool::{AdaptivePoolConfig, AdaptiveV8Pool};

#[tokio::main]
async fn main() {
    // Инициализация логирования
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .init();

    // Загружаем конфигурацию
    let config = ServerConfig::default();
    tracing::info!("🎯 Using all {} available CPU threads", config.v8_pool_size);

    // Инициализируем V8 pool (загружаем SSR бандл)
    enndel_core_v8pool::init();

    // Создаём V8 thread pool
    let pool_config = AdaptivePoolConfig::default();
    let v8_pool = AdaptiveV8Pool::new(pool_config);

    // Создаём SSR cache (300 страниц в cold cache)
    let ssr_cache = SSRCache::new(300);

    // Создаём состояние приложения
    let app_state = Arc::new(AppState::new(v8_pool, ssr_cache));

    // Создаём роутер
    let app = Router::new()
        // API прокси (должен быть первым, чтобы не перехватывался SSR)
        .route("/api/*path", get(api_proxy_handler))
        // Статические файлы с Brotli middleware
        .nest_service("/assets", ServeDir::new("../EnndelClient/dist/client/assets"))
        .layer(middleware::from_fn(enndel_core_brotli::brotli_static))
        // SSR рендеринг (последний, catch-all для всех остальных путей)
        .fallback(ssr_handler)
        .with_state(app_state)
        // Трейсинг
        .layer(TraceLayer::new_for_http());

    // Запускаем сервер
    let bind_address = config.bind_address();
    let listener = tokio::net::TcpListener::bind(&bind_address)
        .await
        .unwrap();

    tracing::info!("🦀 Rust server running on http://localhost:{}", config.port);

    axum::serve(listener, app).await.unwrap();
}
