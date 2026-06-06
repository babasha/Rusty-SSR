//! Item 2 / watchdog: a hanging render times out the caller AND the watchdog
//! terminates the runaway so the worker is reclaimed (not wedged forever).
//!
//! Own test binary because the SSR bundle is process-global.

#![cfg(all(feature = "v8-pool", feature = "cache"))]

use rusty_ssr::SsrEngine;
use std::time::{Duration, Instant};

// Only "/hang" loops forever (never allocates, so the heap cap can't catch it);
// other URLs render normally. With one worker, a successful render *after* the
// hang proves the watchdog freed the worker.
const BUNDLE: &str = r#"
    globalThis.renderPage = function(url, data) {
        if (url === "/hang") { while (true) {} }
        return "<html>ok " + url + "</html>";
    };
"#;

#[tokio::test]
async fn hanging_render_times_out_and_worker_is_reclaimed() {
    let dir = tempfile::tempdir().unwrap();
    let bundle_path = dir.path().join("hang.js");
    std::fs::write(&bundle_path, BUNDLE).unwrap();

    let engine = SsrEngine::builder()
        .bundle_path(&bundle_path)
        .pool_size(1) // single worker: recovery is only possible via the watchdog
        .request_timeout(Some(Duration::from_millis(300)))
        .build_engine()
        .unwrap();

    // 1) The hang must surface as an error near request_timeout, not hang.
    let start = Instant::now();
    let r1 = tokio::time::timeout(Duration::from_secs(10), engine.render("/hang"))
        .await
        .expect("render must return via request_timeout, not hang the caller");
    assert!(r1.is_err(), "a hanging render must surface as an error");
    assert!(
        start.elapsed() < Duration::from_secs(5),
        "should time out near request_timeout, took {:?}",
        start.elapsed()
    );

    // 2) The watchdog must have terminated the runaway and freed the (only)
    //    worker, so a normal render now succeeds. Without active termination
    //    this second render would also time out.
    let r2 = tokio::time::timeout(Duration::from_secs(10), engine.render("/ok"))
        .await
        .expect("second render must not hang");
    let html = r2.expect("worker should be reclaimed after the watchdog terminates the runaway");
    assert!(html.contains("ok /ok"), "unexpected render output: {html}");
}
