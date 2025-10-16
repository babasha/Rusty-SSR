use axum::{extract::State, http::StatusCode};
use std::sync::Arc;

use crate::enndel_core_state::AppState;

/// Инвалидация кэша критичных данных продуктов
pub async fn invalidate_products_handler(State(state): State<Arc<AppState>>) -> StatusCode {
    state.product_cache.invalidate_critical();
    StatusCode::NO_CONTENT
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::enndel_core_cache::{product_cache::ProductCache, SSRCache};
    use crate::enndel_core_state::AppState;
    use crate::enndel_core_v8pool::AdaptiveV8Pool;
    use axum::{http::Request, routing::post, Router};
    use httpmock::prelude::*;
    use std::sync::Arc;
    use tower::util::ServiceExt;

    #[tokio::test]
    async fn invalidate_products_clears_critical_cache() {
        let server = MockServer::start_async().await;
        let _products_mock = server
            .mock_async(|when, then| {
                when.method(GET).path("/api/products");
                then.status(200).json_body(serde_json::json!({
                    "products": [
                        {
                            "id": 1,
                            "name": {"ru": "Тест"},
                            "price": 10.0,
                            "unit": "kg",
                            "step": 1.0
                        }
                    ]
                }));
            })
            .await;

        let product_cache = ProductCache::with_options(
            format!("{}/api", server.base_url()),
            ProductCache::DEFAULT_LAZY_CACHE_CAPACITY,
        );

        // Прогреваем кэш, чтобы была версия
        let (_data, version) = product_cache.get_critical_all().await.unwrap();
        assert!(version > 0);
        assert!(product_cache.get_version().is_some());

        let app_state = Arc::new(AppState::new(
            AdaptiveV8Pool::new_stub(),
            SSRCache::new(10),
            product_cache,
        ));

        let router = Router::new()
            .route(
                "/internal/cache/products/invalidate",
                post(invalidate_products_handler),
            )
            .with_state(Arc::clone(&app_state));

        let response = router
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/internal/cache/products/invalidate")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        assert!(app_state.product_cache.get_version().is_none());
    }
}
