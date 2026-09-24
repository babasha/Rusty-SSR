//! A render reports the code-split modules it used, and only its own.
//!
//! The list becomes `<link rel="modulepreload">` tags in a cached document, so
//! the two failures that matter are both silent: a module the render asked for
//! that never reaches the list (the page waits a round trip it did not have
//! to), and one render's modules reaching another's page (every visitor of a
//! cheap page downloads an expensive one's chunks).

#![cfg(all(feature = "v8-pool", feature = "cache"))]

mod common;

use rusty_ssr::assets::ViteManifest;

/// What a build plugin's output looks like: every `import()` preceded by a
/// report of the module it names. The screen depends on the URL.
const BUNDLE: &str = r#"
    const load = (id) => (globalThis.__rustySsrModule(id), Promise.resolve(id));
    globalThis.renderPage = async function(url) {
        if (url.startsWith("/detail")) {
            await load("src/Detail.tsx");
            await load("src/i18n/public.ts");
            await load("src/Detail.tsx"); // asked twice, listed once
        }
        if (url.startsWith("/map")) await load("src/Map.tsx");
        return "<main>" + url + "</main>";
    };
"#;

#[tokio::test]
async fn a_render_reports_the_modules_it_loaded_in_order_and_once() {
    let engine = common::engine(BUNDLE);
    let page = engine.render_uncached_collect("/detail/1", "{}").await.unwrap();
    assert_eq!(page.html, "<main>/detail/1</main>");
    assert_eq!(page.modules, vec!["src/Detail.tsx", "src/i18n/public.ts"]);
}

/// One worker, so both renders share an isolate — the case the reset exists for.
#[tokio::test]
async fn one_renders_modules_never_reach_the_next() {
    let engine = common::engine(BUNDLE);
    engine.render_uncached_collect("/detail/1", "{}").await.unwrap();
    let map = engine.render_uncached_collect("/map", "{}").await.unwrap();
    assert_eq!(map.modules, vec!["src/Map.tsx"]);
    let home = engine.render_uncached_collect("/", "{}").await.unwrap();
    assert!(home.modules.is_empty(), "{:?}", home.modules);
}

/// A render that throws after reporting leaves its list behind in the isolate;
/// the next request must not inherit it.
#[tokio::test]
async fn a_failed_render_leaves_nothing_for_the_next_one() {
    let engine = common::engine_with(
        r#"
        globalThis.renderPage = function(url) {
            if (url === "/boom") { globalThis.__rustySsrModule("src/Heavy.tsx"); throw new Error("boom"); }
            return "<main>ok</main>";
        };
        "#,
        |b| b,
    );
    assert!(engine.render_uncached_collect("/boom", "{}").await.is_err());
    let next = engine.render_uncached_collect("/fine", "{}").await.unwrap();
    assert!(next.modules.is_empty(), "{:?}", next.modules);
}

#[tokio::test]
async fn a_bundle_that_reports_nothing_gets_an_empty_list() {
    let engine = common::engine(r#"globalThis.renderPage = () => "<main>plain</main>";"#);
    let page = engine.render_uncached_collect("/", "{}").await.unwrap();
    assert_eq!(page.html, "<main>plain</main>");
    assert!(page.modules.is_empty());
}

/// Without the prelude there is no collector; the render still works.
#[tokio::test]
async fn without_the_prelude_the_list_is_empty_and_the_render_unchanged() {
    let engine = common::engine_with(
        r#"globalThis.renderPage = () => "<main>bare</main>";"#,
        |b| b.polyfills(false),
    );
    let page = engine.render_uncached_collect("/", "{}").await.unwrap();
    assert_eq!(page.html, "<main>bare</main>");
    assert!(page.modules.is_empty());
}

/// The empty-render floor applies here exactly as to `render_uncached`.
#[tokio::test]
async fn the_empty_render_guard_still_holds() {
    let engine = common::engine_with(r#"globalThis.renderPage = () => "";"#, |b| b.min_render_bytes(10));
    assert!(engine.render_uncached_collect("/", "{}").await.is_err());
}

/// End to end: the modules of a render, through a manifest, into tags.
#[tokio::test]
async fn the_list_turns_into_preload_tags() {
    let engine = common::engine(BUNDLE);
    let manifest = ViteManifest::parse(
        r#"{
            "index.html": { "file": "assets/index.js", "isEntry": true },
            "src/Detail.tsx": { "file": "assets/Detail-1.js", "imports": ["index.html"] },
            "src/i18n/public.ts": { "file": "assets/public-2.js" }
        }"#,
    )
    .unwrap();
    let page = engine.render_uncached_collect("/detail/1", "{}").await.unwrap();
    assert_eq!(
        manifest.preload_links(&page.modules),
        "<link rel=\"modulepreload\" crossorigin href=\"/assets/Detail-1.js\">\
         <link rel=\"modulepreload\" crossorigin href=\"/assets/public-2.js\">"
    );
}
