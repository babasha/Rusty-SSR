//! Two engines in one process are two applications.
//!
//! This is the whole point of taking the bundle out of a process-global
//! `OnceLock`. Under 0.1 the second `SsrEngine` built in a process silently
//! rendered the *first* one's bundle: `init_bundle_with` saw the global was
//! already set and returned `Ok(())` without loading anything. Nothing errored,
//! nothing warned — the second application simply served the first one's pages.
//!
//! It is also why this file can exist at all. Every other integration test in
//! this crate needs its own binary because one bundle per process was the rule.

#![cfg(all(feature = "v8-pool", feature = "cache"))]

use rusty_ssr::cache::{BuiltPage, CachePolicy, RenderKey};
use rusty_ssr::SsrEngine;

fn engine_with(source: &str) -> (tempfile::TempDir, SsrEngine) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("bundle.js");
    std::fs::write(&path, source).unwrap();
    let engine = SsrEngine::builder()
        .bundle_path(&path)
        .pool_size(1)
        .build_engine()
        .unwrap();
    (dir, engine)
}

#[tokio::test]
async fn each_engine_renders_its_own_bundle() {
    let (_a_dir, alpha) = engine_with(
        r#"globalThis.renderPage = (url) => "ALPHA:" + url;"#,
    );
    let (_b_dir, beta) = engine_with(
        r#"globalThis.renderPage = (url) => "BETA:" + url;"#,
    );

    assert_eq!(alpha.render_uncached("/x", "{}").await.unwrap(), "ALPHA:/x");
    assert_eq!(beta.render_uncached("/x", "{}").await.unwrap(), "BETA:/x");
    // …and again, in case the first render is what fixes the runtime.
    assert_eq!(alpha.render_uncached("/y", "{}").await.unwrap(), "ALPHA:/y");
    assert_eq!(beta.render_uncached("/y", "{}").await.unwrap(), "BETA:/y");
}

/// The page caches are per engine too, so one application's pages can never be
/// answered from another's.
#[tokio::test]
async fn each_engine_has_its_own_page_cache() {
    let (_a_dir, alpha) = engine_with(r#"globalThis.renderPage = () => "alpha";"#);
    let (_b_dir, beta) = engine_with(r#"globalThis.renderPage = () => "beta";"#);

    let key = RenderKey::new("/shared").variant("host", "example.test");

    let a = alpha
        .page(&key, || async { Ok(BuiltPage::ok("from alpha")) })
        .await
        .unwrap();
    assert_eq!(String::from_utf8(a.body.to_vec()).unwrap(), "from alpha");

    // Same key, other engine: a miss, not alpha's page.
    let b = beta
        .page(&key, || async { Ok(BuiltPage::ok("from beta")) })
        .await
        .unwrap();
    assert_eq!(String::from_utf8(b.body.to_vec()).unwrap(), "from beta");

    assert_eq!(alpha.page_cache().len(), 1);
    assert_eq!(beta.page_cache().len(), 1);
}

/// Different render-function names, different policies — the configurations do
/// not bleed into each other either.
#[tokio::test]
async fn engines_keep_their_own_configuration() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("named.js");
    std::fs::write(&path, r#"globalThis.customEntry = (url) => "custom:" + url;"#).unwrap();
    let named = SsrEngine::builder()
        .bundle_path(&path)
        .pool_size(1)
        .render_function("customEntry")
        .page_cache(CachePolicy::Off)
        .build_engine()
        .unwrap();

    let (_d, standard) = engine_with(r#"globalThis.renderPage = (url) => "standard:" + url;"#);

    assert_eq!(named.render_uncached("/a", "{}").await.unwrap(), "custom:/a");
    assert_eq!(
        standard.render_uncached("/a", "{}").await.unwrap(),
        "standard:/a"
    );

    let key = RenderKey::new("/a");
    named
        .page(&key, || async { Ok(BuiltPage::ok("x")) })
        .await
        .unwrap();
    standard
        .page(&key, || async { Ok(BuiltPage::ok("x")) })
        .await
        .unwrap();
    assert!(named.page_cache().is_empty(), "this one was configured Off");
    assert_eq!(standard.page_cache().len(), 1, "this one was not");
}
