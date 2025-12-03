# Rusty SSR

**The fastest SSR engine for Rust. Period.**

Render 88,000+ pages per second with sub-millisecond latency. Drop-in replacement for Node.js SSR that's 50x faster.

[![Crates.io](https://img.shields.io/crates/v/rusty-ssr.svg)](https://crates.io/crates/rusty-ssr)
[![Documentation](https://docs.rs/rusty-ssr/badge.svg)](https://docs.rs/rusty-ssr)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)

## Benchmarks (Apple M4, 10 cores)

```
┌─────────────────────────────────────────────────────────────┐
│                    STRESS TEST (30 seconds)                 │
├─────────────────────────────────────────────────────────────┤
│  Requests/sec:      88,731 RPS                              │
│  Total requests:    2,666,835                               │
│  Data transferred:  159 GB                                  │
├─────────────────────────────────────────────────────────────┤
│  Latency p50:       1.05ms                                  │
│  Latency p99:       6.12ms                                  │
│  Max latency:       183ms                                   │
└─────────────────────────────────────────────────────────────┘
```

### vs Competition

| Engine | RPS | p99 Latency | Memory |
|--------|-----|-------------|--------|
| **Rusty SSR** | **88,731** | **6ms** | ~200MB |
| Next.js (Node) | 500-2,000 | 50-200ms | ~500MB+ |
| Nuxt (Node) | 500-1,500 | 40-150ms | ~500MB+ |

**50x faster throughput. 30x lower latency. 60% less memory.**

## Why Rusty SSR?

### The Problem with Node.js SSR

```
Node.js Cluster Mode          Rusty SSR
┌─────────────────────┐       ┌─────────────────────┐
│ Process 1           │       │ 1 Process           │
│  └─ V8 + 512MB heap │       │  ├─ V8 isolate 1    │
├─────────────────────┤       │  ├─ V8 isolate 2    │
│ Process 2           │       │  ├─ V8 isolate 3    │
│  └─ V8 + 512MB heap │       │  ├─ ...             │
├─────────────────────┤       │  └─ V8 isolate 10   │
│ ... × 10            │       │                     │
├─────────────────────┤       │  Shared L1/L2 Cache │
│ ~5GB RAM total      │       │  ~200MB RAM total   │
│ No shared cache     │       │  Zero-copy Arc<str> │
└─────────────────────┘       └─────────────────────┘
```

- **Node.js**: 10 processes × 512MB = 5GB RAM, no shared cache
- **Rusty SSR**: 1 process, 10 V8 isolates, shared cache, 200MB RAM

### The Solution

Rusty SSR runs V8 isolates in a thread pool managed by Rust. Each CPU core gets its own V8 instance, but they share a common cache. Zero-copy `Arc<str>` means no memory duplication.

## Quick Start

```toml
[dependencies]
rusty-ssr = "0.1"
tokio = { version = "1", features = ["full"] }
axum = "0.7"
```

### 1. Create SSR Bundle

```javascript
// ssr-bundle.js
globalThis.renderPage = async function(url, data) {
    // Your framework's SSR here (React, Preact, Vue, Solid...)
    const html = renderToString(<App url={url} {...data} />);

    return `<!DOCTYPE html>
<html>
<head><title>My App</title></head>
<body><div id="app">${html}</div></body>
</html>`;
};
```

### 2. Use with Axum

```rust
use axum::{extract::State, response::Html, routing::get, Router};
use rusty_ssr::prelude::*;
use std::sync::Arc;

#[tokio::main]
async fn main() {
    // Initialize SSR engine (auto-detects CPU cores)
    let engine = Arc::new(
        SsrEngine::builder()
            .bundle_path("ssr-bundle.js")
            .cache_size(500)        // 500MB cache
            .cache_ttl_secs(300)    // 5 min TTL
            .build_engine()
            .expect("Failed to create SSR engine")
    );

    let app = Router::new()
        .route("/", get(ssr_handler))
        .route("/*path", get(ssr_handler))
        .with_state(engine);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    println!("SSR server running on http://localhost:3000");
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

That's it. Your SSR is now 50x faster.

## Features

### Built-in Browser Polyfills

No more "window is not defined" errors. Rusty SSR automatically injects polyfills for:

- `window`, `document`, `navigator`, `location`
- `localStorage`, `sessionStorage`
- `requestAnimationFrame`, `cancelAnimationFrame`
- `MutationObserver`, `ResizeObserver`, `IntersectionObserver`
- `matchMedia`, `Image`, `performance`

Just load your bundle — it works.

### Multi-tier Cache

```
Request → L1/L2 Hot Cache (1-3ns) → Cold Cache (100ns) → V8 Render
               ↑                          ↑                  ↓
               └──────────────────────────┴──── cache result ┘
```

- **Hot cache**: Thread-local, L1/L2 CPU cache speed
- **Cold cache**: DashMap with LRU eviction
- **Automatic**: No configuration needed

### Framework Agnostic

Works with any JavaScript framework that supports SSR:

- **React** / **Preact**
- **Vue 3** / **Nuxt**
- **Solid**
- **Svelte** / **SvelteKit**
- **Vanilla JS**

See `examples/bundles/` for complete examples.

## API Reference

### Basic Render

```rust
// Simple render
let html = engine.render("/products").await?;

// With JSON data
use serde_json::json;
let html = engine.render_json("/products", json!({
    "products": [...],
    "user": { "id": 1 }
})).await?;

// With string data
let html = engine.render_with_data("/products", r#"{"page": 1}"#).await?;

// Skip cache (always render fresh)
let html = engine.render_uncached("/admin", "{}").await?;
```

### Configuration

```rust
let engine = SsrEngine::builder()
    .bundle_path("ssr-bundle.js")     // Path to JS bundle
    .pool_size(num_cpus::get())       // V8 workers (default: CPU count)
    .queue_capacity(512)               // Task queue size
    .pin_threads(true)                 // Pin workers to CPU cores
    .cache_size(500)                   // Cache size in MB
    .cache_ttl_secs(300)               // Cache TTL (0 = forever)
    .render_function("renderPage")     // JS function name
    .build_engine()?;
```

### Cache Metrics

```rust
let metrics = engine.cache_metrics();
println!("Hit rate: {:.1}%", metrics.hit_rate);
println!("Hot hits: {}", metrics.hot_hits);
println!("Cold hits: {}", metrics.cold_hits);
println!("Misses: {}", metrics.misses);
```

## Building SSR Bundles

### Option 1: Vite (Recommended)

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
        inlineDynamicImports: true
      },
    },
  },
});
```

```bash
# Build SSR bundle
vite build --ssr

# Wrap for Rusty SSR
node scripts/build-bundle.js dist/server/entry.js ssr-bundle.js --iife SSRBundle
```

### Option 2: Direct

Write your bundle with `globalThis.renderPage` directly:

```javascript
import { render } from 'preact-render-to-string';
import App from './App';

globalThis.renderPage = async function(url, data) {
    const html = render(<App url={url} {...data} />);
    return `<!DOCTYPE html><html><body>${html}</body></html>`;
};
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
# Minimal (just V8 pool)
rusty-ssr = { version = "0.1", default-features = false, features = ["v8-pool"] }

# Full (everything)
rusty-ssr = { version = "0.1", features = ["full"] }
```

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                        SsrEngine                            │
│                                                             │
│  ┌─────────────────┐           ┌──────────────────────────┐ │
│  │   SSR Cache     │           │       V8 Pool            │ │
│  │                 │  miss     │                          │ │
│  │  ┌───────────┐  │ ───────►  │  ┌────┐ ┌────┐ ┌────┐   │ │
│  │  │ Hot (L1)  │  │           │  │ V8 │ │ V8 │ │ V8 │   │ │
│  │  └───────────┘  │           │  └────┘ └────┘ └────┘   │ │
│  │  ┌───────────┐  │  result   │         ...              │ │
│  │  │ Cold (RAM)│  │ ◄───────  │  ┌────┐ ┌────┐ ┌────┐   │ │
│  │  └───────────┘  │           │  │ V8 │ │ V8 │ │ V8 │   │ │
│  │  LRU eviction   │           │  └────┘ └────┘ └────┘   │ │
│  └─────────────────┘           └──────────────────────────┘ │
└─────────────────────────────────────────────────────────────┘
```

## Deployment

### Docker

```dockerfile
FROM rust:1.75-slim-bookworm AS builder
WORKDIR /app
COPY . .
RUN cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/your-app /app/server
COPY ssr-bundle.js /app/
WORKDIR /app
CMD ["./server"]
```

### Railway / Fly.io

Just push your code — Rusty SSR works with any platform that supports Rust.

## Troubleshooting

### "window is not defined"

This shouldn't happen with v0.1+ — browser polyfills are automatic. If it does:

1. Check your bundle doesn't run browser code at module load time
2. Use `typeof window !== 'undefined'` guards if needed

### "renderPage is not a function"

Your bundle must expose `globalThis.renderPage`:

```javascript
// Correct
globalThis.renderPage = async (url, data) => { ... };

// Wrong
export function renderPage() { ... }  // ESM export won't work
```

### Memory usage grows

Set a cache TTL to prevent unbounded growth:

```rust
.cache_ttl_secs(300)  // Expire after 5 minutes
```

## License

MIT — use it however you want.

## Contributing

Issues and PRs welcome! See [CONTRIBUTING.md](CONTRIBUTING.md).
