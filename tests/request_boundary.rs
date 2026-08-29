//! The pooled isolate must not hand one request the previous request's state.
//!
//! Own test binary by habit, not necessity: since 0.2 the bundle belongs to the
//! pool, so `two_engines.rs` covers several in one file. Kept separate because
//! each case here wants a whole engine of its own anyway.

#![cfg(all(feature = "v8-pool", feature = "cache"))]

use rusty_ssr::SsrEngine;

/// Writes to `localStorage` and reports what it found there on arrival.
///
/// `localStorage` in the prelude is *real* in-memory storage, not a stub that
/// forgets — which is what makes this a leak rather than a curiosity. Without a
/// per-request reset, render two would report render one's URL, and in
/// production those two renders belong to two different people.
const STORAGE_BUNDLE: &str = r#"
    globalThis.renderPage = function(url, data) {
        const inherited = globalThis.localStorage.getItem("secret") || "nothing";
        globalThis.localStorage.setItem("secret", url);
        return inherited;
    };
"#;

/// `pool_size(1)` on purpose: with more workers the second render might land on
/// a different isolate and pass for the wrong reason. One worker means both
/// renders share an isolate, which is exactly the case the boundary exists for.
#[tokio::test]
async fn a_render_cannot_read_what_the_previous_one_stored() {
    let dir = tempfile::tempdir().unwrap();
    let bundle_path = dir.path().join("storage.js");
    std::fs::write(&bundle_path, STORAGE_BUNDLE).unwrap();

    let engine = SsrEngine::builder()
        .bundle_path(&bundle_path)
        .pool_size(1)
        .build_engine()
        .unwrap();

    let first = engine.render_uncached("/visitor-a", "{}").await.unwrap();
    assert_eq!(first, "nothing", "a cold isolate starts empty");

    let second = engine.render_uncached("/visitor-b", "{}").await.unwrap();
    assert_eq!(
        second, "nothing",
        "render two read render one's storage — that is one visitor reading another's"
    );
}

/// The bundle's own module state is beyond the prelude's reach, so the prelude
/// calls `onSsrRequest` and the bundle clears it there.
const HOOK_BUNDLE: &str = r#"
    globalThis.__renders = 0;
    globalThis.onSsrRequest = function() { globalThis.__renders = 0; };
    globalThis.renderPage = function(url, data) {
        globalThis.__renders += 1;
        return String(globalThis.__renders);
    };
"#;

#[tokio::test]
async fn the_bundle_gets_a_hook_to_clear_its_own_state() {
    let dir = tempfile::tempdir().unwrap();
    let bundle_path = dir.path().join("hook.js");
    std::fs::write(&bundle_path, HOOK_BUNDLE).unwrap();

    let engine = SsrEngine::builder()
        .bundle_path(&bundle_path)
        .pool_size(1)
        .build_engine()
        .unwrap();

    for _ in 0..3 {
        let out = engine.render_uncached("/x", "{}").await.unwrap();
        assert_eq!(out, "1", "onSsrRequest should have reset the counter each time");
    }
}

/// A bundle that brings its own storage is not clobbered by the reset — the
/// prelude only resets what the prelude created.
const OWN_STORAGE_BUNDLE: &str = r#"
    const mine = {};
    globalThis.localStorage = {
        getItem: (k) => (k in mine ? mine[k] : null),
        setItem: (k, v) => { mine[k] = String(v); },
        removeItem: (k) => { delete mine[k]; },
        clear: () => {},
        length: 0,
        key: () => null,
    };
    globalThis.renderPage = function(url) {
        const seen = globalThis.localStorage.getItem("n") || "0";
        globalThis.localStorage.setItem("n", String(Number(seen) + 1));
        return seen;
    };
"#;

