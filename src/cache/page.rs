//! The page cache: finished documents, keyed by whatever actually decides them.
//!
//! This is the tier [`SsrCache`](super::SsrCache) is not. That one stores the
//! *fragment* a render returned, under `url + data`. Three things go wrong when
//! a real application tries to use it as its page cache, and all three showed
//! up in the same production codebase:
//!
//! 1. **The fragment is not the page.** Between the render and the response
//!    there is usually per-request work the render cannot do — `<head>` tags
//!    from a database, a serialised store for the client to hydrate from, a
//!    status code. Caching the fragment means redoing all of it on every hit,
//!    and caching it *with* the fragment is impossible when the cache is keyed
//!    on the render alone.
//! 2. **The URL is not the key.** The same path under two hostnames is two
//!    documents as soon as anything absolute (`<link rel=canonical>`,
//!    `og:url`) is built from the Host header. Locale, device class and
//!    currency do the same thing. A cache whose key is fixed at `url + data`
//!    cannot express any of it.
//! 3. **`data` is the wrong thing to put in a key.** It is the payload, and
//!    payloads are large — a catalogue seed of tens of kilobytes gets hashed
//!    on every lookup to store an entry no second request will ever hit.
//!    Whether the data belongs in the key at all is a question only the caller
//!    can answer, and [`RenderKey`] is where they answer it.
//!
//! So: the caller says what the key is ([`RenderKey`]), the caller builds the
//! document, and this stores the finished bytes with their status. What the
//! cache adds on top is the part that is tedious and easy to get wrong —
//! expiry, capacity, [single-flight](PageCache::get_or_build) so a burst on a
//! cold key renders once instead of once per request, and
//! [stale-while-revalidate](CachePolicy::stale_while_revalidate) so a hot key
//! never makes anyone wait for a rebuild.

use std::borrow::Cow;
use std::collections::HashMap;
use std::future::Future;
use std::num::NonZeroUsize;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use bytes::Bytes;
use lru::LruCache;
use tokio::sync::broadcast;

use crate::error::{SsrError, SsrResult};

/// US-ASCII 0x1f (unit separator). It cannot appear unescaped in a URL, and it
/// is not something a sane variant name or value contains either, so joining
/// with it leaves an unambiguous boundary: `a\u{1f}b` cannot also be spelled
/// `a\u{1f}b` by different halves.
const SEP: char = '\u{1f}';

// ── Policy ───────────────────────────────────────────────────────────────────

/// How long pages live and how many are kept.
///
/// Deliberately not three loose numbers on a builder. The 0.1 line spelled
/// this as `cache_size(usize)` + `cache_ttl_secs(u64)` and both had a value
/// that meant something other than what it read as: `cache_ttl_secs(0)` meant
/// "never expires" rather than "do not cache", and `cache_size(0)` was refused
/// outright, so there was no way to say "off" at all. Here "off" is a variant
/// and cannot be confused with anything.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CachePolicy {
    /// Do not cache. Every lookup misses, nothing is stored, and single-flight
    /// still applies — concurrent requests for one key share one build.
    Off,
    /// Keep up to `capacity` pages, evicting least-recently-used.
    Keep {
        /// Maximum pages held at once.
        capacity: NonZeroUsize,
        /// How long a page stays fresh. `None` = it never expires and only
        /// capacity ever removes it.
        ttl: Option<Duration>,
        /// How long past `ttl` a page may still be served while a fresh one is
        /// built in the background. `None` = an expired page is a miss and the
        /// requester waits for the rebuild.
        ///
        /// Only meaningful together with `ttl`.
        stale_while_revalidate: Option<Duration>,
    },
}

impl CachePolicy {
    /// `capacity` pages, each fresh for `ttl`, no stale window.
    ///
    /// # Panics
    /// If `capacity` is zero — say [`CachePolicy::Off`] instead, which is what
    /// a zero capacity was trying to mean.
    pub fn ttl(capacity: usize, ttl: Duration) -> Self {
        Self::Keep {
            capacity: NonZeroUsize::new(capacity).expect("capacity 0 — use CachePolicy::Off"),
            ttl: Some(ttl),
            stale_while_revalidate: None,
        }
    }

    /// `capacity` pages that never expire; only eviction removes them.
    ///
    /// # Panics
    /// If `capacity` is zero — see [`CachePolicy::ttl`].
    pub fn forever(capacity: usize) -> Self {
        Self::Keep {
            capacity: NonZeroUsize::new(capacity).expect("capacity 0 — use CachePolicy::Off"),
            ttl: None,
            stale_while_revalidate: None,
        }
    }

    /// Serve a page for `window` past its TTL while a fresh one is built.
    ///
    /// This is the setting that decides whether a rebuild is something a
    /// visitor waits for. Without it, every expiry hands somebody the full cost
    /// of the render; with it, the page that expired is still answered in
    /// microseconds and the next one is ready before anyone notices.
    ///
    /// No-op on [`CachePolicy::Off`], and on a policy with no TTL (nothing ever
    /// becomes stale).
    pub fn stale_while_revalidate(self, window: Duration) -> Self {
        match self {
            Self::Keep { capacity, ttl, .. } => Self::Keep {
                capacity,
                ttl,
                stale_while_revalidate: Some(window),
            },
            Self::Off => Self::Off,
        }
    }

    fn capacity(&self) -> Option<NonZeroUsize> {
        match self {
            Self::Keep { capacity, .. } => Some(*capacity),
            Self::Off => None,
        }
    }
}

impl Default for CachePolicy {
    /// 300 pages, fresh for five minutes — the 0.1 defaults, restated.
    fn default() -> Self {
        Self::ttl(300, Duration::from_secs(300))
    }
}

// ── Key ──────────────────────────────────────────────────────────────────────

