//! Item 4: V8 heap-limit enforcement.
//!
//! A runaway render must be terminated (returning an `Err`) instead of
//! aborting the whole process. Lives in its own test binary because the SSR
//! bundle is process-global (`OnceLock`), so it needs a dedicated process to
//! load the runaway bundle.

#![cfg(all(feature = "v8-pool", feature = "cache"))]

mod common;

use std::time::Duration;

const RUNAWAY_BUNDLE: &str = r#"
    globalThis.renderPage = function(url, data) {
        // Grow the heap without bound — must hit the cap and be terminated.
        let s = "x";
        while (true) { s = s + s; }
        return s;
    };
"#;

#[tokio::test]
async fn heap_cap_terminates_runaway_render_without_aborting() {
    let engine = common::engine_with(RUNAWAY_BUNDLE, |b| b.cache_size(8).max_heap_mb(64));

    // Wrap in a timeout so a misbehaving cap fails the test instead of hanging
    // the suite forever.
    let result = tokio::time::timeout(Duration::from_secs(30), engine.render("/boom")).await;
    assert!(result.is_ok(), "render should terminate via the heap cap, not hang");
    assert!(
        result.unwrap().is_err(),
        "runaway render must return Err (terminated), not OOM-abort the process"
    );

    // Process survived and the isolate recovered (deno_core cancels the
    // termination when converting it to an error) — a second runaway also
    // fails gracefully rather than wedging the worker.
    let second = tokio::time::timeout(Duration::from_secs(30), engine.render("/boom2")).await;
    assert!(second.is_ok(), "engine should stay responsive after a terminated render");
    assert!(second.unwrap().is_err(), "second runaway render should also be terminated");
}