#[tokio::test]
async fn a_bundle_that_owns_its_storage_keeps_it() {
    let dir = tempfile::tempdir().unwrap();
    let bundle_path = dir.path().join("own.js");
    std::fs::write(&bundle_path, OWN_STORAGE_BUNDLE).unwrap();

    let engine = SsrEngine::builder()
        .bundle_path(&bundle_path)
        .pool_size(1)
        .build_engine()
        .unwrap();

    // The bundle's own storage is its business: resetting it would be the
    // prelude overruling a deliberate choice, which the non-clobbering rule
    // exists to prevent.
    assert_eq!(engine.render_uncached("/x", "{}").await.unwrap(), "0");
    assert_eq!(engine.render_uncached("/x", "{}").await.unwrap(), "1");
}

/// The reset hook is a safety boundary, so a bundle that breaks it must not get
/// a served page. Swallowing the throw would mean answering a request whose
/// isolation failed — with the previous request's state still in place, and the
/// previous request belonged to somebody else.
const BROKEN_HOOK_BUNDLE: &str = r#"
    globalThis.onSsrRequest = function() { throw new Error("cleanup exploded"); };
    globalThis.renderPage = function(url) { return "<main>" + url + "</main>"; };
"#;

#[tokio::test]
async fn a_throwing_reset_hook_fails_the_render() {
    let dir = tempfile::tempdir().unwrap();
    let bundle_path = dir.path().join("broken-hook.js");
    std::fs::write(&bundle_path, BROKEN_HOOK_BUNDLE).unwrap();

    let engine = SsrEngine::builder()
        .bundle_path(&bundle_path)
        .pool_size(1)
        .build_engine()
        .unwrap();

    let err = engine
        .render_uncached("/x", "{}")
        .await
        .expect_err("a failed reset must not produce a page");
    let msg = err.to_string();
    assert!(msg.contains("reset"), "unhelpful error: {msg}");
    assert!(msg.contains("cleanup exploded"), "the cause is lost: {msg}");

    // And it stays an error rather than poisoning the worker for good.
    assert!(engine.render_uncached("/y", "{}").await.is_err());
}

/// With the prelude off there is no `__rustySsrReset` at all. That is a
/// legitimate configuration — the bundle brings its own globals — and it must
/// render, not fail on a missing hook.
#[tokio::test]
async fn a_bundle_without_the_prelude_still_renders() {
    let dir = tempfile::tempdir().unwrap();
    let bundle_path = dir.path().join("bare.js");
    std::fs::write(
        &bundle_path,
        r#"globalThis.renderPage = function(url) { return "bare:" + url; };"#,
    )
    .unwrap();

    let engine = SsrEngine::builder()
        .bundle_path(&bundle_path)
        .pool_size(1)
        .polyfills(false)
        .build_engine()
        .unwrap();

    // Twice: the "there is no hook" answer has to be cached as an answer, or
    // every render would pay to re-discover it.
    assert_eq!(engine.render_uncached("/x", "{}").await.unwrap(), "bare:/x");
    assert_eq!(engine.render_uncached("/y", "{}").await.unwrap(), "bare:/y");
}

/// `location` is set from the render URL before every render.
///
/// Every consumer used to write this by hand, because the engine is the only
/// thing that knows the URL and a router reading `location.pathname` is how
/// most applications decide what to render. Getting it wrong is silent: the
/// router sees "/" and every URL renders the home page, which reads as a bug in
/// the app rather than a missing line in the harness.
const LOCATION_BUNDLE: &str = r#"
    globalThis.renderPage = function(url) {
        const l = globalThis.location;
        return [l.pathname, l.search, l.hash, l.href].join("|");
    };
"#;

#[tokio::test]
async fn location_comes_from_the_render_url() {
    let dir = tempfile::tempdir().unwrap();
    let bundle_path = dir.path().join("location.js");
    std::fs::write(&bundle_path, LOCATION_BUNDLE).unwrap();

    let engine = SsrEngine::builder()
        .bundle_path(&bundle_path)
        .pool_size(1)
        .build_engine()
        .unwrap();

    assert_eq!(
        engine.render_uncached("/venda/blumenau?quartos=2#mapa", "{}").await.unwrap(),
        "/venda/blumenau|?quartos=2|#mapa|http://localhost/venda/blumenau?quartos=2#mapa"
    );

    // And it is per request, not sticky: the next render must not inherit the
    // previous URL's query.
    assert_eq!(
        engine.render_uncached("/sobre", "{}").await.unwrap(),
        "/sobre|||http://localhost/sobre"
    );
}

