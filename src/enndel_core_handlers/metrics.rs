use axum::{
    body::Body,
    extract::State,
    http::{header, StatusCode},
    response::{Json, Response},
};
use serde::Serialize;
use std::sync::Arc;

use crate::enndel_core_cache::{ProductCacheMetricsSnapshot, SSRCacheMetricsSnapshot};
use crate::enndel_core_state::AppState;

#[derive(Serialize)]
pub struct CacheMetricsResponse {
    pub product_cache: ProductCacheMetricsSnapshot,
    pub ssr_cache: SSRCacheMetricsSnapshot,
}

pub async fn metrics_handler(State(state): State<Arc<AppState>>) -> Json<CacheMetricsResponse> {
    let product_cache = state.product_cache.metrics();
    let ssr_cache = state.ssr_cache.metrics();

    Json(CacheMetricsResponse {
        product_cache,
        ssr_cache,
    })
}

pub async fn metrics_prometheus_handler(State(state): State<Arc<AppState>>) -> Response {
    let product_cache = state.product_cache.metrics();
    let ssr_cache = state.ssr_cache.metrics();
    let body = format_prometheus_metrics(&product_cache, &ssr_cache);

    Response::builder()
        .status(StatusCode::OK)
        .header(
            header::CONTENT_TYPE,
            "text/plain; version=0.0.4; charset=utf-8",
        )
        .body(Body::from(body))
        .expect("failed to build metrics response")
}