/// What decides which document a request gets.
///
/// The URL is always part of it. Everything else is a **variant** the caller
/// adds because it changes the bytes: the Host header when canonical URLs are
/// built from it, a locale, a device class, a currency, a logged-in-or-not
/// flag.
///
/// ## Whether to put the render data in the key
///
/// A judgement only the caller can make, so this type makes it explicit rather
/// than choosing for you:
///
/// - **Leave it out** when the URL (plus variants) determines the page, and the
///   data is merely how the server fetched it. Cheapest by far: a hit is
///   answered before the data is even fetched, so an expensive query never
///   runs. This is right for a page whose content is a pure function of its
///   address.
/// - **Put a digest in** with [`variant_digest`](Self::variant_digest) when the
///   data can change independently of the URL and a stale page would be wrong.
///   Hashing a payload costs microseconds; storing it in the key costs its full
///   size on every lookup, forever.
///
/// What you must not do is pass data that changes the output and not say so —
/// that is how a 0.1.0 caller ended up serving a body built from one request's
/// rows next to another request's hydration state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderKey {
    url: String,
    variants: Vec<(String, String)>,
}

impl RenderKey {
    /// Key a page on this URL (path + query, as handed to the render function).
    pub fn new(url: impl Into<String>) -> Self {
        Self { url: url.into(), variants: Vec::new() }
    }

    /// Add something other than the URL that decides the bytes.
    ///
    /// Order does not matter: variants are sorted when the key is built, so two
    /// call sites that add the same pair in different orders still name the
    /// same page.
    pub fn variant(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.variants.push((name.into(), value.into()));
        self
    }

    /// Add a variant that is a short, stable digest of `payload`.
    ///
    /// For keying on render data without paying its size: the digest is 16 hex
    /// characters whatever the payload weighs.
    pub fn variant_digest(self, name: impl Into<String>, payload: &[u8]) -> Self {
        self.variant(name, format!("{:016x}", digest(payload)))
    }

    /// The URL to render — never the key, which carries the variants too.
    pub fn url(&self) -> &str {
        &self.url
    }

    /// The string this page is stored under, borrowed when it can be.
    ///
    /// A key with no variants *is* its URL, so a lookup can use the URL the
    /// caller already owns instead of copying it. That covers the plain
    /// `RenderKey::new(path)` case entirely, and a page-cache hit is supposed
    /// to be the cheapest thing the engine does — an allocation to build a
    /// string identical to one already in hand is a strange thing to spend it
    /// on.
    fn key_ref(&self) -> Cow<'_, str> {
        if self.variants.is_empty() {
            Cow::Borrowed(&self.url)
        } else {
            Cow::Owned(self.cache_key())
        }
    }

    /// The string this page is stored under.
    fn cache_key(&self) -> String {
        if self.variants.is_empty() {
            return self.url.clone();
        }
        let mut sorted: Vec<&(String, String)> = self.variants.iter().collect();
        sorted.sort_unstable();
        let mut key = String::with_capacity(
            self.url.len() + sorted.iter().map(|(n, v)| n.len() + v.len() + 2).sum::<usize>(),
        );
        key.push_str(&self.url);
        for (name, value) in sorted {
            key.push(SEP);
            key.push_str(name);
            key.push(SEP);
            key.push_str(value);
        }
        key
    }
}

/// FNV-1a. Not cryptographic and not meant to be — this identifies a payload to
/// a cache, and a collision costs one wrong page in a store the caller can
/// clear, not a security boundary.
fn digest(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x1000_0000_01b3);
    }
    hash
}

// ── Page ─────────────────────────────────────────────────────────────────────

/// A finished response, ready to hand back untouched.
#[derive(Debug, Clone)]
pub struct CachedPage {
    /// The status the document was built with. A 404 body served under a 200 is
    /// a soft-404, so this travels with the bytes rather than being recomputed.
    pub status: u16,
    /// The document. `Bytes`, so answering from the cache is a refcount bump
    /// rather than a copy of the whole page.
    pub body: Bytes,
    /// Response headers the build asked for — usually none.
    pub headers: Vec<(String, String)>,
    /// How long ago it was built. Useful for an `Age:` header.
    pub age: Duration,
    /// True when this came from the stale-while-revalidate window, i.e. it is
    /// past its TTL and a rebuild is running behind it.
    pub stale: bool,
}

/// What a build step produces: a whole response.
///
/// Headers are here because without them a build cannot express a redirect, and
/// a caller that cannot cache its redirects has to decide *before* the cache
/// whether this URL is one — which means running the query that answers that
/// question on every request, hit or miss. The point of caching the finished
/// response is that a hit costs nothing; a mandatory pre-flight query would give
/// most of that back.
#[derive(Debug, Clone)]
pub struct BuiltPage {
    /// HTTP status.
    pub status: u16,
    /// The document. Empty for a redirect.
    pub body: String,
    /// Extra response headers. Usually empty.
    pub headers: Vec<(String, String)>,
}

impl BuiltPage {
    /// A document with a status and no extra headers.
    pub fn new(status: u16, body: impl Into<String>) -> Self {
        Self { status, body: body.into(), headers: Vec::new() }
    }

    /// A 200 with this body.
    pub fn ok(body: impl Into<String>) -> Self {
        Self::new(200, body)
    }

    /// A redirect: no body, just `Location`.
    pub fn redirect(status: u16, location: impl Into<String>) -> Self {
        Self {
            status,
            body: String::new(),
            headers: vec![("location".to_string(), location.into())],
        }
    }

    /// Add a response header.
    pub fn header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.push((name.into(), value.into()));
        self
    }
}

impl From<(u16, String)> for BuiltPage {
    fn from((status, body): (u16, String)) -> Self {
        Self::new(status, body)
    }
}

