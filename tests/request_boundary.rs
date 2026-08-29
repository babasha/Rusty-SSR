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