fn format_prometheus_metrics(
    product: &ProductCacheMetricsSnapshot,
    ssr: &SSRCacheMetricsSnapshot,
) -> String {
    use std::fmt::Write;

    let mut out = String::with_capacity(1024);

    writeln!(
        &mut out,
        "# HELP enddel_product_cache_lazy_hits_total Total cache hits for lazy product data"
    )
    .unwrap();
    writeln!(
        &mut out,
        "# TYPE enddel_product_cache_lazy_hits_total counter"
    )
    .unwrap();
    writeln!(
        &mut out,
        "enddel_product_cache_lazy_hits_total {}",
        product.lazy_hits
    )
    .unwrap();

    writeln!(
        &mut out,
        "# HELP enddel_product_cache_lazy_misses_total Total cache misses for lazy product data"
    )
    .unwrap();
    writeln!(
        &mut out,
        "# TYPE enddel_product_cache_lazy_misses_total counter"
    )
    .unwrap();
    writeln!(
        &mut out,
        "enddel_product_cache_lazy_misses_total {}",
        product.lazy_misses
    )
    .unwrap();

    writeln!(
        &mut out,
        "# HELP enddel_product_cache_lazy_fetch_success_total Successful lazy fetches"
    )
    .unwrap();
    writeln!(
        &mut out,
        "# TYPE enddel_product_cache_lazy_fetch_success_total counter"
    )
    .unwrap();
    writeln!(
        &mut out,
        "enddel_product_cache_lazy_fetch_success_total {}",
        product.lazy_fetch_success
    )
    .unwrap();

    writeln!(
        &mut out,
        "# HELP enddel_product_cache_lazy_fetch_errors_total Failed lazy fetches"
    )
    .unwrap();
    writeln!(
        &mut out,
        "# TYPE enddel_product_cache_lazy_fetch_errors_total counter"
    )
    .unwrap();
    writeln!(
        &mut out,
        "enddel_product_cache_lazy_fetch_errors_total {}",
        product.lazy_fetch_errors
    )
    .unwrap();

    writeln!(
        &mut out,
        "# HELP enddel_product_cache_lazy_evictions_total Lazy cache evictions"
    )
    .unwrap();
    writeln!(
        &mut out,
        "# TYPE enddel_product_cache_lazy_evictions_total counter"
    )
    .unwrap();
    writeln!(
        &mut out,
        "enddel_product_cache_lazy_evictions_total {}",
        product.lazy_evictions
    )
    .unwrap();

    writeln!(
        &mut out,
        "# HELP enddel_product_cache_lazy_last_fetch_seconds Duration of the last lazy fetch in seconds"
    )
    .unwrap();
    writeln!(
        &mut out,
        "# TYPE enddel_product_cache_lazy_last_fetch_seconds gauge"
    )
    .unwrap();
    writeln!(
        &mut out,
        "enddel_product_cache_lazy_last_fetch_seconds {:.6}",
        nanos_to_seconds(product.lazy_last_fetch_ns)
    )
    .unwrap();

    writeln!(
        &mut out,
        "# HELP enddel_product_cache_lazy_cache_size Number of entries in lazy cache"
    )
    .unwrap();
    writeln!(
        &mut out,
        "# TYPE enddel_product_cache_lazy_cache_size gauge"
    )
    .unwrap();
    writeln!(
        &mut out,
        "enddel_product_cache_lazy_cache_size {}",
        product.lazy_cache_len
    )
    .unwrap();

    writeln!(
        &mut out,
        "# HELP enddel_product_cache_lazy_cache_capacity Configured lazy cache capacity"
    )
    .unwrap();
    writeln!(
        &mut out,
        "# TYPE enddel_product_cache_lazy_cache_capacity gauge"
    )
    .unwrap();
    writeln!(
        &mut out,
        "enddel_product_cache_lazy_cache_capacity {}",
        product.lazy_cache_capacity
    )
    .unwrap();

    writeln!(
        &mut out,
        "# HELP enddel_product_cache_critical_hits_total Critical data cache hits"
    )
    .unwrap();
    writeln!(
        &mut out,
        "# TYPE enddel_product_cache_critical_hits_total counter"
    )
    .unwrap();
    writeln!(
        &mut out,
        "enddel_product_cache_critical_hits_total {}",
        product.critical_hits
    )
    .unwrap();

    writeln!(
        &mut out,
        "# HELP enddel_product_cache_critical_misses_total Critical data cache misses"
    )
    .unwrap();
    writeln!(
        &mut out,
        "# TYPE enddel_product_cache_critical_misses_total counter"
    )
    .unwrap();
    writeln!(
        &mut out,
        "enddel_product_cache_critical_misses_total {}",
        product.critical_misses
    )
    .unwrap();

    writeln!(
        &mut out,
        "# HELP enddel_product_cache_critical_refresh_success_total Successful critical refreshes"
    )
    .unwrap();
    writeln!(
        &mut out,
        "# TYPE enddel_product_cache_critical_refresh_success_total counter"
    )
    .unwrap();
    writeln!(
        &mut out,
        "enddel_product_cache_critical_refresh_success_total {}",
        product.critical_refresh_success
    )
    .unwrap();

    writeln!(
        &mut out,
        "# HELP enddel_product_cache_critical_refresh_errors_total Failed critical refreshes"
    )
    .unwrap();
    writeln!(
        &mut out,
        "# TYPE enddel_product_cache_critical_refresh_errors_total counter"
    )
    .unwrap();
    writeln!(
        &mut out,
        "enddel_product_cache_critical_refresh_errors_total {}",
        product.critical_refresh_errors
    )
    .unwrap();

    writeln!(
        &mut out,
        "# HELP enddel_product_cache_critical_last_refresh_seconds Duration of the last critical refresh in seconds"
    )
    .unwrap();
    writeln!(
        &mut out,
        "# TYPE enddel_product_cache_critical_last_refresh_seconds gauge"
    )
    .unwrap();
    writeln!(
        &mut out,
        "enddel_product_cache_critical_last_refresh_seconds {:.6}",
        nanos_to_seconds(product.critical_last_refresh_ns)
    )
    .unwrap();

    writeln!(
        &mut out,
        "# HELP enddel_product_cache_critical_cached_entries Number of cached critical entries"
    )
    .unwrap();
    writeln!(
        &mut out,
        "# TYPE enddel_product_cache_critical_cached_entries gauge"
    )
    .unwrap();
    writeln!(
        &mut out,
        "enddel_product_cache_critical_cached_entries {}",
        product.critical_cached_entries
    )
    .unwrap();

    writeln!(
        &mut out,
        "# HELP enddel_product_cache_critical_cache_age_seconds Age of cached critical data in seconds"
    )
    .unwrap();
    writeln!(
        &mut out,
        "# TYPE enddel_product_cache_critical_cache_age_seconds gauge"
    )
    .unwrap();
    let cache_age_seconds = product
        .critical_cache_age_ms
        .map(|ms| ms as f64 / 1_000.0)
        .unwrap_or(0.0);
    writeln!(
        &mut out,
        "enddel_product_cache_critical_cache_age_seconds {:.3}",
        cache_age_seconds
    )
    .unwrap();

    append_ssr_metrics(&mut out, ssr);

    out
}

