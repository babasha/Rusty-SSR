use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Json,
};
use std::sync::Arc;

use crate::enndel_core_cache::product_cache::LazyProductData;
use crate::enndel_core_state::AppState;

/// Handler для ленивой загрузки изображений продукта
pub async fn lazy_images_handler(
    State(state): State<Arc<AppState>>,
    Path(product_id): Path<i32>,
) -> Result<Json<LazyProductData>, StatusCode> {
    tracing::debug!("📸 Lazy loading images for product {}", product_id);

    state
        .product_cache
        .get_lazy(product_id)
        .await
        .map(|data| Json((*data).clone()))
        .ok_or_else(|| {
            tracing::warn!("Product {} not found for lazy loading", product_id);
            StatusCode::NOT_FOUND
        })
}