struct Entry {
    status: u16,
    body: Bytes,
    headers: Vec<(String, String)>,
    stored_at: Instant,
    /// Set while a background revalidation is in flight, so a burst of stale
    /// hits starts exactly one rebuild.
    refreshing: bool,
}

// ── Cache ────────────────────────────────────────────────────────────────────

/// Finished pages, plus the single-flight and revalidation machinery around
/// them.
pub struct PageCache {
    policy: CachePolicy,
    entries: Mutex<Option<LruCache<String, Entry>>>,
    /// Keys with a build in flight, and the channel its result goes out on.
    inflight: Mutex<HashMap<String, broadcast::Sender<Result<CachedPage, String>>>>,
}

impl PageCache {
    /// Create a cache under `policy`.
    pub fn new(policy: CachePolicy) -> Self {
        Self {
            policy,
            entries: Mutex::new(policy.capacity().map(LruCache::new)),
            inflight: Mutex::new(HashMap::new()),
        }
    }

    /// The policy in force.
    pub fn policy(&self) -> CachePolicy {
        self.policy
    }

    /// The page stored under `key`, if there is one and it may still be served.
    ///
    /// A page past its TTL is returned only when the policy has a stale window
    /// and the page is inside it, flagged `stale: true`. Note this does **not**
    /// start a revalidation — [`get_or_build`](Self::get_or_build) is what
    /// knows how to rebuild. Reach for this directly only when you have no
    /// build step to offer.
    pub fn get(&self, key: &RenderKey) -> Option<CachedPage> {
        self.get_by_key(&key.key_ref())
    }

    /// [`get`](Self::get), for a caller that already has the composed key.
    ///
    /// Building that key means sorting the variants and allocating a string,
    /// and a single request used to do it three or four times over — once to
    /// look up, once to claim a refresh, once for single-flight, once to store
    /// — producing the identical string each time. Every step now takes the
    /// one that was built at the top of the request.
    fn get_by_key(&self, cache_key: &str) -> Option<CachedPage> {
        let (ttl, swr) = match self.policy {
            CachePolicy::Off => return None,
            CachePolicy::Keep { ttl, stale_while_revalidate, .. } => (ttl, stale_while_revalidate),
        };
        let mut guard = self.entries.lock().ok()?;
        let entries = guard.as_mut()?;
        let entry = entries.get(cache_key)?;
        let age = entry.stored_at.elapsed();

        let stale = match ttl {
            None => false,
            Some(ttl) if age <= ttl => false,
            Some(ttl) => {
                // Past the TTL. Serveable only inside the stale window; beyond
                // it the entry is dead and goes now rather than lingering to be
                // re-examined on every future lookup.
                let window = swr.unwrap_or_default();
                if age > ttl + window {
                    entries.pop(cache_key);
                    return None;
                }
                true
            }
        };

        Some(CachedPage {
            status: entry.status,
            body: entry.body.clone(),
            headers: entry.headers.clone(),
            age,
            stale,
        })
    }

    /// Store a finished page and hand back the buffer to answer this request
    /// with, so the caller does not clone the document it just built.
    ///
    /// Under [`CachePolicy::Off`] this stores nothing and just converts.
    pub fn store(&self, key: &RenderKey, page: BuiltPage) -> CachedPage {
        self.store_at(key.cache_key(), page)
    }

    /// [`store`](Self::store), for a caller that already has the composed key.
    fn store_at(&self, cache_key: String, page: BuiltPage) -> CachedPage {
        let BuiltPage { status, body, headers } = page;
        let body = Bytes::from(body);
        if let Ok(mut guard) = self.entries.lock() {
            if let Some(entries) = guard.as_mut() {
                entries.put(
                    cache_key,
                    Entry {
                        status,
                        body: body.clone(),
                        headers: headers.clone(),
                        stored_at: Instant::now(),
                        refreshing: false,
                    },
                );
            }
        }
        CachedPage { status, body, headers, age: Duration::ZERO, stale: false }
    }

    /// Drop the page stored under `key`. Returns whether there was one.
    pub fn invalidate(&self, key: &RenderKey) -> bool {
        match self.entries.lock() {
            Ok(mut guard) => guard.as_mut().and_then(|e| e.pop(&key.cache_key())).is_some(),
            Err(_) => false,
        }
    }

    /// Drop every page whose key starts with `prefix`. Returns how many went.
    pub fn invalidate_prefix(&self, prefix: &str) -> usize {
        let Ok(mut guard) = self.entries.lock() else { return 0 };
        let Some(entries) = guard.as_mut() else { return 0 };
        let doomed: Vec<String> = entries
            .iter()
            .map(|(k, _)| k)
            .filter(|k| k.starts_with(prefix))
            .cloned()
            .collect();
        for key in &doomed {
            entries.pop(key);
        }
        doomed.len()
    }

    /// Drop everything.
    pub fn clear(&self) {
        if let Ok(mut guard) = self.entries.lock() {
            if let Some(entries) = guard.as_mut() {
                entries.clear();
            }
        }
    }

    /// How many pages are held.
    pub fn len(&self) -> usize {
        self.entries.lock().ok().and_then(|g| g.as_ref().map(|e| e.len())).unwrap_or(0)
    }

