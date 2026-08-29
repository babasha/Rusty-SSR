//! An HTTP server around the engine, for benchmarking it against other SSR
//! stacks on equal terms.
//!
//! ```text
//! cargo run --release --example http_server --features axum-integration -- \
//!     --bundle path/to/ssr-bundle.js --rows 20 --port 3001
//! ```
//!
//! Two routes, because the honest comparison needs both and they measure
//! different things:
//!
//! - `GET /render/{id}` — renders every time, cache bypassed entirely. This is
//!   the SSR number: how many pages per second the thing can actually build.
//! - `GET /cached/{id}` — goes through the fragment cache, so a repeated `id`
//!   is served from memory. This is mostly a measurement of the HTTP stack, and
//!   it is the number that "requests per second with caching" refers to.
//!
//! `GET /metrics` returns the pool and cache metrics as JSON, so a run can be
//! checked for saturation rather than assumed to be at it.
//!
//! The page is assembled into the same document shape whichever route serves
//! it, so the bytes on the wire are comparable.

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use rusty_ssr::SsrEngine;

struct App {
    engine: SsrEngine,
    payload: String,
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let text = |name: &str| -> Option<String> {
        args.iter()
            .position(|a| a == name)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };
    let value = |name: &str, default: usize| -> usize {
        text(name).and_then(|v| v.parse().ok()).unwrap_or(default)
    };

    let bundle = text("--bundle").expect("--bundle <path> is required");
    let rows = value("--rows", 20);
    let port = value("--port", 3001);
    let pool_size = value("--pool-size", num_cpus::get());
    let cache_size = value("--cache-size", 1000);

    let payload = build_payload(rows);

    let engine = SsrEngine::builder()
        .bundle_path(&bundle)
        .pool_size(pool_size)
        .cache_size(cache_size)
        .cache_ttl_secs(300)
        .build_engine()
        .expect("engine");

    // Warm the isolates so the first requests of a run are not measuring V8's
    // interpreter tiering up.
    for i in 0..600 {
        let _ = engine.render_uncached(&format!("/warm/{i}"), &payload).await;
    }

    let sample = engine
        .render_uncached("/render/0", &payload)
        .await
        .expect("probe render");
    println!("rusty-ssr on :{port}");
    println!("  bundle:     {bundle}");
    println!("  pool_size:  {pool_size}");
    println!("  rows:       {rows} ({} bytes of payload)", payload.len());
    println!("  fragment:   {} bytes", sample.len());
    println!("  document:   {} bytes", document(&sample).len());

    let app = Arc::new(App { engine, payload });

    let router = Router::new()
        .route("/render/:id", get(render_uncached))
        .route("/cached/:id", get(render_cached))
        .route("/metrics", get(metrics))
        .with_state(app);

    let listener = tokio::net::TcpListener::bind(("0.0.0.0", port as u16))
        .await
        .expect("bind");
    axum::serve(listener, router).await.expect("serve");
}

/// Every request renders. No cache is consulted and none is written.
async fn render_uncached(State(app): State<Arc<App>>, Path(id): Path<String>) -> Response {
    match app
        .engine
        .render_uncached(&format!("/render/{id}"), &app.payload)
        .await
    {
        Ok(fragment) => html(document(&fragment)),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

/// Goes through the fragment cache: a repeated `id` is answered from memory.
async fn render_cached(State(app): State<Arc<App>>, Path(id): Path<String>) -> Response {
    match app
        .engine
        .render_with_data(&format!("/cached/{id}"), &app.payload)
        .await
    {
        Ok(fragment) => html(document(&fragment)),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

async fn metrics(State(app): State<Arc<App>>) -> Response {
    let body = serde_json::json!({
        "pool": app.engine.pool_metrics(),
        "cache": app.engine.cache_metrics(),
    });
    (
        [(header::CONTENT_TYPE, "application/json")],
        body.to_string(),
    )
        .into_response()
}

fn html(body: String) -> Response {
    (
        [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
        body,
    )
        .into_response()
}

/// The shell every route wraps the fragment in, so the bytes on the wire are
/// the same shape whichever path produced them — and comparable to what another
/// framework would send for the same page.
fn document(fragment: &str) -> String {
    format!(
        "<!DOCTYPE html><html lang=\"pt-BR\"><head><meta charset=\"utf-8\"/>\
         <title>Apartamentos à venda em Blumenau</title></head><body>{fragment}</body></html>"
    )
}

fn build_payload(rows: usize) -> String {
    const DISTRICTS: [&str; 4] = ["Velha", "Garcia", "Itoupava", "Centro"];
    let listings: Vec<serde_json::Value> = (0..rows)
        .map(|i| {
            serde_json::json!({
                "id": i,
                "slug": format!("apartamento-{i}-blumenau"),
                "title": format!("Apartamento {} quartos — Edifício {i}", 1 + i % 4),
                "district": DISTRICTS[i % 4],
                "city": "Blumenau",
                "photo": format!("/media/{i}.jpg"),
                "bedrooms": 1 + i % 4,
                "bathrooms": 1 + i % 3,
                "parking": i % 3,
                "area": 45 + (i * 7) % 160,
                "price": 25_000_000i64 + (i as i64 * 137_000),
                "discount": if i % 5 == 0 { 10 } else { 0 },
                "featured": i % 7 == 0,
            })
        })
        .collect();

    serde_json::json!({
        "heading": "Apartamentos à venda em Blumenau",
        "tipo": "venda",
        "rows": listings,
    })
    .to_string()
}
