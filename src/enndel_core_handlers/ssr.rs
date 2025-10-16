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
    let url = uri.path().to_string();

    // Проверяем кэш (hot + cold with auto-promotion)
    if let Some(cached_html) = state.ssr_cache.try_get(&url) {
        return Ok(Html(cached_html.to_string()));
    }

    // Загружаем продукты с API для SSR
    let products_json = fetch_products().await.unwrap_or_else(|e| {
        tracing::warn!("Failed to fetch products for SSR: {}", e);
        "[]".to_string()
    });

    // Cache miss - рендерим через V8 с данными продуктов
    let html = state
        .v8_pool
        .render_with_data(url.clone(), products_json)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Сохраняем в кэш (hot + cold)
    state.ssr_cache.insert(&url, Arc::from(html.as_str()));

    Ok(Html(html))
}

/// Загружает продукты с API
async fn fetch_products() -> Result<String, Box<dyn std::error::Error>> {
    let response = reqwest::get("https://enddel.com/api/products")
        .await?
        .json::<serde_json::Value>()
        .await?;

    Ok(serde_json::to_string(&response)?)
}