    /// Whether no pages are held.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The page for `key`, building it with `build` if there isn't a fresh one.
    ///
    /// This is the method that earns the type. It gives, in order:
    ///
    /// - a **fresh hit**, answered without touching `build` at all — so any
    ///   query the build would have run does not run;
    /// - a **stale hit** inside the revalidation window, answered immediately
    ///   while exactly one background rebuild starts (later stale hits join the
    ///   ride rather than starting their own);
    /// - a **miss**, where the first caller runs `build` and every other caller
    ///   for the same key waits on that one result instead of running their own.
    ///   A crawler that opens forty tabs on a cold page costs one render.
    ///
    /// `build` returns the status and the finished document — everything the
    /// response needs, assembled. It is `Send + 'static` because a stale-window
    /// rebuild outlives the request that noticed it.
    ///
    /// Takes `&Arc<Self>` for the same reason: the background rebuild needs a
    /// handle to the cache that survives the request.
    pub async fn get_or_build<F, Fut>(
        self: &Arc<Self>,
        key: &RenderKey,
        build: F,
    ) -> SsrResult<CachedPage>
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = SsrResult<BuiltPage>> + Send + 'static,
    {
        // Composed once and handed to every step below. See
        // [`get_by_key`](Self::get_by_key) for what this used to cost. Borrowed
        // rather than built when the key has no variants, so a hit -- the case
        // this method exists to make fast -- allocates nothing at all.
        //
        // Scoped, and deliberately so. Everything a hit touches is confined to
        // this block, which returns before the function's only `await`, so none
        // of it becomes part of the future this method compiles into. A `Cow`
        // left alive across that await would be carried by every poll of it,
        // including the polls of the hit path that never needed it.
        let cache_key = {
            let cache_key = key.key_ref();

            if let Some(page) = self.get_by_key(&cache_key) {
                if !page.stale {
                    return Ok(page);
                }
                // Stale but serveable. Start at most one rebuild behind it and
                // answer now — the whole point of the window is that nobody waits.
                if self.claim_refresh(&cache_key) {
                    let cache = Arc::clone(self);
                    let url = key.url().to_string();
                    let cache_key = cache_key.into_owned();
                    tokio::spawn(async move { cache.refresh(cache_key, url, build).await });
                }
                return Ok(page);
            }

            cache_key.into_owned()
        };

        self.build_single_flight(cache_key, build).await
    }

    /// Mark the entry as being refreshed, returning false if someone already
    /// had. Keeps a burst of stale hits down to one rebuild.
    fn claim_refresh(&self, cache_key: &str) -> bool {
        let Ok(mut guard) = self.entries.lock() else { return false };
        let Some(entries) = guard.as_mut() else { return false };
        match entries.peek_mut(cache_key) {
            Some(entry) if !entry.refreshing => {
                entry.refreshing = true;
                true
            }
            _ => false,
        }
    }

    /// Undo [`claim_refresh`](Self::claim_refresh). Only needed when the
    /// rebuild failed: a successful one replaces the entry outright, and the
    /// replacement is not refreshing. Without this a single failed rebuild
    /// would leave the key marked forever and no later request would ever try
    /// again — it would serve the stale page until the entry was evicted.
    fn release_refresh(&self, cache_key: &str) {
        if let Ok(mut guard) = self.entries.lock() {
            if let Some(entries) = guard.as_mut() {
                if let Some(entry) = entries.peek_mut(cache_key) {
                    entry.refreshing = false;
                }
            }
        }
    }

    /// The background half of stale-while-revalidate.
    ///
    /// Errors are logged and dropped on purpose: nobody is waiting on this, and
    /// the stale page the visitor already received is a better answer than any
    /// error this could raise. The claim is released so the next stale hit
    /// tries again.
    async fn refresh<F, Fut>(&self, cache_key: String, url: String, build: F)
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = SsrResult<BuiltPage>> + Send,
    {
        match build().await {
            Ok(page) => {
                self.store_at(cache_key, page);
                tracing::debug!(url = %url, "revalidated stale page");
            }
            Err(e) => {
                tracing::warn!(url = %url, error = %e, "stale revalidation failed");
                self.release_refresh(&cache_key);
            }
        }
    }

    /// Run `build` unless another caller is already building this key, in which
    /// case wait for theirs.
    async fn build_single_flight<F, Fut>(
        &self,
        cache_key: String,
        build: F,
    ) -> SsrResult<CachedPage>
    where
        F: FnOnce() -> Fut + Send,
        Fut: Future<Output = SsrResult<BuiltPage>> + Send,
    {
        // Either become the builder, or subscribe to the one that exists.
        let follower = {
            let mut inflight = self
                .inflight
                .lock()
                .map_err(|_| SsrError::Cache("page cache in-flight map poisoned".into()))?;
            match inflight.get(&cache_key) {
                Some(tx) => Some(tx.subscribe()),
                None => {
                    let (tx, _rx) = broadcast::channel(1);
                    inflight.insert(cache_key.clone(), tx);
                    None
                }
            }
        };

        if let Some(mut rx) = follower {
            return match rx.recv().await {
                Ok(Ok(page)) => Ok(page),
                Ok(Err(msg)) => Err(SsrError::JsExecution(msg)),
                // The leader dropped without publishing — its task was
                // cancelled, or the client that started it went away. Answer
                // from the cache if the leader got far enough to store, and
                // otherwise say so rather than hanging.
                Err(_) => self
                    .get_by_key(&cache_key)
                    .ok_or_else(|| SsrError::Cache("page build was abandoned".into())),
            };
        }

        // We are the leader, and from here the slot MUST disappear whatever
        // happens — including this future being dropped mid-build, which is
        // routine: it happens every time the client that started the request
        // goes away, or a timeout layer above us gives up.
        //
        // A line at the end of the function does not cover that case, because a
        // dropped future never reaches the end of the function. The sender would
        // stay in the map, and every later request for this key would subscribe
        // to a channel nobody will ever send on and nobody will ever drop —
        // a permanent hang, for one URL, clearing only on restart. Hence a
        // guard: `Drop` runs on the cancellation path too.
        let _flight = Flight { inflight: &self.inflight, key: &cache_key };

        let built = build().await;
        let result = match built {
            Ok(page) => Ok(self.store_at(cache_key.clone(), page)),
            Err(e) => Err(e),
        };

        let published: Result<CachedPage, String> = match &result {
            Ok(page) => Ok(page.clone()),
            Err(e) => Err(e.to_string()),
        };

        // Send while the slot is still visible — a follower that subscribes
        // between a removal and a send would never hear anything. `_flight`
        // removes it immediately afterwards, on the way out of this scope.
        if let Ok(inflight) = self.inflight.lock() {
            if let Some(tx) = inflight.get(&cache_key) {
                let _ = tx.send(published);
            }
        }

        result
    }
}

