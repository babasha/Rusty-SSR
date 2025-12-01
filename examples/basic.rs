//! Basic example of using Rusty SSR
//!
//! This example shows how to set up a simple SSR server with Axum.
//!
//! Run with: cargo run --example basic

use axum::{extract::State, response::Html, routing::get, Router};
use rusty_ssr::prelude::*;
use std::sync::Arc;
use tracing_subscriber;

#[tokio::main]
async fn main() {
    // Initialize logging
    tracing_subscriber::fmt::init();

    // Create the SSR engine
    let engine = SsrEngine::builder()
        .bundle_path("ssr-bundle.js")
        .pool_size(num_cpus::get())
        .cache_size(300)
        .cache_ttl_secs(300) // 5 minutes
        .build_engine()
        .expect("Failed to create SSR engine");

    let engine = Arc::new(engine);

    // Create Axum router
    let app = Router::new()
        .route("/", get(index_handler))
        .route("/*path", get(ssr_handler))
        .with_state(engine);

    // Start server
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    tracing::info!("🚀 Server running on http://localhost:3000");

    axum::serve(listener, app).await.unwrap();
}

async fn index_handler(State(engine): State<Arc<SsrEngine>>) -> Html<String> {
    match engine.render("/").await {
        Ok(html) => Html(html.to_string()),
        Err(e) => Html(format!("<h1>Error</h1><pre>{}</pre>", e)),
    }
}

async fn ssr_handler(
    State(engine): State<Arc<SsrEngine>>,
    axum::extract::Path(path): axum::extract::Path<String>,
) -> Html<String> {
    let url = format!("/{}", path);

    match engine.render(&url).await {
        Ok(html) => Html(html.to_string()),
        Err(e) => Html(format!("<h1>Error</h1><pre>{}</pre>", e)),
    }
}
