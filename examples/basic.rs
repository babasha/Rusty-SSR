//! Basic example of using Rusty SSR
//!
//! This example shows two modes:
//! 1. Without template — renderPage() returns the full HTML (classic mode)
//! 2. With template — renderPage() returns a fragment, Rust injects it into a template
//!
//! Run with: cargo run --example basic
//!
//! With HTML template (recommended for Vite projects):
//!   cargo run --example basic -- --template dist-web/index.html --manifest dist-web/.vite/manifest.json

use axum::{extract::State, response::Html, routing::get, Router};
use rusty_ssr::prelude::*;
use std::sync::Arc;

#[tokio::main]
async fn main() {
    // Initialize logging
    tracing_subscriber::fmt::init();

    let args: Vec<String> = std::env::args().collect();

    // Build SSR engine
    let mut builder = SsrEngine::builder()
        .bundle_path("ssr-bundle.js")
        .pool_size(num_cpus::get())
        .cache_size(300)
        .cache_ttl_secs(300); // 5 minutes

    // Optional: HTML template mode (Rust assembles the full HTML)
    if let Some(pos) = args.iter().position(|a| a == "--template") {
        if let Some(path) = args.get(pos + 1) {
            builder = builder.html_template(path);
            tracing::info!("📄 Using HTML template: {}", path);
        }
    }
    if let Some(pos) = args.iter().position(|a| a == "--manifest") {
        if let Some(path) = args.get(pos + 1) {
            builder = builder.assets_manifest(path);
            tracing::info!("📦 Using Vite manifest: {}", path);
        }
    }

    let engine = builder
        .build_engine()
        .expect("Failed to create SSR engine");

    let engine = Arc::new(engine);

    // Create Axum router
    let app = Router::new()
        .route("/", get(index_handler))
        .route("/*path", get(ssr_handler))
        .with_state(engine);

    // Start server
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000")
        .await
        .unwrap();
    tracing::info!("🚀 Server running on http://localhost:3000");

    axum::serve(listener, app).await.unwrap();
}

async fn index_handler(State(engine): State<Arc<SsrEngine>>) -> Html<String> {
    // render_to_html: uses template if configured, otherwise returns raw fragment
    match engine.render_to_html("/").await {
        Ok(html) => Html(html),
        Err(e) => Html(format!("<h1>Error</h1><pre>{}</pre>", e)),
    }
}

async fn ssr_handler(
    State(engine): State<Arc<SsrEngine>>,
    axum::extract::Path(path): axum::extract::Path<String>,
) -> Html<String> {
    let url = format!("/{}", path);

    match engine.render_to_html(&url).await {
        Ok(html) => Html(html),
        Err(e) => Html(format!("<h1>Error</h1><pre>{}</pre>", e)),
    }
}
