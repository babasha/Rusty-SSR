# 🦀 Rusty SSR

High-performance Server-Side Rendering engine for Rust with V8 isolate pool and multi-tier CPU-optimized caching.

[![Crates.io](https://img.shields.io/crates/v/rusty-ssr.svg)](https://crates.io/crates/rusty-ssr)
[![Documentation](https://docs.rs/rusty-ssr/badge.svg)](https://docs.rs/rusty-ssr)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)

## Features

- **V8 Isolate Pool** — Parallel SSR rendering on all CPU cores
- **Multi-tier Cache** — L1/L2 CPU cache (hot) + RAM (cold) with LRU eviction
- **Axum Integration** — Ready-to-use middleware and handlers
- **Brotli Compression** — Static and dynamic compression
- **Zero-copy** — Efficient memory usage with `Arc<str>`

## Performance

| Metric | Value |
|--------|-------|
| **Peak throughput** | 73,000+ req/s |
| **Cache hit latency** | ~0.2ms |
| **vs Node.js SSR** | 10-15x faster |
| **vs Go SSR** | 3x faster |

## Quick Start

Add to your `Cargo.toml`:

```toml
[dependencies]
rusty-ssr = "0.1"
tokio = { version = "1", features = ["full"] }
axum = "0.7"
```

### Basic Usage

```rust
use rusty_ssr::prelude::*;
use std::sync::Arc;

#[tokio::main]
async fn main() {
    // Create the SSR engine
    let engine = SsrEngine::builder()
        .bundle_path("ssr-bundle.js")
        .pool_size(num_cpus::get())
        .cache_size(300)
        .cache_ttl_secs(300)
        .build_engine()
        .expect("Failed to create SSR engine");

    // Render a page
    let html = engine.render("/home").await.unwrap();
    println!("{}", html);
}
```

### With Axum

```rust
use axum::{extract::State, response::Html, routing::get, Router};
use rusty_ssr::prelude::*;
use std::sync::Arc;

#[tokio::main]
async fn main() {
    let engine = Arc::new(
        SsrEngine::builder()
            .bundle_path("ssr-bundle.js")
            .build_engine()
            .unwrap()
    );

    let app = Router::new()
        .route("/*path", get(ssr_handler))
        .with_state(engine);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn ssr_handler(
    State(engine): State<Arc<SsrEngine>>,
    axum::extract::Path(path): axum::extract::Path<String>,
) -> Html<String> {
    match engine.render(&format!("/{}", path)).await {
        Ok(html) => Html(html.to_string()),
        Err(e) => Html(format!("<h1>Error</h1><pre>{}</pre>", e)),
    }
}
```

## JavaScript Bundle

Your SSR bundle should expose a global render function:

```javascript
// ssr-bundle.js
globalThis.renderPage = async function(url, data) {
    // Your SSR logic here (Preact, React, etc.)
    return `<html>
        <body>
            <h1>Hello from ${url}</h1>
        </body>
    </html>`;
};
```

### With Preact

```javascript
import { h } from 'preact';
import renderToString from 'preact-render-to-string';
import App from './App';

globalThis.renderPage = async function(url, data) {
    const html = renderToString(<App url={url} data={data} />);
    return `<!DOCTYPE html>
        <html>
            <head><title>My App</title></head>
            <body>${html}</body>
        </html>`;
};
```

## Configuration

```rust
let engine = SsrEngine::builder()
    .bundle_path("ssr-bundle.js")     // Path to JS bundle
    .pool_size(8)                      // V8 worker threads
    .queue_capacity(512)               // Task queue size
    .pin_threads(true)                 // Pin to CPU cores
    .cache_size(300)                   // Max cached pages
    .cache_ttl_secs(300)               // Cache TTL (5 min)
    .render_function("renderPage")     // JS function name
    .build_engine()?;
```

## Architecture

```
┌─────────────────────────────────────────────────────┐
│                    SsrEngine                         │
│                                                      │
│  ┌──────────────┐         ┌───────────────────┐    │
│  │   SSR Cache  │ ──miss─► │     V8 Pool       │    │
│  │              │ ◄─────── │                   │    │
│  │  Hot (L1/L2) │  result  │  ┌─────┐ ┌─────┐ │    │
│  │  Cold (RAM)  │          │  │ V8  │ │ V8  │ │    │
│  │  LRU evict   │          │  └─────┘ └─────┘ │    │
│  └──────────────┘          └───────────────────┘    │
└─────────────────────────────────────────────────────┘
```

### Cache Tiers

| Tier | Location | Latency | Size |
|------|----------|---------|------|
| Hot | L1/L2 CPU | ~1-3ns | 8 entries/thread |
| Cold | RAM | ~100ns | Configurable (default 300) |

## Feature Flags

| Feature | Default | Description |
|---------|---------|-------------|
| `v8-pool` | ✅ | V8 thread pool |
| `cache` | ✅ | Multi-tier caching |
| `axum-integration` | ✅ | Axum middleware |
| `brotli-compression` | ❌ | Brotli middleware |
| `full` | ❌ | All features |

```toml
# Minimal (just V8 pool)
rusty-ssr = { version = "0.1", default-features = false, features = ["v8-pool"] }

# Full
rusty-ssr = { version = "0.1", features = ["full"] }
```

## Metrics

```rust
let metrics = engine.cache_metrics();
println!("Hit rate: {:.1}%", metrics.hit_rate);
println!("Lookups: {}", metrics.lookups);
println!("Hot hits: {}", metrics.hot_hits);
println!("Cold hits: {}", metrics.cold_hits);
println!("Misses: {}", metrics.misses);
println!("Cache size: {}/{}", metrics.cold_size, metrics.cold_capacity);
```

## Benchmarks

Tested on MacBook Pro M1/M2 (10 cores, 16GB RAM):

```bash
wrk -t12 -c1000 -d10s http://localhost:3000/
```

```
Requests/sec:  73,304
Latency avg:   18.37ms
Transfer/sec:  156.82MB
```

### Comparison

| Framework | Throughput | vs Rusty SSR |
|-----------|-----------|--------------|
| **Rusty SSR** | 73,000 req/s | 1x |
| Next.js | ~5,000 req/s | 0.07x |
| Remix | ~6,000 req/s | 0.08x |
| Go SSR | ~25,000 req/s | 0.34x |

## License

MIT License - free to use, modify and distribute.
The only requirement is to keep the copyright notice (attribution).

See [LICENSE](LICENSE) for details.

## Contributing

Contributions are welcome! Please open an issue or PR on GitHub.
