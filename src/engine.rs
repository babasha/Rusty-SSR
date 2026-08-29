//! Main SSR Engine

use std::sync::Arc;

use crate::config::{SsrConfig, SsrConfigBuilder};
use crate::error::{SsrError, SsrResult};

#[cfg(feature = "v8-pool")]
use crate::v8_pool::{PoolError, V8Pool};

#[cfg(feature = "cache")]
use crate::cache::{BuiltPage, CachedPage, PageCache, RenderKey, SsrCache};

#[cfg(feature = "cache")]
use std::borrow::Cow;

#[cfg(feature = "cache")]
use std::future::Future;

/// Separator between the URL part and the render-data part of a cache key.
///
/// U+001F (unit separator) never appears unescaped in a URL or in serde_json's
/// compact output, so `normalize(url) + SEP + data` has an unambiguous boundary
/// for URL-scoped invalidation.
#[cfg(feature = "cache")]
const CACHE_KEY_SEP: char = '\u{1f}';

// ── HTML Template System ─────────────────────────────────────────────────────

/// Parsed Vite manifest entry
#[derive(Debug)]
struct ManifestEntry {
    file: String,
    css: Vec<String>,
}

/// Pre-loaded HTML template with asset tags injected
#[derive(Debug, Clone)]
struct HtmlTemplate {
    /// Template string with `<!--ssr:outlet-->` still present (replaced per-request)
    content: String,
}

