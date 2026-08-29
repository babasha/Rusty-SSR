# Rusty SSR

**The fastest SSR engine for Rust. Period.**

Render 95,000+ pages per second with sub-millisecond latency. Drop-in replacement for Node.js SSR that's 50x faster.

[![Crates.io](https://img.shields.io/crates/v/rusty-ssr.svg)](https://crates.io/crates/rusty-ssr)
[![Documentation](https://docs.rs/rusty-ssr/badge.svg)](https://docs.rs/rusty-ssr)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)

## Benchmarks (Apple M4, 10 cores)

```
┌─────────────────────────────────────────────────────────────┐
│                    STRESS TEST (30 seconds)                 │
├─────────────────────────────────────────────────────────────┤
│  Requests/sec:      95,363 RPS                              │
│  Total requests:    2,869,878                               │
│  Data transferred:  171 GB                                  │
├─────────────────────────────────────────────────────────────┤
│  Latency p50:       0.46ms                                  │
│  Latency p99:       4.60ms                                  │
│  Max latency:       45.7ms                                  │
└─────────────────────────────────────────────────────────────┘
```

### vs Competition

| Engine | RPS | p99 Latency | Memory |
|--------|-----|-------------|--------|
| **Rusty SSR** | **95,363** | **4.6ms** | ~200MB |
| Next.js (Node) | 500-2,000 | 50-200ms | ~500MB+ |
| Nuxt (Node) | 500-1,500 | 40-150ms | ~500MB+ |

**50x faster throughput. 40x lower latency. 60% less memory.**

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
            .cache_size(500)        // ~500 cached pages (entries, not MB)
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

That tier caches the **fragment** the render returned. Above it sits the page
cache, which caches the **response**.

### Page Cache (0.2)

Between the render and the response there is usually work the render cannot do:
`<head>` tags from a database, a serialised store for the client to hydrate
from, a status code. Caching the fragment means redoing all of it on every hit.
And the URL alone is rarely the key — one path under two hostnames is two
documents the moment a canonical URL is built from the Host header.

So: you say what the key is, you build the document, and the cache does the rest.

```rust
use rusty_ssr::cache::{BuiltPage, CachePolicy, RenderKey};
use std::time::Duration;

let engine = SsrEngine::builder()
    .bundle_path("ssr-bundle.js")
    .page_cache(
        CachePolicy::ttl(500, Duration::from_secs(300))
            .stale_while_revalidate(Duration::from_secs(300)),
    )
    .build_engine()?;

// The Host header is inside the document, so it is part of the key.
let key = RenderKey::new(&path).variant("host", &host);

let page = engine.page(&key, move || async move {
    let meta = load_seo(&path).await?;                  // a database round trip
    let fragment = engine2.render_uncached(&path, "{}").await?;
    Ok(BuiltPage::new(meta.status, assemble(fragment, meta)))
}).await?;

// page.body is `Bytes` — answering is a refcount bump, not a copy.
```

What that buys over caching the fragment:

| | |
|---|---|
| **A hit costs nothing** | the closure never runs, so those database round trips never happen either |
| **Single-flight** | forty concurrent requests for one cold key run **one** build, not forty |
| **Stale-while-revalidate** | an expired page is answered immediately while its replacement builds behind the visitor |
| **Status and headers travel** | a 404 body is served as a 404; a redirect is cacheable like anything else |
| **`Bytes` bodies** | a hit hands back a refcount bump |

`CachePolicy` is `Off` / `ttl(capacity, d)` / `forever(capacity)` — "off" is a
variant, not a magic zero. `RenderKey::variant_digest` keys on a payload
without storing it, for when the data can change independently of the URL.

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

// Assemble the full document in a single pass: the cached fragment goes
// into <!--ssr:outlet-->, and your per-request head tags into their own
// placeholders — one allocation, no chained String::replace.
let html = engine.render_to_html_with_replacements(
    "/?listing=42",
    "{}",
    &[
        ("<!--ssr:title-->", "Listing #42"),
        ("<!--seo-->", "<meta property=\"og:title\" content=\"Listing #42\" />"),
    ],
).await?;
```

### What's in 0.3.0

Full notes in [CHANGELOG.md](CHANGELOG.md).

- **`rusty-ssr-check`** — run your bundle in the real engine and find out what
  it does, instead of in a hand-copied imitation of it:
  ```text
  rusty-ssr-check dist/ssr-bundle.js --url / --url /products/42 --min-bytes 200
  ```
  Loads it, renders each URL, reports the bytes, and detects state carried
  between requests. Exit 0 or 1 — put it in your deploy script.
- **`.seal_globals(true)`** — delete globals the bundle did not have at startup,
  before every render. The half of request isolation that needs no cooperation
  from the bundle, which matters because the code that leaks is usually a
  dependency that never defines a hook.
- **`.min_render_bytes(n)`** — a render that produced (almost) nothing becomes
  an error instead of a blank page served with a 200.
- **`location` from the render URL** — no more writing that assignment by hand
  before every render, and no more "every URL renders the home page".
- **`atob` / `btoa` / `screen` / `devicePixelRatio`** in the prelude.
- **axum is no longer a default feature.** `features = ["axum-integration"]` to
  opt in.

A fresh V8 context per request was investigated and is not reachable through
`deno_core` + `rusty_v8` today — the CHANGELOG explains exactly where it stops.

### What's in 0.2.0

Everything here came out of running 0.1 in production for a season. Full notes
in [CHANGELOG.md](CHANGELOG.md).

- **A page cache** — finished documents keyed by `RenderKey`, with
  single-flight, stale-while-revalidate, `CachePolicy`, and status + headers +
  `Bytes` bodies. See above.
- **A request boundary in the pooled isolate.** `globalThis` outlives a render
  and the prelude's `localStorage` is real storage, so request B used to start
  inside request A's leftovers — and A and B are different people. The prelude
  now resets its own state before every render and calls `onSsrRequest()` so the
  bundle can clear its own.
- **Binary payloads.** `render_with_bytes` delivers a `Uint8Array` over the very
  buffer you passed; `render_with_json_and_bytes` gives you
  `renderPage(url, data, bytes)` — a JSON envelope beside a blob, without
  base64 in between.
- **The bundle is no longer process-global.** Two engines in one process are two
  applications now; before, the second silently rendered the first one's bundle.
- **`BROWSER_POLYFILLS` is public**, so a bundle can be tested in the real
  environment instead of a hand-copied imitation of it that drifts.

Breaking: the `init_bundle*` / `get_bundle` / `is_initialized` family is gone
(replaced by `v8_pool::compose`), `V8PoolConfig` gained a `bundle` field, and
`renderer::render_html` takes a `RenderPayload`.

### What's in 0.1.1

Correctness, robustness and efficiency overhaul:

- **Cache key covers URL *and* data**, and the **full key is compared** on
  lookup — `render_json(url, A)` and `render_json(url, B)` no longer collide,
  and a 64-bit hash collision degrades to a miss (never serves wrong content).
- **Errors/empty aren't frozen in cache**: a render that throws returns `Err`
  (uncached); `.cache_empty(false)` skips caching empty output.
- **Whole-request timeout**: `request_timeout` bounds enqueue *and* the render
  wait, and a **watchdog terminates a runaway render** (even a non-allocating
  `while(true)`) so the worker is reclaimed. A panicking render no longer kills
  its worker.
- **No per-request JS recompile**: the render function is resolved once and
  invoked via a native call; URL/data are passed as V8 values (no
  source-escaping pitfalls).
- **Real LRU hot cache** (no FIFO drift / duplicate entries), **V8 heap cap**
  (`.max_heap_mb`), **non-clobbering polyfills** with `URL`/`URLSearchParams`
  (+ `.polyfills(false)`), single-pass template assembly, and a
  `.cache_key_normalizer` for collapsing tracking-param URLs.

### Cache-bypass for one-off URLs

One-time and tracking URLs (`?reset=…`, `?verify=…`, `?utm_*`, `?fbclid=…`)
shouldn't each take a slot in a fixed-size cache. Two tools:

```rust
// Render fresh + assemble the template, but never touch the cache:
let html = engine
    .render_to_html_uncached("/?reset=onetimetoken", "{}")
    .await?;

// Or collapse equivalent URLs onto one cache key (strip the query):
fn strip_query(url: &str) -> String {
    url.split('?').next().unwrap_or(url).to_string()
}
let engine = SsrEngine::builder()
    .bundle_path("ssr-bundle.js")
    .cache_key_normalizer(strip_query) // utm/fbclid variants now share one entry
    .build_engine()?;
```

### Memory cap

On a small box, cap each isolate's heap. A render that exceeds it is
terminated and returns `Err` (uncached) instead of aborting the process:

```rust
let engine = SsrEngine::builder()
    .bundle_path("ssr-bundle.js")
    .max_heap_mb(256)
    .build_engine()?;
```

### Configuration

```rust
    let engine = SsrEngine::builder()
        .bundle_path("ssr-bundle.js")     // Path to JS bundle
        .pool_size(num_cpus::get())       // V8 workers (default: CPU count)
        .queue_capacity(512)               // Task queue size
        .pin_threads(true)                 // Pin workers to CPU cores
        .cache_size(500)                   // Number of cached entries
        .cache_ttl_secs(300)               // Cache TTL (0 = forever)
        .cache_empty(false)                // Don't cache empty renders (default: true)
        .polyfills(true)                   // Built-in browser polyfills (default: true)
        .max_heap_mb(512)                  // Per-isolate V8 heap cap (default: none)
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

## Testing & Benchmarks

### Running Tests

```bash
# Run all tests
cargo test

# Run integration tests only
cargo test --test integration_tests

# Run with verbose output
cargo test -- --nocapture
```

Integration tests cover:
- V8 pool configuration
- DashMap concurrent cache operations
- LRU cache eviction behavior
- Async patterns (tokio channels, timeouts)
- Thread safety (Arc, Mutex, mpsc)
- URL parsing and JSON serialization

### Running Benchmarks

```bash
# Run all benchmarks
cargo bench

# Run SSR benchmarks only
cargo bench --bench ssr_benchmark

# Run cache benchmarks only
cargo bench --bench cache_benchmark
```

**SSR Benchmarks** (`ssr_benchmark`):
- Pool config creation overhead
- String operations (small/medium/large HTML)
- JSON serialization performance
- Channel throughput (request queue simulation)

**Cache Benchmarks** (`cache_benchmark`):
- DashMap concurrent read/write (1, 2, 4, 8 threads)
- DashMap sharding (sequential vs random keys)
- L1/L2 cache hit performance
- LRU eviction overhead (128, 512, 2048 entries)
- Arc<str> vs String cloning

Results are saved to `target/criterion/` with HTML reports.

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
