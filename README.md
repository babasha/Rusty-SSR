# Rusty SSR

**Server-side rendering inside your Rust binary — no Node sidecar, one shared cache, bounded memory.**

Runs your framework's SSR bundle in a pool of V8 isolates in the same process that serves the request. Renders at the speed Node does, in about a tenth of the memory, with request isolation and a cache every worker shares.

[![Crates.io](https://img.shields.io/crates/v/rusty-ssr.svg)](https://crates.io/crates/rusty-ssr)
[![Documentation](https://docs.rs/rusty-ssr/badge.svg)](https://docs.rs/rusty-ssr)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)

## What it actually does, measured

Two numbers decide what an SSR service can serve, they differ by fifty times, and quoting one without the other says nothing. From a real application — a 2.5 MB Preact bundle producing 200–320 kB pages with database queries behind them, on an 8-core Ryzen 7 260, Linux:

| | pages/s | p50 | p99 |
|---|---|---|---|
| Served from the page cache | 48,029 | 1.04 ms | 5.61 ms |
| **Actually rendered** | **950** | 17.1 ms | 31.2 ms |

**The second row is the one that sizes a deployment.** A pool serves at most `pool_size / render_time` — 16 workers at 16.8 ms each — and throughput stops rising the moment every worker is busy. Going from 16 to 128 concurrent callers bought 9% more throughput and made p50 seven times worse.

So plan with `requests/s ≤ 950 / (1 − hit_rate)`:

| cache hit rate | sustained req/s |
|---|---|
| 90% | ~9,500 |
| 95% | ~19,000 |
| 99% | ~95,000 |

Memory for that configuration was **1.7 GB** at `pool_size = 16` — roughly 70 MB per isolate, reached within seconds and then flat. Cap it with [`max_heap_mb`](#memory-cap) and size `pool_size` deliberately; see [Sizing the pool](#sizing-the-pool).

## How it compares

Rendering is V8 executing your bundle, and that is the same V8 Node embeds. Measured on one machine, same page, same bundle where the bundle is shared:

| | pages/s | note |
|---|---|---|
| rusty-ssr, `pool_size=16` | 18,475 | one process |
| rusty-ssr, `pool_size=1` | 2,421 | one render thread |
| Node + the same Preact bundle | 2,250 | one render thread |
| Node + React `renderToString` | 2,206 | one render thread |
| Next.js 15 App Router (RSC) | 125 | one process |

Read that honestly:

- **Per render thread this engine is at parity with Node** (2,421 vs 2,250 — about 1.1×). There is no version of this that runs JavaScript faster than Node, because it is the same engine running the same bytecode.
- **React and Preact render at the same speed.** 2,206 vs 2,250. The gap to Next.js is not "React is slow" — it is the App Router's RSC pipeline and its 77 kB hydration payload against 24 kB of markup, on a page Next renders in ~10 ms.
- **The whole advantage is the first row**: 7.6× from running 16 isolates in one process. Matching it with Node means about nine processes, each with its own heap, its own compiled copy of the bundle, and its own fragmented cache.

Caveats that belong with those numbers: 24 kB page, load generator on the same machine, Windows for the synthetic rows and Linux for the application ones, Next.js measured in App Router mode (its slowest). Full method and raw output in [BENCHMARK.md](BENCHMARK.md).

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
├─────────────────────┤       │  One shared cache   │
│ ~5GB RAM total      │       │  ~1.7GB RAM total   │
│ No shared cache     │       │  Zero-copy Arc<str> │
└─────────────────────┘       └─────────────────────┘
```

This is the argument for the crate, and it is the part the measurements support. Isolates are not free — about 70 MB each with a 2.5 MB bundle, so sixteen of them is 1.7 GB — but they are far cheaper than processes, and only one of them holds the cache.

The cache being shared is worth as much as the memory. Nine Node processes have nine caches, so a 90%-hit workload fragments into nine partial ones; the equivalent here is one cache all sixteen workers read. Getting that across processes means Redis, and a network hop is four orders of magnitude slower than the lookup it replaces.

### The Solution

Rusty SSR runs V8 isolates in a thread pool managed by Rust. Each worker gets its own V8 instance, they share one cache and one copy of the bundle, and `Arc<str>` means a cache hit hands back a refcount bump rather than a copy of the page.

## Quick Start

```toml
[dependencies]
rusty-ssr = "0.3"
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

That is it — SSR now happens inside your Rust binary, with no Node process to deploy, supervise or pay for.

## Features

### Built-in Browser Polyfills

No more "window is not defined" errors. Rusty SSR automatically injects polyfills for:

- `window`, `document`, `navigator`, `location`
- `localStorage`, `sessionStorage`
- `requestAnimationFrame`, `cancelAnimationFrame`
- `MutationObserver`, `ResizeObserver`, `IntersectionObserver`
- `matchMedia`, `Image`, `performance`

Also `URL`/`URLSearchParams`, `atob`/`btoa`, `fetch` (which throws), timers that
defer to a microtask, and a per-request boundary that resets what a render left
behind.

Not provided: `TextEncoder`/`TextDecoder` and `MessageChannel`. React needs
both at module scope — see [React needs `TextEncoder` and
`MessageChannel`](#react-needs-textencoder-and-messagechannel). Write the exact
prelude out with `rusty-ssr-check --dump-prelude` rather than guessing at it.

### Multi-tier Cache

```
Request → Hot Cache (thread-local) → Cold Cache (sharded) → V8 Render
               ↑                          ↑                     ↓
               └──────────────────────────┴──── cache result ────┘
```

- **Hot cache**: thread-local, 8 entries in an array plus a 128-entry LRU
- **Cold cache**: `DashMap` across 128 shards, LRU eviction, shared by every worker
- **Automatic**: no configuration needed

A hit measures **42–69 ns** end to end through `SsrCache`, a miss ~34–52 ns.
Those are criterion figures and the range is the machine, not the code: run the
same unchanged build twice and criterion will report a 32% "improvement", so
treat anything under about a third as noise. Sharing one cache across all
workers matters far more than the tiering inside it does — against a render at
17 ms, the difference between a 40 ns lookup and a 400 ns one is not visible.

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

- **Preact** — renders as-is, nothing extra needed.
- **React** — needs two globals the prelude does not ship; see below.
- **Vue 3** / **Nuxt**
- **Solid**
- **Svelte** / **SvelteKit**
- **Vanilla JS**

See `examples/bundles/` for complete examples.

#### React needs `TextEncoder` and `MessageChannel`

`react-dom/server` reaches for both at module scope, so a React bundle fails to
*load* — not to render — with `ReferenceError: MessageChannel is not defined`.
The prelude leaves `TextEncoder` out deliberately (a wrong UTF-8 implementation
is worse than an absent one), and that reasoning does not survive contact with
React, which does not feature-detect.

Until the prelude ships them, prepend your own. A correct minimal pair is about
forty lines: a `MessageChannel` whose ports deliver through `Promise.resolve()`,
and a `TextEncoder`/`TextDecoder` that handles surrogate pairs. React measured
at the same speed as Preact here once they were in place.

### Sizing the pool

`pool_size` is the ceiling: the engine serves at most `pool_size / render_time`
pages per second, and every worker costs memory whether it is busy or not.

- **Memory.** About 36 MB per isolate, plus roughly four times your bundle's
  size again per isolate for compiled code — a 2.5 MB bundle measured ~70 MB per
  worker, so `pool_size = 16` was 1.7 GB. Cap each isolate with
  [`max_heap_mb`](#memory-cap) so a runaway render fails instead of growing.
- **`num_cpus::get()` is the wrong default in a container.** It reads CPU
  affinity, not the CFS quota, so on a host with 32 cores it returns 32 however
  small your container's share is. Take `pool_size` from your platform's own
  variable instead.
- **More workers than physical cores buys little.** Measured on 8 physical
  cores: 8 workers gave 5.1× one worker's throughput, 16 gave 6.2×, and past
  ~1.5× the core count p99 degrades sharply while p50 barely improves.
- **Watch, don't guess.** `engine.pool_metrics()` reports `saturation`,
  `queue_pressure` and render percentiles. Saturation pinned at 100% with a
  filling queue means the pool is the bottleneck; nothing else will tell you.

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
- Hot-tier vs cold-tier hit performance
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