/// An absolute URL brings its own origin with it.
#[tokio::test]
async fn an_absolute_url_sets_the_origin_too() {
    let dir = tempfile::tempdir().unwrap();
    let bundle_path = dir.path().join("origin.js");
    std::fs::write(
        &bundle_path,
        r#"globalThis.renderPage = () => {
               const l = globalThis.location;
               return [l.origin, l.protocol, l.host, l.hostname, l.port, l.pathname].join("|");
           };"#,
    )
    .unwrap();

    let engine = SsrEngine::builder()
        .bundle_path(&bundle_path)
        .pool_size(1)
        .build_engine()
        .unwrap();

    assert_eq!(
        engine.render_uncached("https://morada.test:8443/venda", "{}").await.unwrap(),
        "https://morada.test:8443|https:|morada.test:8443|morada.test|8443|/venda"
    );
}

/// `seal_globals` removes what a render hung on `globalThis`, without the
/// bundle having to know anything about it. This is the half `onSsrRequest`
/// cannot cover: the code that leaks is usually a dependency that never
/// defines a hook.
const LEAKY_BUNDLE: &str = r#"
    globalThis.renderPage = function(url) {
        const inherited = globalThis.__leftBehind || "nothing";
        globalThis.__leftBehind = url;
        return inherited;
    };
"#;

#[tokio::test]
async fn sealed_globals_do_not_survive_a_render() {
    let dir = tempfile::tempdir().unwrap();
    let bundle_path = dir.path().join("leaky.js");
    std::fs::write(&bundle_path, LEAKY_BUNDLE).unwrap();

    let engine = SsrEngine::builder()
        .bundle_path(&bundle_path)
        .pool_size(1)
        .seal_globals(true)
        .build_engine()
        .unwrap();

    assert_eq!(engine.render_uncached("/visitor-a", "{}").await.unwrap(), "nothing");
    assert_eq!(
        engine.render_uncached("/visitor-b", "{}").await.unwrap(),
        "nothing",
        "visitor B read what visitor A left on globalThis"
    );
}

/// Off by default, and the default has to keep working — a bundle that caches
/// on `globalThis` on purpose is doing something legitimate, and turning this
/// on for everyone would break it silently.
#[tokio::test]
async fn without_sealing_a_global_survives_as_before() {
    let dir = tempfile::tempdir().unwrap();
    let bundle_path = dir.path().join("unsealed.js");
    std::fs::write(&bundle_path, LEAKY_BUNDLE).unwrap();

    let engine = SsrEngine::builder()
        .bundle_path(&bundle_path)
        .pool_size(1)
        .build_engine()
        .unwrap();

    assert_eq!(engine.render_uncached("/a", "{}").await.unwrap(), "nothing");
    assert_eq!(engine.render_uncached("/b", "{}").await.unwrap(), "/a");
}

/// Sealing must not delete what the BUNDLE defined — it runs after the bundle's
/// top-level code, so the bundle's own globals are part of the baseline.
#[tokio::test]
async fn sealing_keeps_the_bundles_own_globals() {
    let dir = tempfile::tempdir().unwrap();
    let bundle_path = dir.path().join("bundle-globals.js");
    std::fs::write(
        &bundle_path,
        r#"globalThis.APP_CONFIG = { name: "morada" };
           globalThis.renderPage = () => globalThis.APP_CONFIG.name;"#,
    )
    .unwrap();

    let engine = SsrEngine::builder()
        .bundle_path(&bundle_path)
        .pool_size(1)
        .seal_globals(true)
        .build_engine()
        .unwrap();

    for _ in 0..3 {
        assert_eq!(engine.render_uncached("/", "{}").await.unwrap(), "morada");
    }
}
