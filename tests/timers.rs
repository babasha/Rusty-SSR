//! Item 6: setTimeout is deferred to a microtask, not run synchronously.
//!
//! Own test binary so this case gets an engine to itself.

#![cfg(all(feature = "v8-pool", feature = "cache"))]

use rusty_ssr::SsrEngine;
use std::time::Duration;

// Proves two things at once:
//  - the setTimeout callback does NOT run synchronously inline (so `immediate`
//    still reads "before"), and
//  - `await new Promise(r => setTimeout(r))` still resolves (the render
//    completes instead of hanging), because deferred callbacks run as
//    microtasks drained while the render promise resolves.
const TIMER_BUNDLE: &str = r#"
    globalThis.renderPage = async function(url, data) {
        let ran = "before";
        setTimeout(() => { ran = "sync"; }, 0);
        const immediate = ran;
        await new Promise((resolve) => setTimeout(resolve, 0));
        return immediate;
    };
"#;

#[tokio::test]
async fn settimeout_callback_is_deferred_not_synchronous() {
    let dir = tempfile::tempdir().unwrap();
    let bundle_path = dir.path().join("timer.js");
    std::fs::write(&bundle_path, TIMER_BUNDLE).unwrap();

    let engine = SsrEngine::builder()
        .bundle_path(&bundle_path)
        .pool_size(1)
        .build_engine()
        .unwrap();

    let html = tokio::time::timeout(Duration::from_secs(10), engine.render("/timer"))
        .await
        .expect("render should not hang (await-yield via setTimeout must resolve)")
        .expect("render should succeed");

    assert_eq!(
        &*html, "before",
        "setTimeout callback must be deferred to a microtask, not run synchronously inline"
    );
}
