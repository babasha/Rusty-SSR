# Changelog

## 0.2.0

Everything here came out of running 0.1 in production for a season. The theme
is that an SSR engine's job does not end at "V8 returned a string" — the caller
still has a response to assemble, a key to decide, and an isolate full of the
last request's state to worry about.

### The page cache (`rusty_ssr::cache::page`)

A second cache tier, above the existing fragment cache, holding **finished
documents**. `SsrEngine::page(&key, build)` is the new entry point.

The fragment cache is keyed on `url + data` and stores what the render
returned. Three things go wrong when an application tries to use that as its
page cache:

1. **The fragment is not the page.** `<head>` tags from a database, a serialised
   store for the client to hydrate from, a status code — all of it is added
   after the render, and a hit redoes all of it.
2. **The URL is not the key.** One path under two hostnames is two documents the
   moment a canonical URL or `og:url` is built from the Host header. Locale,
   device class and currency do the same.
3. **`data` is the wrong thing to put in a key.** It is the payload. Hashing tens
   of kilobytes on every lookup, to store an entry no second request will hit,
   is not caching.

So the caller says what the key is, and the cache handles what is tedious:

- **`RenderKey`** — the URL plus caller-declared *variants* (`host`, `locale`,
  …), order-independent. `variant_digest` keys on a payload without storing it.
- **`CachePolicy`** — `Off` / `ttl(capacity, d)` / `forever(capacity)`, plus
  `stale_while_revalidate(window)`. Replaces two settings that both lied:
  `cache_ttl_secs(0)` meant "never expires" rather than "do not cache", and
  `cache_size(0)` was refused outright, so "off" could not be said at all.
- **Single-flight** — forty concurrent requests for one cold key run one build.
- **Stale-while-revalidate** — an expired page is answered immediately while its
  replacement is built behind the visitor.
- **`BuiltPage` / `CachedPage`** carry status *and* headers, so a redirect is
  cacheable like anything else, and bodies are `Bytes` — a hit is a refcount
  bump, not a copy of the document.

### A request boundary in the pooled isolate

`globalThis` outlives a render, and the prelude's `localStorage` is *real*
in-memory storage. Without a boundary, request B starts inside request A's
leftovers — a correctness problem for any module-level cache in the bundle, and
a privacy one, because A and B are different people.

The prelude now installs `__rustySsrReset()`, called before every render. It
re-creates the storage objects it owns (tracked by identity, so a bundle that
installs its own storage later keeps it) and then calls
**`globalThis.onSsrRequest`** if the bundle defines one — the hook for a bundle
to clear its own state. A throw there fails the render rather than being
swallowed: a request that could not be isolated must not be served.

### Binary payloads

- `SsrEngine::render_with_bytes(url, Vec<u8>)` — the payload arrives as a
  `Uint8Array` over the very buffer passed in. No copy in Rust, no decode in JS.
- `SsrEngine::render_with_json_and_bytes(url, json, bytes)` — a small JSON
  envelope *and* a binary payload: `renderPage(url, data, bytes)`. This is the
  shape most page data actually has, and it removes the base64-into-a-JSON-field
  round trip that otherwise obliges every bundle to carry its own `atob`.

A bundle written as `function(url, data)` ignores the third argument, so this
can be adopted one side at a time.

### The bundle is no longer process-global

`SSR_BUNDLE`, `init_bundle`, `init_bundle_with`, `init_bundle_from_string`,
`init_bundle_raw`, `get_bundle` and `is_initialized` are **removed**, replaced by
`v8_pool::compose(path, polyfills)`. The composed source is owned by the pool.

The global meant the *second* engine built in a process silently rendered the
*first* one's bundle, and that a test binary could hold only one bundle however
many cases it had. Engines are now independent.

### `BROWSER_POLYFILLS` is public

Testing an SSR bundle means running it in the engine's environment. Without the
constant exported, projects re-type the prelude into a Node `vm` sandbox by
hand — and that copy drifts silently, because a drifted probe still passes. Write
the real thing out instead:

```rust
std::fs::write("prelude.js", rusty_ssr::v8_pool::BROWSER_POLYFILLS)?;
```

### Breaking changes

- The `init_bundle*` / `get_bundle` / `is_initialized` family is gone (see
  above). `SsrEngine` never needed them; it composed the bundle itself.
- `V8PoolConfig` gained a required `bundle: Arc<str>` field.
- `v8_pool::renderer::render_html` takes a `RenderPayload` instead of
  `Option<&str>`.

Nothing else changed shape: `render`, `render_with_data`, `render_to_html*`,
`render_uncached*` and the fragment cache behave exactly as in 0.1.1.

## 0.1.1

- Cache key covers `data` as well as the URL (0.1.0 checked the cache *before*
  it looked at `data`, so a render with different data could return another
  request's body).
- Render arguments are passed as native V8 values with the function handle
  cached, instead of interpolating the URL and data into freshly compiled JS on
  every request.
- A throw inside the render propagates as an error instead of being turned into
  a successful "SSR Error" page carrying a stack trace.
- Watchdog terminates runaway renders; per-isolate heap cap; panics in a render
  no longer kill the worker.
- `URL` / `URLSearchParams` added to the prelude; `setTimeout` defers to a
  microtask rather than running inline.
- Polyfills are non-clobbering.

## 0.1.0

Initial release.
