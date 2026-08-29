//! A render that produces nothing is a failure, when the caller has said so.
//!
//! This is the SSR failure that does not announce itself. Every real bundle
//! wraps its render in a `try/catch` — frameworks throw for ordinary reasons —
//! and the catch returns `""`. Nothing errors, nothing logs, and the caller
//! drops an empty string into its HTML shell and serves a blank page with a
//! 200. The only way anyone finds out is a person looking at it.

#![cfg(all(feature = "v8-pool", feature = "cache"))]

mod common;

use rusty_ssr::SsrError;

/// The shape production actually has: the bundle catches its own throw and
/// returns the empty string, so from Rust the render "succeeded".
const SWALLOWING_BUNDLE: &str = r#"
    globalThis.renderPage = function(url) {
        try {
            if (url === "/broken") throw new Error("a component threw");
            return "<main>a real page with enough bytes to look like one</main>";
        } catch (e) {
            return "";
        }
    };
"#;

fn engine(source: &str, min_bytes: Option<usize>) -> common::Fixture {
    common::engine_with(source, |b| match min_bytes {
        Some(min) => b.min_render_bytes(min),
        None => b,
    })
}

#[tokio::test]
async fn a_blank_render_is_an_error_when_a_floor_is_set() {
    let engine = engine(SWALLOWING_BUNDLE, Some(20));

    let good = engine.render_uncached("/", "{}").await.unwrap();
    assert!(good.contains("a real page"));

    match engine.render_uncached("/broken", "{}").await {
        Err(SsrError::EmptyRender(bytes)) => assert_eq!(bytes, 0),
        other => panic!("expected EmptyRender, got {other:?}"),
    }
}

/// Off by default, because a page whose content is legitimately nothing is a
/// real thing and the engine cannot tell the two apart on its own.
#[tokio::test]
async fn without_a_floor_the_blank_render_is_returned_as_is() {
    let engine = engine(SWALLOWING_BUNDLE, None);

    assert_eq!(engine.render_uncached("/broken", "{}").await.unwrap(), "");
}

/// Whitespace is not content. A bundle that returns "\n  \n" has produced a
/// blank page just as surely as one that returned "".
#[tokio::test]
async fn whitespace_does_not_count_as_a_page() {
    let engine = engine(
        r#"globalThis.renderPage = () => "\n   \t  \n";"#,
        Some(10),
    );

    match engine.render_uncached("/", "{}").await {
        Err(SsrError::EmptyRender(bytes)) => assert_eq!(bytes, 0, "trimmed length"),
        other => panic!("expected EmptyRender, got {other:?}"),
    }
}

/// A thin-but-not-empty render trips it too, and the error says how thin — the
/// number is what tells you whether the floor is set right.
#[tokio::test]
async fn a_thin_render_reports_what_it_produced() {
    let engine = engine(r#"globalThis.renderPage = () => "<div></div>";"#, Some(200));

    match engine.render_uncached("/", "{}").await {
        Err(SsrError::EmptyRender(bytes)) => assert_eq!(bytes, "<div></div>".len()),
        other => panic!("expected EmptyRender, got {other:?}"),
    }
}

/// The guard covers the binary channels as well — a blank page is a blank page
/// whichever way the data went in.
#[tokio::test]
async fn the_guard_covers_every_render_path() {
    let engine = engine(r#"globalThis.renderPage = () => "";"#, Some(10));

    assert!(matches!(
        engine.render_uncached("/", "{}").await,
        Err(SsrError::EmptyRender(0))
    ));
    assert!(matches!(
        engine.render_with_bytes("/", vec![1, 2, 3]).await,
        Err(SsrError::EmptyRender(0))
    ));
    assert!(matches!(
        engine.render_with_json_and_bytes("/", "{}", vec![1]).await,
        Err(SsrError::EmptyRender(0))
    ));
    assert!(matches!(engine.render("/").await, Err(SsrError::EmptyRender(0))));
}

/// A refused render must not be cached — the fragment cache would otherwise
/// hold the blank page the guard exists to reject.
#[tokio::test]
async fn a_refused_render_is_not_cached() {
    let engine = engine(r#"globalThis.renderPage = () => "";"#, Some(10));

    assert!(engine.render("/x").await.is_err());
    assert!(engine.render("/x").await.is_err(), "a cached blank would come back Ok");
    assert_eq!(engine.cache_metrics().insertions, 0);
}
