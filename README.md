# Rusty SSR

High-performance Server-Side Rendering engine for Rust with V8 isolate pool and multi-tier CPU-optimized caching.

**Framework-agnostic** — works with React, Preact, Vue, Solid, Svelte, or any JS framework that supports SSR.

[![Crates.io](https://img.shields.io/crates/v/rusty-ssr.svg)](https://crates.io/crates/rusty-ssr)
[![Documentation](https://docs.rs/rusty-ssr/badge.svg)](https://docs.rs/rusty-ssr)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)

## Features

- **V8 Isolate Pool** — Parallel SSR rendering on all CPU cores
- **Multi-tier Cache** — L1/L2 CPU cache (hot) + RAM (cold) with LRU eviction
- **Framework Agnostic** — React, Preact, Vue, Solid, Svelte, vanilla JS
- **Axum Integration** — Ready-to-use middleware and handlers
- **Zero-copy** — Efficient memory usage with `Arc<str>`

## Performance

| Metric | Value |
|--------|-------|
| **Peak throughput** | 73,000+ req/s |
| **Cache hit latency** | ~0.2ms |
| **vs Node.js SSR** | 10-15x faster |

## Quick Start

```toml
[dependencies]
rusty-ssr = "0.1"
tokio = { version = "1", features = ["full"] }
```

### 1. Create your SSR bundle

Your JavaScript bundle must expose a global `renderPage` function:

```javascript
// ssr-bundle.js
globalThis.renderPage = async function(url, data) {
    // Your SSR logic here
    return `<!DOCTYPE html>
<html>
<body>
    <h1>Hello from ${url}</h1>
</body>
</html>`;
};
```

### 2. Use in Rust

```rust
use rusty_ssr::prelude::*;

#[tokio::main]
async fn main() {
    let engine = SsrEngine::builder()
        .bundle_path("ssr-bundle.js")
        .pool_size(num_cpus::get())
        .cache_size(300)
        .build_engine()
        .expect("Failed to create SSR engine");

    let html = engine.render("/home").await.unwrap();
    println!("{}", html);
}
```

## SSR Bundle Contract

Rusty SSR calls your JavaScript function with this signature:

```typescript
globalThis.renderPage(url: string, data: string | object): Promise<string>
```

| Parameter | Type | Description |
|-----------|------|-------------|
| `url` | `string` | URL path (e.g., `"/"`, `"/products/123"`) |
| `data` | `string \| object` | JSON data from `render_with_data()` |
| **Returns** | `string` | Complete HTML document |

The function must return a **complete HTML document** (including `<!DOCTYPE html>`).

## Framework Examples

### Preact

```javascript
import { h } from 'preact';
import renderToString from 'preact-render-to-string';
import App from './App';

globalThis.renderPage = async function(url, data) {
    const props = typeof data === 'string' ? JSON.parse(data) : data;
    const html = renderToString(<App url={url} {...props} />);

    return `<!DOCTYPE html>
<html>
<head><title>My App</title></head>
<body>
    <div id="app">${html}</div>
    <script>window.__DATA__ = ${JSON.stringify(props)}</script>
    <script type="module" src="/client.js"></script>
</body>
</html>`;
};
```

### React

```javascript
import React from 'react';
import { renderToString } from 'react-dom/server';
import App from './App';

globalThis.renderPage = async function(url, data) {
    const props = typeof data === 'string' ? JSON.parse(data) : data;
    const html = renderToString(<App url={url} {...props} />);

    return `<!DOCTYPE html>
<html>
<head><title>React App</title></head>
<body>
    <div id="root">${html}</div>
    <script>window.__DATA__ = ${JSON.stringify(props)}</script>
    <script type="module" src="/client.js"></script>
</body>
</html>`;
};
```

### Vue 3

```javascript
import { createSSRApp } from 'vue';
import { renderToString } from '@vue/server-renderer';
import App from './App.vue';

globalThis.renderPage = async function(url, data) {
    const props = typeof data === 'string' ? JSON.parse(data) : data;
    const app = createSSRApp(App, { url, ...props });
    const html = await renderToString(app);

    return `<!DOCTYPE html>
<html>
<head><title>Vue App</title></head>
<body>
    <div id="app">${html}</div>
    <script>window.__DATA__ = ${JSON.stringify(props)}</script>
    <script type="module" src="/client.js"></script>
</body>
</html>`;
};
```

### Solid

```javascript
import { renderToString } from 'solid-js/web';
import App from './App';

globalThis.renderPage = async function(url, data) {
    const props = typeof data === 'string' ? JSON.parse(data) : data;
    const html = await renderToString(() => App({ url, ...props }));

    return `<!DOCTYPE html>
<html>
<head><title>Solid App</title></head>
<body>
    <div id="app">${html}</div>
    <script>window.__DATA__ = ${JSON.stringify(props)}</script>
    <script type="module" src="/client.js"></script>
</body>
</html>`;
};
```

See `examples/bundles/` for complete examples.

## With Axum

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

## Passing Data to JavaScript

```rust
// Simple render (data = "{}")
let html = engine.render("/products").await?;

// With custom data
let data = serde_json::json!({
    "products": [...],
    "user": { "name": "John" }
});
let html = engine.render_with_data("/products", &data.to_string()).await?;
```

## Configuration

```rust
let engine = SsrEngine::builder()
    .bundle_path("ssr-bundle.js")     // Path to JS bundle
    .pool_size(8)                      // V8 worker threads (default: CPU count)
    .queue_capacity(512)               // Task queue size
    .pin_threads(true)                 // Pin workers to CPU cores
    .cache_size(300)                   // Max cached pages
    .cache_ttl_secs(300)               // Cache TTL in seconds (0 = no expiry)
    .render_function("renderPage")     // JS function name
    .build_engine()?;
```

## Architecture

```
┌─────────────────────────────────────────────────────┐
│                    SsrEngine                        │
│                                                     │
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
| Hot | L1/L2 CPU cache | ~1-3ns | 8 entries/thread |
| Cold | RAM (DashMap) | ~100ns | Configurable |

## Building Your SSR Bundle

### Option 1: Direct (recommended)

Write your bundle with `globalThis.renderPage` directly. See `examples/bundles/`.

### Option 2: Wrap existing bundle

Use the build script to wrap a Vite/webpack SSR output:

```bash
node scripts/build-bundle.js dist/server.js ssr-bundle.js --iife SSRBundle
```

### Vite Configuration

```typescript
// vite.config.ts
export default defineConfig({
  build: {
    ssr: true,
    rollupOptions: {
      input: 'src/entry-server.tsx',
      output: {
        format: 'iife',
        name: 'SSRBundle',
      },
    },
  },
});
```

## Metrics

```rust
let metrics = engine.cache_metrics();
println!("Hit rate: {:.1}%", metrics.hit_rate);
println!("Hot hits: {}", metrics.hot_hits);
println!("Cold hits: {}", metrics.cold_hits);
println!("Misses: {}", metrics.misses);
```

## Feature Flags

| Feature | Default | Description |
|---------|---------|-------------|
| `v8-pool` | ✅ | V8 thread pool |
| `cache` | ✅ | Multi-tier caching |
| `axum-integration` | ✅ | Axum middleware |
| `brotli-compression` | ❌ | Brotli middleware |
| `full` | ❌ | All features |

```toml
# Minimal
rusty-ssr = { version = "0.1", default-features = false, features = ["v8-pool"] }

# Full
rusty-ssr = { version = "0.1", features = ["full"] }
```

## Project Structure

```
rusty-ssr/
├── src/
│   ├── lib.rs           # Public API
│   ├── engine.rs        # SsrEngine
│   ├── config.rs        # Configuration
│   ├── error.rs         # Error types
│   ├── v8_pool/         # V8 thread pool
│   │   ├── pool.rs      # Worker pool
│   │   ├── runtime.rs   # Thread-local V8
│   │   ├── renderer.rs  # JS execution
│   │   └── bundle.rs    # Bundle loader
│   ├── cache/           # Multi-tier cache
│   │   ├── hot.rs       # L1/L2 cache
│   │   ├── cold.rs      # RAM cache
│   │   └── ssr.rs       # Combined cache
│   └── middleware/      # Axum middleware
├── examples/
│   ├── basic.rs         # Basic Axum example
│   ├── bundles/         # JS bundle examples
│   │   ├── minimal.js
│   │   ├── preact.js
│   │   ├── react.js
│   │   ├── vue.js
│   │   └── solid.js
│   └── build-preact-bundle.js
└── scripts/
    └── build-bundle.js  # Bundle wrapper tool
```

## License

MIT License — free to use, modify, and distribute.

## Contributing

Contributions welcome! Please open an issue or PR.
