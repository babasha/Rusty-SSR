# Changelog

## 0.3.0

### Request isolation, as far as this stack allows

0.2 gave the pooled isolate a request boundary through `onSsrRequest`, which
only helps a bundle that knows to define it — and the code that actually leaks
is usually a dependency that does not. `.seal_globals(true)` closes that half
without asking the bundle for anything: the engine records the own property
names of `globalThis` once the bundle has loaded, and every render begins by
deleting whatever was added since.

Off by default, because "delete every global you did not have at startup" is
right for correctness and wrong for a bundle that caches across renders on
purpose — a compiled-template cache, a warmed lookup table. It does not reach
state held in module closures (nothing outside the bundle can), so it composes
with `onSsrRequest` rather than replacing it.

**A fresh V8 context per request was investigated and is not reachable through
this dependency stack.** The cheap way to do it is a context snapshot, and
`v8::SnapshotCreator::add_context` / `set_default_context` are `pub(crate)` in
the `v8` crate, while `deno_core`'s `JsRuntime` always enters its main context.
The alternatives are forking `rusty_v8`, or dropping `deno_core` and driving
raw V8 — a rewrite of the runtime layer, giving up module loading and promise
resolution, for a benefit that sealing plus `onSsrRequest` largely already
deliver. If that changes upstream, this is the note to revisit.

### `location` comes from the render URL

The engine is the only thing that knows the URL, and a router reading
`location.pathname` is how most applications decide what to render — so every
consumer was writing that assignment by hand before calling the bundle. Missing
it is silent: the router sees `/`, every URL renders the home page, and it
reads as a bug in the application rather than a gap in the harness.

The prelude now sets `pathname`, `search`, `hash` and `href` per request, plus
`origin`/`protocol`/`host`/`port` when the URL is absolute. Identity-checked, so
a bundle that installed its own `location` keeps it. `onSsrRequest` receives the
URL too.

### An empty render can be a failure

`.min_render_bytes(n)` turns a render shorter than `n` (trimmed) into
`SsrError::EmptyRender`. This is the SSR failure that does not announce itself:
every real bundle wraps its render in a `try/catch`, the catch returns `""`, and
the caller serves a blank page under a 200 with nothing anywhere saying so. With
a floor set the caller gets an `Err` it can fall back from, and the blank result
is not cached.

### `rusty-ssr-check`

A binary that runs a bundle in the real engine and reports what happens.

```text
rusty-ssr-check dist/ssr-bundle.js --url / --url /products/42 --min-bytes 200
rusty-ssr-check --dump-prelude > prelude.js
```

It checks that the bundle loads, that each URL renders and how many bytes it
produced, and — by rendering one URL, then another, then the first again —
whether the bundle carries state between requests. Exit code 0 or 1, so it
belongs in a deploy script between building the bundle and shipping it.

Every project otherwise writes some version of this as a Node `vm` sandbox with
the prelude copied into it by hand, and that copy drifts: a drifted probe stays
green while production serves blank pages.

### `atob`, `btoa`, `screen`, `devicePixelRatio`

Web APIs bare V8 does not have. A bundle that reaches a missing `atob` throws,
and on this path a throw is an empty page rather than an error anyone sees. For
payloads prefer `render_with_bytes` — there is nothing to decode at all.

### axum is no longer a default feature

`default = ["v8-pool", "cache"]`. Every consumer used to compile axum + tower +
tower-http whether or not they touched the single middleware behind that flag,
and got a *second* axum in their dependency graph as soon as their own version
moved past ours. An SSR engine has no business pinning anyone's web framework.
Opt in with `features = ["axum-integration"]`.

### The example no longer teaches two bugs

`examples/build-preact-bundle.js` wrote `window.__INITIAL_DATA__ =
${JSON.stringify(...)}` unescaped — a `</script>` anywhere in the data ends the
tag and everything after it becomes markup — and caught render errors to return
a page containing the stack trace, which the engine cannot distinguish from a
successful render and therefore caches and serves. Both are gone: the data goes
into a `type="application/json"` tag with `<` escaped, and the render is not
wrapped in a catch, with a comment explaining why that is deliberate.

### Breaking changes

- `default` no longer includes `axum-integration`.
- `V8PoolConfig` gained a required `seal_globals` field.
- `v8_pool::runtime::init_runtime` takes a `seal_globals` argument.
- `SsrError` gained an `EmptyRender` variant.

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
