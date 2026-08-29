//! The page cache through the engine, with a real V8 render behind it.
//!
//! The unit tests in `cache::page` prove the cache's own behaviour with a fake
//! build step. This proves the shape an application actually uses: a build
//! closure that renders, assembles a document around the fragment, and returns
//! it with a status — and a hit that skips all of it.
//!
//! Own test binary because each engine loads its own bundle.

#![cfg(all(feature = "v8-pool", feature = "cache"))]

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use rusty_ssr::cache::{BuiltPage, CachePolicy, RenderKey};
use rusty_ssr::SsrEngine;

const BUNDLE: &str = r#"
    globalThis.renderPage = function(url, data) {
        return "<main>" + url + "</main>";
    };
"#;

fn engine(policy: CachePolicy) -> Arc<SsrEngine> {
    let dir = tempfile::tempdir().unwrap();
    let bundle_path = dir.path().join("bundle.js");
    std::fs::write(&bundle_path, BUNDLE).unwrap();
    let engine = SsrEngine::builder()
        .bundle_path(&bundle_path)
        .pool_size(2)
        .page_cache(policy)
        .build_engine()
        .unwrap();
    // The bundle is read at build time, so the temp dir can go.
    drop(dir);
    Arc::new(engine)
}

fn body(page: &rusty_ssr::cache::CachedPage) -> String {
    String::from_utf8(page.body.to_vec()).unwrap()
}

/// The shape from the module docs: the build assembles the *whole* response,
/// and the second request pays for none of it.
#[tokio::test]
async fn a_hit_skips_the_render_and_everything_around_it() {
    let engine = engine(CachePolicy::default());
    let key = RenderKey::new("/venda/blumenau").variant("host", "morada.test");
    let builds = Arc::new(AtomicUsize::new(0));

    for _ in 0..3 {
        let engine2 = Arc::clone(&engine);
        let counter = Arc::clone(&builds);
        let page = engine
            .page(&key, move || async move {
                counter.fetch_add(1, Ordering::SeqCst);
                // Stands in for the per-request work a real caller does around
                // the render: a SEO query, a data query, injections.
                let fragment = engine2.render_uncached("/venda/blumenau", "{}").await?;
                Ok(BuiltPage::ok(format!("<!doctype html><body>{fragment}</body>")))
            })
            .await
            .unwrap();
        assert_eq!(body(&page), "<!doctype html><body><main>/venda/blumenau</main></body>");
        assert_eq!(page.status, 200);
    }

    assert_eq!(builds.load(Ordering::SeqCst), 1, "three requests, one build");
}

/// The host is not part of the URL, so only the key can carry it — and if it
/// doesn't, one hostname's canonical URLs get served under another's.
#[tokio::test]
async fn two_hosts_do_not_share_a_page() {
    let engine = engine(CachePolicy::default());

    for host in ["morada.test", "outra.test"] {
        let key = RenderKey::new("/venda/blumenau").variant("host", host);
        let owner = host.to_string();
        let page = engine
            .page(&key, move || async move {
                Ok(BuiltPage::ok(format!("<link rel=canonical href=https://{owner}/>")))
            })
            .await
            .unwrap();
        assert!(body(&page).contains(host));
    }

    // And each is still its own after both are stored.
    for host in ["morada.test", "outra.test"] {
        let key = RenderKey::new("/venda/blumenau").variant("host", host);
        let page = engine
            .page(&key, || async { panic!("must not rebuild — both are cached") })
            .await
            .unwrap();
        assert!(body(&page).contains(host), "{host} got another host's document");
    }
}

/// A redirect is a response like any other, so it goes through the cache like
/// any other. Without headers on `BuiltPage` the caller would have to decide
/// *before* the lookup whether a URL redirects — and deciding that is the query
/// a hit exists to skip.
#[tokio::test]
async fn a_redirect_is_cacheable() {
    let engine = engine(CachePolicy::default());
    let key = RenderKey::new("/Aluguel/Blumenau").variant("host", "morada.test");
    let builds = Arc::new(AtomicUsize::new(0));

    for _ in 0..2 {
        let counter = Arc::clone(&builds);
        let page = engine
            .page(&key, move || async move {
                counter.fetch_add(1, Ordering::SeqCst);
                Ok(BuiltPage::redirect(301, "/aluguel/blumenau"))
            })
            .await
            .unwrap();
        assert_eq!(page.status, 301);
        assert!(page.body.is_empty());
        assert_eq!(
            page.headers,
            vec![("location".to_string(), "/aluguel/blumenau".to_string())]
        );
    }
    assert_eq!(builds.load(Ordering::SeqCst), 1);
}

/// A failed render must not be cached — a five-minute-old error would outlive
/// whatever caused it by a long way.
#[tokio::test]
async fn a_failed_build_leaves_nothing_behind() {
    let engine = engine(CachePolicy::default());
    let key = RenderKey::new("/broken").variant("host", "morada.test");

    let failed = engine
        .page(&key, || async {
            Err(rusty_ssr::SsrError::JsExecution("render blew up".into()))
        })
        .await;
    assert!(failed.is_err());

    // The next request gets a real attempt, not the stored failure.
    let page = engine
        .page(&key, || async { Ok(BuiltPage::ok("recovered")) })
        .await
        .unwrap();
    assert_eq!(body(&page), "recovered");
}

/// `Off` really is off, even through the engine.
#[tokio::test]
async fn the_cache_can_be_turned_off() {
    let engine = engine(CachePolicy::Off);
    let key = RenderKey::new("/x").variant("host", "h");
    let builds = Arc::new(AtomicUsize::new(0));

    for _ in 0..3 {
        let counter = Arc::clone(&builds);
        engine
            .page(&key, move || async move {
                counter.fetch_add(1, Ordering::SeqCst);
                tokio::time::sleep(Duration::from_millis(1)).await;
                Ok(BuiltPage::ok("fresh every time"))
            })
            .await
            .unwrap();
    }
    assert_eq!(builds.load(Ordering::SeqCst), 3);
    assert!(engine.page_cache().is_empty());
}