fn append_ssr_metrics(buffer: &mut String, ssr: &SSRCacheMetricsSnapshot) {
    use std::fmt::Write;

    writeln!(
        buffer,
        "# HELP enddel_ssr_cache_lookups_total Total cache lookups for SSR cache"
    )
    .unwrap();
    writeln!(buffer, "# TYPE enddel_ssr_cache_lookups_total counter").unwrap();
    writeln!(buffer, "enddel_ssr_cache_lookups_total {}", ssr.lookups).unwrap();

    writeln!(
        buffer,
        "# HELP enddel_ssr_cache_hot_hits_total Hot cache hits for SSR cache"
    )
    .unwrap();
    writeln!(buffer, "# TYPE enddel_ssr_cache_hot_hits_total counter").unwrap();
    writeln!(buffer, "enddel_ssr_cache_hot_hits_total {}", ssr.hot_hits).unwrap();

    writeln!(
        buffer,
        "# HELP enddel_ssr_cache_cold_hits_total Cold cache hits for SSR cache"
    )
    .unwrap();
    writeln!(buffer, "# TYPE enddel_ssr_cache_cold_hits_total counter").unwrap();
    writeln!(buffer, "enddel_ssr_cache_cold_hits_total {}", ssr.cold_hits).unwrap();

    writeln!(
        buffer,
        "# HELP enddel_ssr_cache_misses_total Cache misses for SSR cache"
    )
    .unwrap();
    writeln!(buffer, "# TYPE enddel_ssr_cache_misses_total counter").unwrap();
    writeln!(buffer, "enddel_ssr_cache_misses_total {}", ssr.misses).unwrap();

    writeln!(
        buffer,
        "# HELP enddel_ssr_cache_promotions_total Promotions from cold cache to hot cache"
    )
    .unwrap();
    writeln!(buffer, "# TYPE enddel_ssr_cache_promotions_total counter").unwrap();
    writeln!(
        buffer,
        "enddel_ssr_cache_promotions_total {}",
        ssr.promotions
    )
    .unwrap();

    writeln!(
        buffer,
        "# HELP enddel_ssr_cache_renders_total Rendered SSR responses"
    )
    .unwrap();
    writeln!(buffer, "# TYPE enddel_ssr_cache_renders_total counter").unwrap();
    writeln!(buffer, "enddel_ssr_cache_renders_total {}", ssr.renders).unwrap();

    writeln!(
        buffer,
        "# HELP enddel_ssr_cache_render_errors_total SSR rendering errors"
    )
    .unwrap();
    writeln!(
        buffer,
        "# TYPE enddel_ssr_cache_render_errors_total counter"
    )
    .unwrap();
    writeln!(
        buffer,
        "enddel_ssr_cache_render_errors_total {}",
        ssr.render_errors
    )
    .unwrap();

    writeln!(
        buffer,
        "# HELP enddel_ssr_cache_cold_insertions_total Cold cache insertions"
    )
    .unwrap();
    writeln!(
        buffer,
        "# TYPE enddel_ssr_cache_cold_insertions_total counter"
    )
    .unwrap();
    writeln!(
        buffer,
        "enddel_ssr_cache_cold_insertions_total {}",
        ssr.cold_insertions
    )
    .unwrap();

    writeln!(
        buffer,
        "# HELP enddel_ssr_cache_cold_evictions_total Cold cache evictions"
    )
    .unwrap();
    writeln!(
        buffer,
        "# TYPE enddel_ssr_cache_cold_evictions_total counter"
    )
    .unwrap();
    writeln!(
        buffer,
        "enddel_ssr_cache_cold_evictions_total {}",
        ssr.cold_evictions
    )
    .unwrap();

    writeln!(
        buffer,
        "# HELP enddel_ssr_cache_hot_insertions_total Hot cache insertions"
    )
    .unwrap();
    writeln!(
        buffer,
        "# TYPE enddel_ssr_cache_hot_insertions_total counter"
    )
    .unwrap();
    writeln!(
        buffer,
        "enddel_ssr_cache_hot_insertions_total {}",
        ssr.hot_insertions
    )
    .unwrap();

    writeln!(
        buffer,
        "# HELP enddel_ssr_cache_last_render_seconds Duration of the last SSR render in seconds"
    )
    .unwrap();
    writeln!(buffer, "# TYPE enddel_ssr_cache_last_render_seconds gauge").unwrap();
    writeln!(
        buffer,
        "enddel_ssr_cache_last_render_seconds {:.6}",
        nanos_to_seconds(ssr.last_render_ns)
    )
    .unwrap();

    writeln!(
        buffer,
        "# HELP enddel_ssr_cache_cold_cache_size Number of entries in cold cache"
    )
    .unwrap();
    writeln!(buffer, "# TYPE enddel_ssr_cache_cold_cache_size gauge").unwrap();
    writeln!(
        buffer,
        "enddel_ssr_cache_cold_cache_size {}",
        ssr.cold_cache_size
    )
    .unwrap();

    writeln!(
        buffer,
        "# HELP enddel_ssr_cache_cold_cache_capacity Configured cold cache capacity"
    )
    .unwrap();
    writeln!(buffer, "# TYPE enddel_ssr_cache_cold_cache_capacity gauge").unwrap();
    writeln!(
        buffer,
        "enddel_ssr_cache_cold_cache_capacity {}",
        ssr.cold_cache_capacity
    )
    .unwrap();
}