impl HtmlTemplate {
    /// Load template from file, parse manifest, and inject asset tags
    fn load(config: &SsrConfig) -> SsrResult<Option<Self>> {
        let template_path = match &config.html_template_path {
            Some(p) => p,
            None => return Ok(None),
        };

        tracing::info!("📄 Loading HTML template from {:?}", template_path);

        let mut content = std::fs::read_to_string(template_path).map_err(|e| {
            SsrError::Template(format!(
                "Failed to read HTML template {:?}: {}",
                template_path, e
            ))
        })?;

        if !content.contains("<!--ssr:outlet-->") {
            return Err(SsrError::Template(
                "HTML template must contain <!--ssr:outlet--> placeholder".into(),
            ));
        }

        // Parse Vite manifest and inject asset tags
        if let Some(manifest_path) = &config.assets_manifest_path {
            let entry = Self::parse_manifest(manifest_path)?;

            // Build CSS link tags
            let css_tags: String = entry
                .css
                .iter()
                .map(|p| format!(r#"<link rel="stylesheet" href="/{}" />"#, p))
                .collect::<Vec<_>>()
                .join("\n    ");

            // Build script tag
            let script_tag = format!(r#"<script type="module" src="/{}"></script>"#, entry.file);

            content = content.replace("<!--ssr:css-->", &css_tags);
            content = content.replace("<!--ssr:scripts-->", &script_tag);

            tracing::info!(
                "✅ Manifest parsed: JS={}, CSS files={}",
                entry.file,
                entry.css.len()
            );
        }

        // Replace ssr:head with empty (reserved for future use)
        content = content.replace("<!--ssr:head-->", "");

        Ok(Some(HtmlTemplate { content }))
    }

    /// Parse Vite manifest.json and find the entry chunk
    fn parse_manifest(path: &std::path::Path) -> SsrResult<ManifestEntry> {
        let raw = std::fs::read_to_string(path).map_err(|e| {
            SsrError::Template(format!("Failed to read manifest {:?}: {}", path, e))
        })?;

        let manifest: serde_json::Value = serde_json::from_str(&raw).map_err(|e| {
            SsrError::Template(format!("Failed to parse manifest JSON: {}", e))
        })?;

        let obj = manifest
            .as_object()
            .ok_or_else(|| SsrError::Template("Manifest is not a JSON object".into()))?;

        // Find entry with isEntry: true, or fall back to first entry
        let entry_value = obj
            .values()
            .find(|v| v.get("isEntry").and_then(|e| e.as_bool()).unwrap_or(false))
            .or_else(|| obj.values().next())
            .ok_or_else(|| SsrError::Template("Manifest has no entries".into()))?;

        let file = entry_value
            .get("file")
            .and_then(|f| f.as_str())
            .ok_or_else(|| SsrError::Template("Manifest entry missing 'file' field".into()))?
            .to_string();

        let css = entry_value
            .get("css")
            .and_then(|c| c.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();

        Ok(ManifestEntry { file, css })
    }

    /// Assemble the final document in a single pass: the rendered fragment
    /// goes into `<!--ssr:outlet-->`, and each caller-supplied placeholder is
    /// replaced in the same left-to-right scan. One allocation, and
    /// replacement values are never re-scanned (so a fragment that happens to
    /// contain another placeholder string is emitted verbatim).
    fn assemble(&self, fragment: &str, extra: &[(&str, &str)]) -> String {
        let mut subs: Vec<(&str, &str)> = Vec::with_capacity(extra.len() + 1);
        subs.push(("<!--ssr:outlet-->", fragment));
        subs.extend_from_slice(extra);
        multi_replace(&self.content, &subs)
    }
}

/// Replace multiple distinct needles in `template` in a single left-to-right
/// pass, producing one allocation.
///
/// Unlike chaining `str::replace` (one full scan + allocation per needle),
/// this walks the template once. Inserted replacement values are not
/// re-scanned, so a value containing another needle is emitted as-is.
fn multi_replace(template: &str, replacements: &[(&str, &str)]) -> String {
    // Capacity hint: assume each needle occurs ~once.
    let extra: usize = replacements
        .iter()
        .map(|&(n, v)| v.len().saturating_sub(n.len()))
        .sum();
    let mut out = String::with_capacity(template.len() + extra);

    // Where each needle next occurs, found once up front.
    //
    // The scan used to re-search *every* needle across the rest of the template
    // after *every* substitution, which for k placeholders and m matches walks
    // the document k·m times — on a page with five placeholders that is five
    // full passes over the whole document per placeholder found. A needle's
    // position only stops being valid when the cursor passes it, so almost all
    // of that searching was re-deriving answers already in hand.
    let mut next: Vec<Option<usize>> = replacements
        .iter()
        .map(|&(needle, _)| {
            if needle.is_empty() {
                // An empty needle matches everywhere and would never advance
                // the cursor. Ignored, as it always was.
                None
            } else {
                template.find(needle)
            }
        })
        .collect();

    let mut cursor = 0;
    loop {
        // Earliest match wins; a tie goes to whichever needle was listed first.
        let mut best: Option<(usize, usize)> = None; // (position, index)
        for (i, pos) in next.iter().enumerate() {
            if let Some(&p) = pos.as_ref() {
                if best.is_none_or(|(bp, _)| p < bp) {
                    best = Some((p, i));
                }
            }
        }

        let Some((pos, i)) = best else { break };
        let (needle, value) = replacements[i];

        out.push_str(&template[cursor..pos]);
        out.push_str(value);
        cursor = pos + needle.len();

        // Only the needles the cursor has just passed need looking up again —
        // the one consumed, plus any that overlapped it. Everything else still
        // points at a match that is still ahead.
        //
        // `None` is final, not "unknown": a needle with no occurrence at or
        // after some cursor has none at any later cursor either, because the
        // cursor only moves forward. Re-searching those would put the k·m
        // scanning straight back in for every placeholder already exhausted.
        for (j, slot) in next.iter_mut().enumerate() {
            if slot.is_some_and(|p| p < cursor) {
                let needle_j = replacements[j].0;
                *slot = template[cursor..].find(needle_j).map(|rel| cursor + rel);
            }
        }
    }

    out.push_str(&template[cursor..]);
    out
}

// ── SSR Engine ───────────────────────────────────────────────────────────────

/// The main SSR engine that coordinates V8 pool and caching
pub struct SsrEngine {
    config: SsrConfig,

    /// Pre-loaded HTML template (None = return raw fragments)
    template: Option<HtmlTemplate>,

    #[cfg(feature = "v8-pool")]
    v8_pool: V8Pool,

    #[cfg(feature = "cache")]
    cache: SsrCache,

    /// Finished documents, keyed by whatever the caller says decides them.
    /// `Arc` because a stale-while-revalidate rebuild outlives the request that
    /// noticed the page had expired.
    #[cfg(feature = "cache")]
    page_cache: Arc<PageCache>,
}

impl SsrEngine {
    /// Create a new configuration builder
    ///
    /// # Example
    /// ```rust,ignore
    /// use rusty_ssr::SsrEngine;
    ///
    /// let engine = SsrEngine::builder()
    ///     .bundle_path("ssr-bundle.js")
    ///     .pool_size(4)
    ///     .build_engine()
    ///     .expect("Failed to create engine");
    /// ```
    pub fn builder() -> SsrConfigBuilder {
        SsrConfigBuilder::default()
    }

    /// Create a new SSR engine with the given configuration
    pub fn new(config: SsrConfig) -> SsrResult<Self> {
        tracing::info!(
            "🚀 Initializing Rusty SSR engine (pool_size={}, cache_size={})",
            config.pool_size,
            config.cache_size
        );

        // Load HTML template + manifest (if configured)
        let template = HtmlTemplate::load(&config)?;
        if template.is_some() {
            tracing::info!("📄 HTML template system enabled");
        }

        #[cfg(feature = "v8-pool")]
        let v8_pool = {
            // Composed here and owned by the pool. It used to go into a
            // process-global `OnceLock`, which meant the *second* engine built
            // in a process silently rendered the *first* engine's bundle — and
            // meant a test binary could only ever hold one.
            let bundle: Arc<str> =
                Arc::from(crate::v8_pool::compose(&config.bundle_path, config.polyfills)?);

            V8Pool::new(crate::v8_pool::V8PoolConfig {
                num_threads: config.pool_size,
                queue_capacity: config.queue_capacity,
                pin_threads: config.pin_threads,
                request_timeout: config.request_timeout,
                render_function: config.render_function.clone(),
                max_heap_mb: config.max_heap_mb,
                bundle,
                seal_globals: config.seal_globals,
            })
            .map_err(SsrError::V8Init)?
        };

        #[cfg(feature = "cache")]
        let cache = {
            let ttl_secs = config.cache_ttl.map(|d| d.as_secs()).unwrap_or(0);
            SsrCache::with_ttl(config.cache_size, ttl_secs)
        };

        #[cfg(feature = "cache")]
        let page_cache = Arc::new(PageCache::new(config.page_cache));

        Ok(Self {
            config,
            template,
            #[cfg(feature = "v8-pool")]
            v8_pool,
            #[cfg(feature = "cache")]
            cache,
            #[cfg(feature = "cache")]
            page_cache,
        })
    }

    /// Render a URL to HTML
    ///
    /// This will first check the cache, and if not found, render via V8.
    ///
    /// # Arguments
    /// * `url` - The URL path to render (e.g., "/home", "/products/123")
    ///
    /// # Example
    /// ```rust,no_run
    /// # use rusty_ssr::SsrEngine;
    /// # async fn example(engine: SsrEngine) {
    /// let html = engine.render("/home").await.unwrap();
    /// # }
    /// ```
    ///
    /// # The render-function contract
    ///
    /// **The render function may be `async`, and this is the single most
    /// consequential thing to know about it.** The engine drives whatever the
    /// function returns to completion before handing you the result, so all
    /// three of these are supported and behave identically from Rust:
    ///
    /// ```js
    /// globalThis.renderPage = (url, data) => renderToString(<App />);
    /// globalThis.renderPage = async (url, data) => await renderToStringAsync(<App />);
    /// globalThis.renderPage = (url, data) => somethingReturningAPromise();
    /// ```
    ///
    /// A rejected promise becomes [`SsrError::JsExecution`](crate::SsrError)
    /// with the rejection's message, exactly like a synchronous throw.
    ///
    /// This matters more than it sounds, because the sync/async question is
    /// really the suspense question. Every major framework's *synchronous*
    /// renderer throws when a component suspends — a code-split route awaiting
    /// its chunk is the ordinary case — and the usual response is a `try/catch`
    /// returning `""`. That ships a blank page under a 200 status, and it is
    /// why [`min_render_bytes`](crate::SsrConfigBuilder::min_render_bytes)
    /// exists. The better answer is the framework's async renderer, and it has
    /// always been available here:
    /// `renderToStringAsync`, `renderToPipeableStream`'s promise form,
    /// `renderToStringAsync` in Solid, and so on.
    ///
    /// What it costs: the worker thread is occupied for the whole await. The
    /// pool is `pool_size` isolates and a blocked worker is not serving anyone
    /// else, so an `await` on real I/O — a database, an HTTP call — will
    /// saturate the pool far sooner than an `await` that settles in a
    /// microtask. Bundlers that inline dynamic imports (Vite's
    /// `inlineDynamicImports`, Rollup's single-file output) turn a lazy route
    /// into the microtask case, which is what makes async rendering cheap for
    /// the code-splitting scenario specifically. Set
    /// [`request_timeout`](crate::SsrConfigBuilder::request_timeout)
    /// accordingly — the default of 30 s is a library's default, not a page's.
    #[cfg(all(feature = "v8-pool", feature = "cache"))]
    pub async fn render(&self, url: &str) -> SsrResult<Arc<str>> {
        self.render_with_data(url, "{}").await
    }

    /// Render a URL to HTML with custom data
    ///
    /// # Arguments
    /// * `url` - The URL path to render
    /// * `data` - JSON string with data to pass to the render function
    #[cfg(all(feature = "v8-pool", feature = "cache"))]
    pub async fn render_with_data(&self, url: &str, data: &str) -> SsrResult<Arc<str>> {
        // Cache key covers BOTH url and data (the render depends on both), with
        // the URL normalized if a normalizer is configured. The original `url`
        // and `data` are still what we render.
        let key = self.compose_key(url, data);

        // Check cache first
        if let Some(cached) = self.cache.try_get(&key) {
            tracing::debug!("Cache hit: {}", url);
            return Ok(cached);
        }

        // Cache miss - render via V8
        tracing::debug!("Cache miss, rendering: {}", url);

        let html = self.finish(
            self.v8_pool
                .render_with_data(url.to_string(), data.to_string())
                .await,
        )?;

        let html: Arc<str> = Arc::from(html.as_str());

        // Store in cache. Skip empty renders when `cache_empty` is off — a
        // transient empty result (e.g. a suspended component returning "")
        // would otherwise persist, and with `cache_ttl = None` survive until
        // manual invalidation. Render *errors* never reach here: they
        // propagate as `Err` from the V8 pool and are returned uncached.
        if self.config.cache_empty || !html.is_empty() {
            self.cache.insert(&key, Arc::clone(&html));
        }

        Ok(html)
    }

    /// Render a URL with JSON data (serde_json::Value)
    ///
    /// Convenience method that serializes the Value to a string.
    ///
    /// # Example
    /// ```rust,no_run
    /// # use rusty_ssr::SsrEngine;
    /// # async fn example(engine: SsrEngine) {
    /// use serde_json::json;
    ///
    /// let data = json!({
    ///     "user": { "name": "John" },
    ///     "products": [1, 2, 3]
    /// });
    /// let html = engine.render_json("/products", data).await.unwrap();
    /// # }
    /// ```
    #[cfg(all(feature = "v8-pool", feature = "cache"))]
    pub async fn render_json(
        &self,
        url: &str,
        data: serde_json::Value,
    ) -> SsrResult<Arc<str>> {
        let data_str = data.to_string();
        self.render_with_data(url, &data_str).await
    }

    /// Render a URL and inject result into the HTML template
    ///
    /// If no template is configured, behaves identically to `render()`.
    /// When a template is set, the V8-rendered fragment is injected into
    /// `<!--ssr:outlet-->` and a complete HTML document is returned.
    ///
    /// # Example
    /// ```rust,no_run
    /// # use rusty_ssr::SsrEngine;
    /// # async fn example(engine: SsrEngine) {
    /// // Returns complete HTML document if template is configured
    /// let html = engine.render_to_html("/home").await.unwrap();
    /// // html = "<!doctype html><html>...<div id='root'>...app...</div>...</html>"
    /// # }
    /// ```
    #[cfg(all(feature = "v8-pool", feature = "cache"))]
    pub async fn render_to_html(&self, url: &str) -> SsrResult<String> {
        self.render_to_html_with_data(url, "{}").await
    }

    /// Render a URL with data and inject into the HTML template
    #[cfg(all(feature = "v8-pool", feature = "cache"))]
    pub async fn render_to_html_with_data(&self, url: &str, data: &str) -> SsrResult<String> {
        self.render_to_html_with_replacements(url, data, &[]).await
    }

    /// Render a URL and assemble the final document in a single pass,
    /// injecting the cached fragment into `<!--ssr:outlet-->` together with
    /// any caller-supplied `replacements` (e.g. per-request `<head>` tags).
    ///
    /// This avoids the repeated full-document allocations you'd get from
    /// chaining `String::replace` once per placeholder: the template is
    /// walked once and replacement values are not re-scanned.
    ///
    /// If no template is configured, the rendered fragment is returned as-is
    /// and `replacements` are ignored.
    ///
    /// # Example
    /// ```rust,no_run
    /// # use rusty_ssr::SsrEngine;
    /// # async fn example(engine: SsrEngine) {
    /// let title = "Listing #42";
    /// let head = "<meta property=\"og:title\" content=\"Listing #42\" />";
    /// let html = engine
    ///     .render_to_html_with_replacements(
    ///         "/?listing=42",
    ///         "{}",
    ///         &[("<!--ssr:title-->", title), ("<!--seo-->", head)],
    ///     )
    ///     .await
    ///     .unwrap();
    /// # }
    /// ```
    #[cfg(all(feature = "v8-pool", feature = "cache"))]
    pub async fn render_to_html_with_replacements(
        &self,
        url: &str,
        data: &str,
        replacements: &[(&str, &str)],
    ) -> SsrResult<String> {
        let fragment = self.render_with_data(url, data).await?;

        match &self.template {
            Some(tmpl) => Ok(tmpl.assemble(&fragment, replacements)),
            None => Ok(fragment.to_string()),
        }
    }

    /// Check if the HTML template system is enabled
    pub fn has_template(&self) -> bool {
        self.template.is_some()
    }

    /// Render without caching (always hits V8)
    ///
    /// The render function may be `async` — see [the render-function
    /// contract](Self::render#the-render-function-contract), which is the
    /// difference between a code-split route rendering and it shipping a blank
    /// body.
    #[cfg(feature = "v8-pool")]
    pub async fn render_uncached(&self, url: &str, data: &str) -> SsrResult<String> {
        self.finish(
            self.v8_pool
                .render_with_data(url.to_string(), data.to_string())
                .await,
        )
    }

    /// Render without caching, handing the bundle raw bytes.
    ///
    /// The payload arrives as a `Uint8Array` over the very buffer passed in —
    /// no copy on the Rust side, no decoding on the JS side. Reach for this
    /// whenever the data is genuinely binary (protobuf, MessagePack, an image):
    /// the alternative is base64 inside JSON, which costs a third more bytes on
    /// the way in *and* costs a base64 decode inside V8 on every render, which
    /// for a payload of any size is milliseconds spent undoing an encoding that
    /// existed only to fit the argument list.
    ///
    /// Uncached by design: bytes are a payload, and whether a payload belongs
    /// in a cache key is a question only the caller can answer — see
    /// [`RenderKey::variant_digest`](crate::cache::RenderKey::variant_digest)
    /// for the cheap way to say yes.
    ///
    /// ```rust,no_run
    /// # use rusty_ssr::SsrEngine;
    /// # async fn example(engine: SsrEngine, protobuf: Vec<u8>) {
    /// let html = engine.render_with_bytes("/catalog", protobuf).await.unwrap();
    /// # }
    /// ```
    /// ```js
    /// // …and in the bundle:
    /// globalThis.renderPage = (url, data) => {
    ///     const bytes = data instanceof Uint8Array ? data : null;
    ///     // decode straight from `bytes` — it is already the wire format
    /// };
    /// ```
    #[cfg(feature = "v8-pool")]
    pub async fn render_with_bytes(&self, url: &str, data: Vec<u8>) -> SsrResult<String> {
        self.finish(self.v8_pool.render_with_bytes(url.to_string(), data).await)
    }

    /// Render without caching, with a JSON envelope AND a binary payload: the
    /// bundle is called as `renderPage(url, data, bytes)`.
    ///
    /// This is the shape most page data actually has — a handful of scalars
    /// describing what the payload is, wrapped around one large blob. Squeezing
    /// both into the single JSON argument means base64-ing the blob into a
    /// string field and decoding it back inside V8, which for a payload of any
    /// size is milliseconds of every render spent on an encoding that existed
    /// only to satisfy the argument list.
    ///
    /// A bundle written as `function(url, data)` ignores the third argument, so
    /// this can be adopted one side at a time.
    ///
    /// ```rust,no_run
    /// # use rusty_ssr::SsrEngine;
    /// # async fn example(engine: SsrEngine, rows: Vec<u8>) {
    /// let html = engine
    ///     .render_with_json_and_bytes(
    ///         "/venda/blumenau",
    ///         r#"{"city":"Blumenau","page":1,"sort":"relevance"}"#,
    ///         rows,
    ///     )
    ///     .await
    ///     .unwrap();
    /// # }
    /// ```
    #[cfg(feature = "v8-pool")]
    pub async fn render_with_json_and_bytes(
        &self,
        url: &str,
        json: &str,
        bytes: Vec<u8>,
    ) -> SsrResult<String> {
        self.finish(
            self.v8_pool
                .render_with_json_and_bytes(url.to_string(), json.to_string(), bytes)
                .await,
        )
    }

    /// Render without caching with JSON data
    #[cfg(feature = "v8-pool")]
    pub async fn render_uncached_json(
        &self,
        url: &str,
        data: serde_json::Value,
    ) -> SsrResult<String> {
        self.render_uncached(url, &data.to_string()).await
    }

    /// Render without caching, injecting the result into the HTML template.
    ///
    /// Like [`render_to_html_with_data`](Self::render_to_html_with_data) but
    /// always hits V8 and never reads or writes the cache. Use for per-request
    /// or one-off URLs — auth tokens (`?reset=…`, `?verify=…`), campaign links
    /// (`?utm_*`, `?fbclid=…`) — that would otherwise pollute or thrash the
    /// fixed-size cache. If no template is configured, the raw fragment is
    /// returned.
    #[cfg(feature = "v8-pool")]
    pub async fn render_to_html_uncached(&self, url: &str, data: &str) -> SsrResult<String> {
        self.render_to_html_uncached_with_replacements(url, data, &[])
            .await
    }

    /// Uncached template render with single-pass placeholder replacement.
    ///
    /// Combines the one-pass assembly of
    /// [`render_to_html_with_replacements`](Self::render_to_html_with_replacements)
    /// with a full cache bypass.
    #[cfg(feature = "v8-pool")]
    pub async fn render_to_html_uncached_with_replacements(
        &self,
        url: &str,
        data: &str,
        replacements: &[(&str, &str)],
    ) -> SsrResult<String> {
        let fragment = self.render_uncached(url, data).await?;

        match &self.template {
            Some(tmpl) => Ok(tmpl.assemble(&fragment, replacements)),
            None => Ok(fragment),
        }
    }

    /// Turn what the pool returned into what the caller gets: the pool's error
    /// type mapped into the engine's, and the empty-render floor applied.
    ///
    /// Every path that reaches V8 ends this way. They each used to spell it
    /// out, which is one copy per entry point and one chance per copy for a new
    /// one to forget [`guard_empty`](Self::guard_empty) — and forgetting it
    /// does not fail, it serves a blank page under a 200.
    #[cfg(feature = "v8-pool")]
    fn finish(&self, rendered: Result<String, PoolError>) -> SsrResult<String> {
        self.guard_empty(rendered.map_err(Self::map_pool_error)?)
    }

    /// Refuse a render that produced (almost) nothing, when the caller has said
    /// what "nothing" means.
    ///
    /// The check is here rather than in the pool because it is a policy, not a
    /// mechanism: V8 did its job, the bundle returned a string, and whether a
    /// 12-byte string is a page is a question only the application can answer.
    #[cfg(feature = "v8-pool")]
    fn guard_empty(&self, html: String) -> SsrResult<String> {
        if let Some(min) = self.config.min_render_bytes {
            let len = html.trim().len();
            if len < min {
                return Err(SsrError::EmptyRender(len));
            }
        }
        Ok(html)
    }

    /// Apply the configured normalizer to a URL (identity if none set).
    #[cfg(feature = "cache")]
    fn cache_key<'a>(&self, url: &'a str) -> Cow<'a, str> {
        match self.config.cache_key_normalizer {
            Some(f) => Cow::Owned(f(url)),
            None => Cow::Borrowed(url),
        }
    }

    /// Compose the full cache key from a URL and render data.
    ///
    /// The key is `normalize(url) + SEP + data` so renders that differ only by
    /// data get distinct entries, and the URL part stays a prefix for
    /// URL-scoped invalidation.
    #[cfg(feature = "cache")]
    fn compose_key(&self, url: &str, data: &str) -> String {
        let normalized = self.cache_key(url);
        let mut key = String::with_capacity(normalized.len() + 1 + data.len());
        key.push_str(normalized.as_ref());
        key.push(CACHE_KEY_SEP);
        key.push_str(data);
        key
    }

    /// Invalidate every cached entry for a URL (all data variants)
    ///
    /// Use after content updates for a specific page. Because cache keys
    /// include the render data, this removes the page for *all* data variants.
    /// The URL is normalized with the configured `cache_key_normalizer` (if any).
    #[cfg(feature = "cache")]
    pub fn invalidate(&self, url: &str) {
        // Keys are `normalize(url) + SEP + data`; removing every variant means
        // removing all keys with this URL's prefix up to the separator.
        let prefix = format!("{}{}", self.cache_key(url), CACHE_KEY_SEP);
        let removed = self.cache.invalidate_prefix(&prefix);
        tracing::debug!("Cache invalidated {} entr{} for: {}", removed, if removed == 1 { "y" } else { "ies" }, url);
    }

    /// Invalidate all cached URLs matching a prefix
    ///
    /// Example: `engine.invalidate_prefix("/products")` clears all product pages.
    /// Returns the number of removed entries.
    #[cfg(feature = "cache")]
    pub fn invalidate_prefix(&self, prefix: &str) -> usize {
        let removed = self.cache.invalidate_prefix(prefix);
        tracing::info!("Cache invalidated {} entries with prefix: {}", removed, prefix);
        removed
    }

    /// Clear the SSR cache
    #[cfg(feature = "cache")]
    pub fn clear_cache(&self) {
        self.cache.clear();
        tracing::info!("SSR cache cleared");
    }

    /// Get cache metrics
    #[cfg(feature = "cache")]
    pub fn cache_metrics(&self) -> crate::cache::CacheMetrics {
        self.cache.metrics()
    }

    /// Get the number of active V8 workers
    #[cfg(feature = "v8-pool")]
    pub fn worker_count(&self) -> usize {
        self.v8_pool.worker_count()
    }

    /// Get a reference to the configuration
    pub fn config(&self) -> &SsrConfig {
        &self.config
    }

    /// Get a reference to the fragment cache (if enabled)
    #[cfg(feature = "cache")]
    pub fn cache(&self) -> &SsrCache {
        &self.cache
    }

    /// Whether the bundle's render function returned a value or a promise, as
    /// last observed.
    ///
    /// [`RenderFnShape::Unknown`](crate::v8_pool::RenderFnShape) until a render
    /// has completed — the shape is observed rather than declared, so there is
    /// nothing to report before one has.
    ///
    /// This exists to be *reported*, not branched on: both shapes render
    /// correctly and the engine treats them identically. What it is for is
    /// telling a developer something their bundle never says out loud — that a
    /// synchronous render function cannot render a suspending component, so
    /// every code-split route is being served as whatever placeholder the
    /// bundle falls back to. See [the render-function
    /// contract](Self::render#the-render-function-contract).
    #[cfg(feature = "v8-pool")]
    pub fn render_fn_shape(&self) -> crate::v8_pool::RenderFnShape {
        self.v8_pool.render_fn_shape()
    }

    /// The page cache — finished documents, keyed by [`RenderKey`].
    ///
    /// Use it directly for the lookup-and-store shape (`get` before the queries
    /// a build would run, `store` once the document is assembled), or go
    /// through [`page`](Self::page) to get single-flight and revalidation too.
    #[cfg(feature = "cache")]
    pub fn page_cache(&self) -> &Arc<PageCache> {
        &self.page_cache
    }

    /// The finished document for `key`, building it with `build` on a miss.
    ///
    /// This is the method most HTTP applications want, and the one the fragment
    /// cache behind [`render`](Self::render) cannot be: `build` returns the
    /// **whole response** — status and assembled document — so a hit skips not
    /// just the V8 render but every query and injection that went into the
    /// page. Concurrent callers for one cold key share a single build, and with
    /// a stale window configured nobody waits for a rebuild.
    ///
    /// ```rust,no_run
    /// # use rusty_ssr::{SsrEngine, cache::{BuiltPage, RenderKey}};
    /// # async fn example(engine: std::sync::Arc<SsrEngine>, host: String, path: String) {
    /// // The Host header decides the canonical URLs on the page, so it is part
    /// // of what the page *is* — and therefore part of the key.
    /// let key = RenderKey::new(&path).variant("host", &host);
    /// let engine2 = engine.clone();
    /// let page = engine
    ///     .page(&key, move || async move {
    ///         let fragment = engine2.render_uncached(&path, "{}").await?;
    ///         Ok(BuiltPage::ok(format!("<!doctype html><body>{fragment}")))
    ///     })
    ///     .await
    ///     .unwrap();
    /// // page.body is `Bytes` — answering is a refcount bump, not a copy.
    /// # }
    /// ```
    #[cfg(feature = "cache")]
    pub async fn page<F, Fut>(&self, key: &RenderKey, build: F) -> SsrResult<CachedPage>
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = SsrResult<BuiltPage>> + Send + 'static,
    {
        self.page_cache.get_or_build(key, build).await
    }

    /// What the render pool is doing right now.
    ///
    /// The companion to [`cache_metrics`](Self::cache_metrics), and the one that
    /// answers the operational question a cache cannot: *how close am I to
    /// capacity?* A pool serves at most `workers / render_time` requests per
    /// second, throughput stops rising the moment every worker is busy, and
    /// everything after that becomes queue delay. See
    /// [`PoolMetrics`](crate::v8_pool::PoolMetrics).
    #[cfg(feature = "v8-pool")]
    pub fn pool_metrics(&self) -> crate::v8_pool::PoolMetrics {
        self.v8_pool.metrics()
    }

    /// Get a reference to the V8 pool (if enabled)
    #[cfg(feature = "v8-pool")]
    pub fn v8_pool(&self) -> &V8Pool {
        &self.v8_pool
    }
}

/// Builder extension to create SsrEngine directly
impl SsrConfigBuilder {
    /// Build the configuration and create an SsrEngine
    pub fn build_engine(self) -> SsrResult<SsrEngine> {
        SsrEngine::new(self.build()?)
    }
}

impl SsrEngine {
    #[cfg(feature = "v8-pool")]
    fn map_pool_error(err: PoolError) -> SsrError {
        match err {
            PoolError::Timeout => SsrError::Timeout,
            PoolError::Disconnected => SsrError::PoolFull,
            PoolError::WorkerCrashed => {
                SsrError::JsExecution("V8 worker crashed".to_string())
            }
            PoolError::Render(msg) => SsrError::JsExecution(msg),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::multi_replace;

    #[test]
    fn single_placeholder() {
        let out = multi_replace("a<!--x-->b", &[("<!--x-->", "FRAG")]);
        assert_eq!(out, "aFRAGb");
    }

    #[test]
    fn multiple_placeholders_one_pass() {
        let tmpl = "<title><!--t--></title><head><!--seo--></head><body><!--ssr:outlet--></body>";
        let out = multi_replace(
            tmpl,
            &[
                ("<!--ssr:outlet-->", "<app/>"),
                ("<!--seo-->", "<meta/>"),
                ("<!--t-->", "Hello"),
            ],
        );
        assert_eq!(out, "<title>Hello</title><head><meta/></head><body><app/></body>");
    }

    #[test]
    fn replacement_values_are_not_rescanned() {
        // The fragment contains a literal that matches another needle; it must
        // be emitted verbatim, not re-replaced.
        let tmpl = "[<!--ssr:outlet-->][<!--seo-->]";
        let out = multi_replace(
            tmpl,
            &[
                ("<!--ssr:outlet-->", "contains <!--seo--> literal"),
                ("<!--seo-->", "TAGS"),
            ],
        );
        assert_eq!(out, "[contains <!--seo--> literal][TAGS]");
    }

    #[test]
    fn missing_placeholder_is_noop() {
        let out = multi_replace("no markers here", &[("<!--x-->", "y")]);
        assert_eq!(out, "no markers here");
    }

    #[test]
    fn repeated_placeholder_all_replaced() {
        let out = multi_replace("<!--x-->-<!--x-->", &[("<!--x-->", "Z")]);
        assert_eq!(out, "Z-Z");
    }

    #[test]
    fn empty_needle_is_skipped() {
        let out = multi_replace("abc", &[("", "X"), ("b", "B")]);
        assert_eq!(out, "aBc");
    }
}
