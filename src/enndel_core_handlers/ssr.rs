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

    // Cache miss - рендерим через V8
    let html = state
        .v8_pool
        .render(url.clone())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Сохраняем в кэш (hot + cold)
    state.ssr_cache.insert(&url, Arc::from(html.as_str()));

    Ok(Html(html))
}