fn nanos_to_seconds(nanos: u64) -> f64 {
    nanos as f64 / 1_000_000_000.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::enndel_core_cache::{ProductCache, SSRCache};
    use crate::enndel_core_state::AppState;
    use crate::enndel_core_v8pool::AdaptiveV8Pool;
    use axum::{http::Request, Router};
    use http_body_util::BodyExt;
    use httpmock::prelude::*;
    use serde_json::Value;
    use std::sync::Arc;
    use tower::util::ServiceExt;

    #[test]
    fn test_format_prometheus_metrics_contains_core_lines() {
        let product = ProductCacheMetricsSnapshot {
            lazy_hits: 10,
            lazy_misses: 5,
            lazy_fetch_success: 20,
            lazy_fetch_errors: 1,
            lazy_evictions: 3,
            lazy_last_fetch_ns: 500_000_000,
            lazy_cache_len: 128,
            lazy_cache_capacity: 256,
            critical_hits: 7,
            critical_misses: 2,
            critical_refresh_success: 4,
            critical_refresh_errors: 1,
            critical_last_refresh_ns: 1_000_000_000,
            critical_cached_entries: 42,
            critical_cache_age_ms: Some(1500),
        };

        let ssr = SSRCacheMetricsSnapshot {
            lookups: 100,
            hot_hits: 60,
            cold_hits: 30,
            misses: 10,
            promotions: 25,
            renders: 12,
            render_errors: 1,
            cold_insertions: 12,
            cold_evictions: 5,
            hot_insertions: 12,
            last_render_ns: 2_500_000,
            cold_cache_size: 290,
            cold_cache_capacity: 300,
        };

        let payload = format_prometheus_metrics(&product, &ssr);
        assert!(payload.contains("enddel_product_cache_lazy_hits_total 10"));
        assert!(payload.contains("enddel_product_cache_lazy_cache_capacity 256"));
        assert!(payload.contains("enddel_ssr_cache_renders_total 12"));
        assert!(payload.contains("# TYPE enddel_ssr_cache_last_render_seconds gauge"));
    }

    #[tokio::test]
    async fn test_metrics_handlers_http_flow() {
        let server = MockServer::start_async().await;

        let products_body = serde_json::json!({
            "products": [
                {
                    "id": 1,
                    "name": {"ru": "Товар 1"},
                    "price": 10.0,
                    "unit": "kg",
                    "step": 1.0,
                    "stock_quantity": 3,
                    "category_id": 2,
                    "vendor_id": 3,
                    "slug": "product-1"
                }
            ]
        });

        let lazy_body = serde_json::json!({
            "id": 1,
            "image_url": "https://cdn.example.com/product-1.jpg",
            "images": ["https://cdn.example.com/product-1.jpg"]
        });

        let _products_mock = server
            .mock_async(|when, then| {
                when.method(GET).path("/api/products");
                then.status(200).json_body(products_body.clone());
            })
            .await;

        let lazy_mock = server
            .mock_async(move |when, then| {
                when.method(GET)
                    .path_matches(Regex::new(r"^/api/products/\d+$").unwrap());
                then.status(200).json_body(lazy_body.clone());
            })
            .await;

        let product_cache = ProductCache::with_options(
            format!("{}/api", server.base_url()),
            ProductCache::DEFAULT_LAZY_CACHE_CAPACITY,
        );

        // Прогреваем кэш, чтобы метрики были ненулевыми
        product_cache.get_critical_all().await.unwrap();
        product_cache.get_lazy(1).await.unwrap();

        let app_state = Arc::new(AppState::new(
            AdaptiveV8Pool::new_stub(),
            SSRCache::new(32),
            product_cache,
        ));

        let router = Router::new()
            .route(
                "/internal/metrics/cache",
                axum::routing::get(metrics_handler),
            )
            .route(
                "/internal/metrics/cache/prometheus",
                axum::routing::get(metrics_prometheus_handler),
            )
            .with_state(app_state);

        // JSON endpoint
        let response = router
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/internal/metrics/cache")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: Value = serde_json::from_slice(&body).unwrap();
        assert!(json
            .get("product_cache")
            .and_then(|pc| pc.get("lazy_hits"))
            .and_then(Value::as_u64)
            .is_some());

        // Prometheus endpoint
        let response = router
            .oneshot(
                Request::builder()
                    .uri("/internal/metrics/cache/prometheus")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let headers = response.headers();
        assert_eq!(
            headers.get(header::CONTENT_TYPE).unwrap(),
            "text/plain; version=0.0.4; charset=utf-8"
        );
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let text = String::from_utf8(body.to_vec()).unwrap();
        assert!(text.contains("enddel_product_cache_lazy_hits_total"));
        assert!(text.contains("enddel_ssr_cache_lookups_total"));

        assert!(lazy_mock.hits_async().await >= 1);
    }
}
