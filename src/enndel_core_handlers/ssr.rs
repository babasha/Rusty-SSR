use axum::{
    extract::State,
    http::{StatusCode, Uri},
    response::Html,
};
use std::sync::Arc;

use crate::enndel_core_state::AppState;

/// SSR handler - рендерит страницы через V8 pool
pub async fn ssr_handler(
    State(state): State<Arc<AppState>>,
    uri: Uri,
) -> Result<Html<String>, StatusCode> {
    let url = uri
        .path_and_query()
        .map(|pq| pq.as_str().to_string())
        .unwrap_or_else(|| uri.path().to_string());

    // Получаем ТОЛЬКО критичные данные (текст для SEO)
    let (critical_products, version) =
        state.product_cache.get_critical_all().await.map_err(|e| {
            tracing::error!("Failed to get critical products: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    // Проверяем кэш с версионированием
    if let Some(cached_html) = state.ssr_cache.try_get_versioned(&url, version) {
        return Ok(Html(cached_html.to_string()));
    }

    // Сериализуем только критичные данные (извлекаем из Arc)
    let products_data: Vec<_> = critical_products.iter().map(|p| &**p).collect();
    let products_json = serde_json::to_string(&products_data).unwrap_or_else(|_| "[]".to_string());

    tracing::debug!(
        "🎨 Rendering SSR for {} with {} products (version: 0x{:X})",
        url,
        critical_products.len(),
        version
    );

    // Cache miss - рендерим через V8 с критичными данными
    let html = state
        .v8_pool
        .render_with_data(url.clone(), products_json)
        .await
        .map_err(|e| {
            tracing::error!("SSR render error: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    // Сохраняем в кэш с версией
    state
        .ssr_cache
        .insert_versioned(&url, Arc::from(html.as_str()), version);

    Ok(Html(html))
}
