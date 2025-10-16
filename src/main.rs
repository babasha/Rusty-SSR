mod enndel_core_brotli;
mod enndel_core_cache;
mod enndel_core_config;
mod enndel_core_handlers;
mod enndel_core_state;
mod enndel_core_v8pool;

use axum::{
    middleware,
    routing::{get, post},
    Router,
};
use std::net::SocketAddr;
use std::sync::Arc;
use tower_http::{services::ServeDir, trace::TraceLayer};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use enndel_core_cache::{ProductCache, SSRCache};
use enndel_core_config::ServerConfig;
use enndel_core_handlers::{
    api_proxy_handler, invalidate_products_handler, lazy_images_handler, metrics_handler,
    metrics_prometheus_handler, ssr_handler,
};
use enndel_core_state::AppState;
use enndel_core_v8pool::{AdaptivePoolConfig, AdaptiveV8Pool};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Инициализация логирования
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .init();

    // Загружаем конфигурацию
    let config = ServerConfig::default();
    tracing::info!("🎯 Using all {} available CPU threads", config.v8_pool_size);

    // Инициализируем V8 pool (загружаем SSR бандл)
    enndel_core_v8pool::init();

    let worker_threads = config.v8_pool_size.max(1);
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(worker_threads)
        .max_blocking_threads(config.tokio_max_blocking_threads.max(worker_threads))
        .enable_io()
        .enable_time()
        .build()?;

    runtime.block_on(run(config))?;
    Ok(())
}

async fn run(config: ServerConfig) -> Result<(), Box<dyn std::error::Error>> {
    // Создаём V8 thread pool
    let pool_config = AdaptivePoolConfig {
        num_threads: config.v8_pool_size,
        queue_capacity: config.v8_queue_capacity,
        pin_threads: config.v8_pin_threads,
    };
    let v8_pool = AdaptiveV8Pool::new(pool_config);

    // Создаём SSR cache (300 страниц в cold cache)
    let ssr_cache = SSRCache::new(300);

    // Создаём Product cache
    let product_cache = ProductCache::with_options(
        config.product_api_base.clone(),
        config.product_lazy_cache_capacity,
    );

    // Предзагрузка критичных данных
    product_cache.preload().await;

    // Создаём состояние приложения
    let app_state = Arc::new(AppState::new(v8_pool, ssr_cache, product_cache));

    // Создаём роутер
    let app = Router::new()
        // API для ленивой загрузки изображений
        .route("/api/products/lazy/:id", get(lazy_images_handler))
        // API прокси (должен быть первым, чтобы не перехватывался SSR)
        .route("/api/*path", get(api_proxy_handler))
        // Метрики кэшей
        .route("/internal/metrics/cache", get(metrics_handler))
        .route(
            "/internal/metrics/cache/prometheus",
            get(metrics_prometheus_handler),
        )
        .route(
            "/internal/cache/products/invalidate",
            post(invalidate_products_handler),
        )
        // Статические файлы с Brotli middleware
        .nest_service(
            "/assets",
            ServeDir::new("../EnndelClient/dist/client/assets"),
        )
        .layer(middleware::from_fn(enndel_core_brotli::brotli_static))
        .layer(middleware::from_fn(
            enndel_core_brotli::brotli_compress_html,
        ))
        // SSR рендеринг (последний, catch-all для всех остальных путей)
        .fallback(ssr_handler)
        .with_state(app_state)
        // Трейсинг
        .layer(TraceLayer::new_for_http());

    // Запускаем сервер
    let addr: SocketAddr = config.bind_address().parse()?;
    let listener = build_listener(addr, config.tcp_backlog)?;

    tracing::info!("🦀 Rust server running on http://localhost:{}", config.port);

    axum::serve(listener, app).await?;
    Ok(())
}

fn build_listener(addr: SocketAddr, backlog: u32) -> std::io::Result<tokio::net::TcpListener> {
    use tokio::net::TcpSocket;

    let socket = if addr.is_ipv6() {
        TcpSocket::new_v6()?
    } else {
        TcpSocket::new_v4()?
    };
    socket.set_reuseaddr(true)?;
    #[cfg(target_family = "unix")]
    {
        socket.set_reuseport(true)?;
    }
    socket.bind(addr)?;
    socket.listen(backlog)
}