/// Removes an in-flight slot on the way out, however the way out happens.
///
/// See the comment at its construction: the case this exists for is the leader's
/// future being *dropped*, not returning.
struct Flight<'a> {
    inflight: &'a Mutex<HashMap<String, broadcast::Sender<Result<CachedPage, String>>>>,
    key: &'a str,
}

impl Drop for Flight<'_> {
    fn drop(&mut self) {
        if let Ok(mut inflight) = self.inflight.lock() {
            inflight.remove(self.key);
        }
    }
}

#[cfg(test)]
impl PageCache {
    /// Backdate an entry so expiry can be tested without sleeping through a
    /// TTL. Returns false on a machine whose monotonic clock has not run that
    /// long yet (a freshly booted CI box), where the subtraction has no result.
    fn age(&self, key: &RenderKey, by: Duration) -> bool {
        let mut guard = self.entries.lock().unwrap();
        let Some(entries) = guard.as_mut() else { return false };
        let Some(entry) = entries.peek_mut(&key.cache_key()) else { return false };
        match entry.stored_at.checked_sub(by) {
            Some(then) => {
                entry.stored_at = then;
                true
            }
            None => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    const TTL: Duration = Duration::from_secs(300);

    fn key(url: &str) -> RenderKey {
        RenderKey::new(url)
    }

    fn cache(policy: CachePolicy) -> Arc<PageCache> {
        Arc::new(PageCache::new(policy))
    }

    fn body(page: &CachedPage) -> String {
        String::from_utf8(page.body.to_vec()).unwrap()
    }

    // ── keys ────────────────────────────────────────────────────────────────

    /// The URL handed to the render function is not the key: the key carries
    /// the variants too, and the render must not see them.
    #[test]
    fn the_url_and_the_key_are_different_things() {
        let k = key("/venda/blumenau").variant("host", "morada.test");
        assert_eq!(k.url(), "/venda/blumenau");
        assert_ne!(k.cache_key(), "/venda/blumenau");
        assert!(k.cache_key().starts_with("/venda/blumenau"));
    }

    /// Every canonical and og:url on a page can be built from the Host header,
    /// so one path under two hostnames is two documents.
    #[test]
    fn a_variant_makes_two_pages_of_one_url() {
        let c = cache(CachePolicy::default());
        let a = key("/venda/blumenau").variant("host", "morada.test");
        let b = key("/venda/blumenau").variant("host", "outra.test");
        c.store(&a, BuiltPage::ok("canonical=morada"));
        c.store(&b, BuiltPage::ok("canonical=outra"));
        assert_eq!(body(&c.get(&a).unwrap()), "canonical=morada");
        assert_eq!(body(&c.get(&b).unwrap()), "canonical=outra");
    }

    /// Two call sites that name the same page must not miss each other because
    /// they listed the variants in a different order.
    #[test]
    fn variant_order_does_not_change_the_key() {
        let one = key("/x").variant("host", "a").variant("locale", "pt");
        let two = key("/x").variant("locale", "pt").variant("host", "a");
        assert_eq!(one.cache_key(), two.cache_key());
    }

    /// The separator has to be something neither half can contain, or two
    /// different (url, variant) sets could spell one key.
    #[test]
    fn key_halves_cannot_run_together() {
        let a = key("/a").variant("b", "c");
        let b = key("/a\u{1f}b").variant("", "c");
        assert_ne!(a.cache_key(), b.cache_key());
    }

    /// The whole point of a digest: it identifies the payload without storing
    /// it, so a 90 kB seed costs sixteen characters rather than 90 kB.
    #[test]
    fn a_digest_variant_is_short_whatever_the_payload_weighs() {
        let big = vec![b'x'; 90_000];
        let k = key("/x").variant_digest("seed", &big);
        assert!(k.cache_key().len() < 64, "key was {} chars", k.cache_key().len());
        assert_ne!(
            k.cache_key(),
            key("/x").variant_digest("seed", b"different").cache_key()
        );
        // Same payload, same key — or nothing would ever hit.
        assert_eq!(
            key("/x").variant_digest("seed", &big).cache_key(),
            key("/x").variant_digest("seed", &big).cache_key()
        );
    }

    // ── storage ─────────────────────────────────────────────────────────────

    /// A 404 is a rendered page too, and the status has to survive the round
    /// trip or a crawler reads a probe like /wp-admin as a real page.
    #[test]
    fn the_status_is_stored_with_the_body() {
        let c = cache(CachePolicy::default());
        let k = key("/wp-admin");
        c.store(&k, BuiltPage::new(404, "<html>404"));
        let hit = c.get(&k).unwrap();
        assert_eq!(hit.status, 404);
        assert_eq!(body(&hit), "<html>404");
        assert!(!hit.stale);
    }

    /// `store` hands back the buffer it stored, so the caller answers this
    /// request without cloning the document it just built.
    #[test]
    fn store_returns_the_body_it_stored() {
        let c = cache(CachePolicy::default());
        let returned = c.store(&key("/x"), BuiltPage::ok("page"));
        assert_eq!(&returned.body[..], b"page");
        assert_eq!(body(&c.get(&key("/x")).unwrap()), "page");
    }

    #[test]
    fn invalidate_drops_one_page_and_prefix_drops_a_family() {
        let c = cache(CachePolicy::default());
        for url in ["/venda/a", "/venda/b", "/aluguel/a"] {
            c.store(&key(url), BuiltPage::ok(url));
        }
        assert!(c.invalidate(&key("/venda/a")));
        assert!(!c.invalidate(&key("/venda/a")), "already gone");
        assert_eq!(c.invalidate_prefix("/venda"), 1);
        assert!(c.get(&key("/aluguel/a")).is_some(), "an unrelated family stays");
    }

    // ── policy ──────────────────────────────────────────────────────────────

    /// `Off` has to mean off. The 0.1 line could not say this at all: a zero
    /// capacity was rejected by the builder and a zero TTL meant "forever".
    #[test]
    fn off_stores_nothing() {
        let c = cache(CachePolicy::Off);
        let k = key("/x");
        assert_eq!(&c.store(&k, BuiltPage::ok("page")).body[..], b"page", "still converts");
        assert!(c.get(&k).is_none());
        assert!(c.is_empty());
    }

    #[test]
    fn forever_never_expires() {
        let c = cache(CachePolicy::forever(8));
        let k = key("/x");
        c.store(&k, BuiltPage::ok("page"));
        if !c.age(&k, Duration::from_secs(86_400 * 365)) {
            return;
        }
        let hit = c.get(&k).expect("a page with no TTL cannot expire");
        assert!(!hit.stale);
    }

    #[test]
    fn an_expired_page_is_a_miss_and_is_dropped() {
        let c = cache(CachePolicy::ttl(8, TTL));
        let k = key("/x");
        c.store(&k, BuiltPage::ok("old"));
        if !c.age(&k, TTL + Duration::from_secs(1)) {
            return;
        }
        assert!(c.get(&k).is_none());
        assert!(c.is_empty(), "the miss dropped it rather than leaving it to be re-checked");
    }

    #[test]
    fn the_least_recently_used_page_goes_when_the_cache_is_full() {
        let c = cache(CachePolicy::ttl(3, TTL));
        for i in 0..3 {
            c.store(&key(&format!("/p{i}")), BuiltPage::ok(format!("page {i}")));
        }
        // Touch /p0 so it is no longer the least recently used.
        assert!(c.get(&key("/p0")).is_some());
        c.store(&key("/p3"), BuiltPage::ok("newest"));

        assert_eq!(c.len(), 3);
        assert!(c.get(&key("/p3")).is_some(), "the new page is in");
        assert!(c.get(&key("/p0")).is_some(), "the recently used page survived");
        assert!(c.get(&key("/p1")).is_none(), "the least recently used one went");
    }

    // ── stale-while-revalidate ──────────────────────────────────────────────

    #[test]
    fn a_stale_page_is_served_inside_the_window_and_not_outside_it() {
        let window = Duration::from_secs(60);
        let c = cache(CachePolicy::ttl(8, TTL).stale_while_revalidate(window));
        let k = key("/x");

        c.store(&k, BuiltPage::ok("page"));
        if !c.age(&k, TTL + Duration::from_secs(1)) {
            return;
        }
        let hit = c.get(&k).expect("inside the window it is still serveable");
        assert!(hit.stale, "and it says so");
        assert_eq!(body(&hit), "page");

        assert!(c.age(&k, window));
        assert!(c.get(&k).is_none(), "past the window it is a miss");
    }

    /// Without a window, past the TTL is simply gone — no accidental serving of
    /// stale bytes to a caller that never asked for it.
    #[test]
    fn no_window_means_no_stale_serving() {
        let c = cache(CachePolicy::ttl(8, TTL));
        let k = key("/x");
        c.store(&k, BuiltPage::ok("page"));
        if !c.age(&k, TTL + Duration::from_secs(1)) {
            return;
        }
        assert!(c.get(&k).is_none());
    }

    // ── get_or_build ────────────────────────────────────────────────────────

    #[tokio::test]
    async fn a_fresh_hit_never_runs_the_build() {
        let c = cache(CachePolicy::default());
        let k = key("/x");
        c.store(&k, BuiltPage::ok("cached"));

        let ran = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&ran);
        let page = c
            .get_or_build(&k, move || async move {
                counter.fetch_add(1, Ordering::SeqCst);
                Ok(BuiltPage::ok("rebuilt"))
            })
            .await
            .unwrap();

        assert_eq!(body(&page), "cached");
        assert_eq!(ran.load(Ordering::SeqCst), 0, "the build must not have run");
    }

    #[tokio::test]
    async fn a_miss_builds_and_stores() {
        let c = cache(CachePolicy::default());
        let k = key("/x");
        let page = c
            .get_or_build(&k, || async { Ok(BuiltPage::new(404, "built")) })
            .await
            .unwrap();
        assert_eq!(page.status, 404);
        assert_eq!(body(&page), "built");
        assert_eq!(c.get(&k).unwrap().status, 404, "and it is in the cache now");
    }

    /// The crawler case: forty requests arrive for one cold page. Exactly one
    /// render happens and the other thirty-nine wait on it.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn a_burst_on_a_cold_key_builds_once() {
        let c = cache(CachePolicy::default());
        let k = key("/expensive");
        let builds = Arc::new(AtomicUsize::new(0));

        let mut tasks = Vec::new();
        for _ in 0..40 {
            let c = Arc::clone(&c);
            let k = k.clone();
            let builds = Arc::clone(&builds);
            tasks.push(tokio::spawn(async move {
                c.get_or_build(&k, move || async move {
                    builds.fetch_add(1, Ordering::SeqCst);
                    tokio::time::sleep(Duration::from_millis(50)).await;
                    Ok(BuiltPage::ok("rendered once"))
                })
                .await
            }));
        }

        for task in tasks {
            let page = task.await.unwrap().unwrap();
            assert_eq!(body(&page), "rendered once");
        }
        assert_eq!(builds.load(Ordering::SeqCst), 1, "one render for forty requests");
    }

    /// A failing build must not leave the followers hanging on a slot that
    /// never publishes.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn a_failing_build_fails_every_waiter_rather_than_hanging() {
        let c = cache(CachePolicy::default());
        let k = key("/broken");

        let mut tasks = Vec::new();
        for _ in 0..8 {
            let c = Arc::clone(&c);
            let k = k.clone();
            tasks.push(tokio::spawn(async move {
                c.get_or_build(&k, || async {
                    tokio::time::sleep(Duration::from_millis(20)).await;
                    Err(SsrError::JsExecution("boom".into()))
                })
                .await
                .is_err()
            }));
        }

        for task in tasks {
            assert!(task.await.unwrap(), "every caller has to hear about it");
        }
        assert!(c.is_empty(), "and nothing was cached");
    }

    /// The stale hit is answered immediately — the visitor does not wait for
    /// the rebuild — and exactly one rebuild runs however many stale hits land.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn a_stale_hit_answers_now_and_refreshes_once_behind_it() {
        let c = cache(CachePolicy::ttl(8, TTL).stale_while_revalidate(Duration::from_secs(60)));
        let k = key("/x");
        c.store(&k, BuiltPage::ok("stale"));
        if !c.age(&k, TTL + Duration::from_secs(1)) {
            return;
        }

        let builds = Arc::new(AtomicUsize::new(0));
        for _ in 0..5 {
            let counter = Arc::clone(&builds);
            let page = c
                .get_or_build(&k, move || async move {
                    counter.fetch_add(1, Ordering::SeqCst);
                    tokio::time::sleep(Duration::from_millis(30)).await;
                    Ok(BuiltPage::ok("fresh"))
                })
                .await
                .unwrap();
            assert!(page.stale, "answered from the stale copy, without waiting");
            assert_eq!(body(&page), "stale");
        }

        // Let the background rebuild land.
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert_eq!(builds.load(Ordering::SeqCst), 1, "five stale hits, one rebuild");
        let after = c.get(&k).unwrap();
        assert_eq!(body(&after), "fresh");
        assert!(!after.stale, "and it is fresh again");
    }

    /// A rebuild that fails must not wedge the key: the next stale hit has to
    /// be allowed to try again, or the page stays stale until it is evicted.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn a_failed_revalidation_lets_the_next_one_try() {
        let c = cache(CachePolicy::ttl(8, TTL).stale_while_revalidate(Duration::from_secs(60)));
        let k = key("/x");
        c.store(&k, BuiltPage::ok("stale"));
        if !c.age(&k, TTL + Duration::from_secs(1)) {
            return;
        }

        let page = c
            .get_or_build(&k, || async { Err(SsrError::JsExecution("boom".into())) })
            .await
            .unwrap();
        assert!(page.stale, "the visitor still got an answer");
        tokio::time::sleep(Duration::from_millis(150)).await;

        let builds = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&builds);
        let _ = c
            .get_or_build(&k, move || async move {
                counter.fetch_add(1, Ordering::SeqCst);
                Ok(BuiltPage::ok("fresh"))
            })
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(150)).await;
        assert_eq!(builds.load(Ordering::SeqCst), 1, "the second attempt was allowed");
    }

    // ── cancellation ────────────────────────────────────────────────────────

    /// The case a line at the end of the function cannot cover: the leader's
    /// future is dropped mid-build, which is what happens every time a client
    /// disconnects or a timeout layer above gives up.
    ///
    /// Without the `Flight` guard the sender stays in the in-flight map, and
    /// every later request for this key subscribes to a channel nobody will
    /// ever send on and nobody will ever drop. That is a permanent hang for one
    /// URL, and the only cure is a restart — so this test is written as "does
    /// the NEXT request still work", with a timeout, rather than as an
    /// assertion about the map.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn a_cancelled_leader_does_not_wedge_the_key() {
        let c = cache(CachePolicy::default());
        let k = key("/slow");

        let leader = {
            let c = Arc::clone(&c);
            let k = k.clone();
            tokio::spawn(async move {
                c.get_or_build(&k, || async {
                    tokio::time::sleep(Duration::from_secs(30)).await;
                    Ok(BuiltPage::ok("never finishes"))
                })
                .await
            })
        };

        // Let it become the leader, then take it away.
        tokio::time::sleep(Duration::from_millis(50)).await;
        leader.abort();
        let _ = leader.await;

        let page = tokio::time::timeout(
            Duration::from_secs(5),
            c.get_or_build(&k, || async { Ok(BuiltPage::ok("second attempt")) }),
        )
        .await
        .expect("the key was wedged by the cancelled leader")
        .unwrap();
        assert_eq!(body(&page), "second attempt");
    }

    /// A follower already waiting when the leader vanishes must be told, not
    /// left on the channel.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn a_follower_of_a_vanished_leader_is_not_left_waiting() {
        let c = cache(CachePolicy::default());
        let k = key("/slow");

        let leader = {
            let c = Arc::clone(&c);
            let k = k.clone();
            tokio::spawn(async move {
                c.get_or_build(&k, || async {
                    tokio::time::sleep(Duration::from_secs(30)).await;
                    Ok(BuiltPage::ok("never finishes"))
                })
                .await
            })
        };
        tokio::time::sleep(Duration::from_millis(50)).await;

        let follower = {
            let c = Arc::clone(&c);
            let k = k.clone();
            tokio::spawn(async move {
                c.get_or_build(&k, || async { Ok(BuiltPage::ok("follower's own build")) })
                    .await
            })
        };
        tokio::time::sleep(Duration::from_millis(50)).await;

        leader.abort();
        let _ = leader.await;

        let outcome = tokio::time::timeout(Duration::from_secs(5), follower)
            .await
            .expect("the follower hung on a channel that will never send")
            .unwrap();
        // Either answer is correct — what must not happen is waiting forever.
        // Nothing was cached, so in practice this is the "abandoned" error.
        assert!(outcome.is_err() || !body(&outcome.unwrap()).is_empty());
    }

    // ── policy edges ────────────────────────────────────────────────────────

    /// `Off` disables *storage*, not coalescing. Two requests for one cold key
    /// still share a build, which is the difference between a thundering herd
    /// and one render even when nothing is kept.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn off_still_coalesces_concurrent_builds() {
        let c = cache(CachePolicy::Off);
        let k = key("/x");
        let builds = Arc::new(AtomicUsize::new(0));

        let mut tasks = Vec::new();
        for _ in 0..8 {
            let c = Arc::clone(&c);
            let k = k.clone();
            let builds = Arc::clone(&builds);
            tasks.push(tokio::spawn(async move {
                c.get_or_build(&k, move || async move {
                    builds.fetch_add(1, Ordering::SeqCst);
                    tokio::time::sleep(Duration::from_millis(50)).await;
                    Ok(BuiltPage::ok("shared"))
                })
                .await
            }));
        }
        for task in tasks {
            assert_eq!(body(&task.await.unwrap().unwrap()), "shared");
        }

        assert_eq!(builds.load(Ordering::SeqCst), 1, "Off must still coalesce");
        assert!(c.is_empty(), "…and still store nothing");
    }

    /// A zero capacity is the thing `Off` exists to say. Accepting it silently
    /// would recreate the 0.1 confusion in a new spelling.
    #[test]
    #[should_panic(expected = "use CachePolicy::Off")]
    fn a_zero_capacity_is_refused_by_name() {
        let _ = CachePolicy::ttl(0, TTL);
    }

    #[test]
    #[should_panic(expected = "use CachePolicy::Off")]
    fn a_zero_capacity_is_refused_for_forever_too() {
        let _ = CachePolicy::forever(0);
    }

    /// Asking for a stale window on a cache that stores nothing is not an
    /// error, it is simply nothing.
    #[test]
    fn a_stale_window_on_off_is_still_off() {
        let policy = CachePolicy::Off.stale_while_revalidate(Duration::from_secs(60));
        assert_eq!(policy, CachePolicy::Off);
        let c = cache(policy);
        c.store(&key("/x"), BuiltPage::ok("page"));
        assert!(c.get(&key("/x")).is_none());
    }

    // ── the page's own fields ───────────────────────────────────────────────

    /// `age` is what an `Age:` header would be built from, so it has to track
    /// the entry rather than the lookup.
    #[test]
    fn age_reports_how_old_the_entry_is() {
        let c = cache(CachePolicy::ttl(8, TTL));
        let k = key("/x");
        c.store(&k, BuiltPage::ok("page"));
        assert!(c.get(&k).unwrap().age < Duration::from_secs(1), "just stored");

        if !c.age(&k, Duration::from_secs(120)) {
            return;
        }
        let hit = c.get(&k).unwrap();
        assert!(hit.age >= Duration::from_secs(120), "age was {:?}", hit.age);
        assert!(!hit.stale, "still inside the TTL");
    }

    /// Headers survive the round trip, and a fresh build reports `age` ~0 and
    /// `stale: false` rather than whatever the previous entry had.
    #[test]
    fn headers_are_stored_and_returned() {
        let c = cache(CachePolicy::default());
        let k = key("/x");
        let stored = c.store(
            &k,
            BuiltPage::ok("page")
                .header("cache-control", "public, max-age=60")
                .header("content-language", "pt-BR"),
        );
        assert_eq!(stored.age, Duration::ZERO);
        assert!(!stored.stale);

        let hit = c.get(&k).unwrap();
        assert_eq!(
            hit.headers,
            vec![
                ("cache-control".to_string(), "public, max-age=60".to_string()),
                ("content-language".to_string(), "pt-BR".to_string()),
            ]
        );
    }

    /// The tuple conversion is the short spelling the docs use; it must agree
    /// with the long one.
    #[test]
    fn a_status_and_body_tuple_is_a_built_page() {
        let from_tuple: BuiltPage = (404u16, "gone".to_string()).into();
        assert_eq!(from_tuple.status, 404);
        assert_eq!(from_tuple.body, "gone");
        assert!(from_tuple.headers.is_empty());
    }

    /// A redirect is a body-less page carrying `Location`, and nothing else.
    #[test]
    fn a_redirect_is_a_page_with_a_location_and_no_body() {
        let page = BuiltPage::redirect(301, "/aluguel/blumenau");
        assert_eq!(page.status, 301);
        assert!(page.body.is_empty());
        assert_eq!(
            page.headers,
            vec![("location".to_string(), "/aluguel/blumenau".to_string())]
        );
    }

    // ── keys, the remaining edges ───────────────────────────────────────────

    /// A key with no variants is the URL itself — no separator, no padding, so
    /// the common case costs nothing.
    #[test]
    fn a_key_without_variants_is_just_the_url() {
        assert_eq!(key("/venda/blumenau").cache_key(), "/venda/blumenau");
    }

    /// Two variants with the same name are both kept: a repeated dimension
    /// (two `Accept-Language` entries, say) still describes one page, and
    /// dropping one silently would merge two different documents.
    #[test]
    fn repeated_variant_names_are_both_kept() {
        let one = key("/x").variant("lang", "pt").variant("lang", "en");
        let two = key("/x").variant("lang", "pt");
        assert_ne!(one.cache_key(), two.cache_key());
    }

    /// The digest has to distinguish payloads that differ only in order, or a
    /// reordered row set would silently reuse the previous page.
    #[test]
    fn the_digest_notices_a_reordering() {
        let a = key("/x").variant_digest("seed", &[1u8, 2, 3]);
        let b = key("/x").variant_digest("seed", &[3u8, 2, 1]);
        assert_ne!(a.cache_key(), b.cache_key());
    }

}
