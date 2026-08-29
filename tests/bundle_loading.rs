//! Building an engine means the bundle loads, or it means nothing.
//!
//! These exist because for a while it meant nothing. A bundle with a syntax
//! error produced an `SsrEngine` that built successfully, a pool whose workers
//! had all died on startup, and requests that hung for the whole
//! `request_timeout` — thirty seconds by default — before failing with "Render
//! timeout". The actual message went to `tracing::error!`, which is invisible
//! without a subscriber installed, so the one fact that explained everything was
//! the one fact nobody could see.
//!
//! Deploying a bad bundle is not an exotic scenario. It is what a broken build
//! looks like, and the engine has to say so at the point where it is still
//! obvious what happened.

#![cfg(feature = "v8-pool")]

mod common;

use std::time::{Duration, Instant};

use rusty_ssr::SsrError;

/// The floor case: a bundle that does not parse.
#[tokio::test]
async fn a_syntax_error_fails_the_build() {
    let err = common::try_engine_with("globalThis.renderPage = (url) => { not valid js\n", |b| b)
        .err()
        .expect("an unparseable bundle must not produce a working engine");

    assert!(
        matches!(err, SsrError::V8Init(_)),
        "the failure must name V8 initialisation, got {err:?}"
    );
    let msg = err.to_string();
    assert!(
        msg.contains("SyntaxError"),
        "the message must carry what V8 actually said, got: {msg}"
    );
}

/// A bundle that parses and then throws while running its top level. Common in
/// practice: a missing global, an import that resolved to nothing, an
/// initialisation that assumed a browser.
#[tokio::test]
async fn a_throw_at_load_fails_the_build() {
    let err = common::try_engine_with(
        "globalThis.renderPage = (u) => u;\nthrow new Error('missing config');\n",
        |b| b,
    )
    .err()
    .expect("a bundle that throws while loading must not produce a working engine");

    let msg = err.to_string();
    assert!(
        msg.contains("missing config"),
        "the bundle's own message must survive to the caller, got: {msg}"
    );
}

/// The point of the whole thing: the failure has to arrive at build time, not
/// as a timeout per request afterwards. A thirty-second wait per request that
/// blames the render is the behaviour this replaced.
#[tokio::test]
async fn the_failure_is_immediate_rather_than_a_timeout_per_request() {
    let started = Instant::now();
    let result = common::try_engine_with("globalThis.renderPage = ((( \n", |b| {
        b.request_timeout(Some(Duration::from_secs(30)))
    });
    let elapsed = started.elapsed();

    assert!(result.is_err());
    assert!(
        elapsed < Duration::from_secs(5),
        "a bundle that cannot load must fail the build at once, took {elapsed:?}"
    );
}

/// Every worker loads the same bundle, so the failure is reported once however
/// many of them there are — and reporting it must not depend on which worker
/// got there first, nor hang waiting for the others.
#[tokio::test]
async fn a_large_pool_still_reports_the_failure_once_and_quickly() {
    let started = Instant::now();
    let result = common::try_engine_with("throw new Error('nope');", |b| b.pool_size(16));
    let elapsed = started.elapsed();

    let err = result.err().expect("must fail");
    assert!(err.to_string().contains("nope"), "got: {err}");
    assert!(
        elapsed < Duration::from_secs(10),
        "sixteen workers must not turn one failure into a long wait, took {elapsed:?}"
    );
}

// ── the other half: a good bundle really is ready when the build returns ─────

/// Building the engine now waits for every worker to have the bundle loaded, so
/// the first request does not pay for isolate creation and compilation. That is
/// worth pinning: it is the reason the build got slower, and undoing it would
/// look like a speed-up.
#[tokio::test]
async fn a_built_engine_is_ready_to_render_immediately() {
    let engine = common::engine_with(
        "globalThis.renderPage = (url) => '<main>' + url + '</main>';",
        |b| b.pool_size(4),
    );

    // No warm-up: the very first render, on a cold caller, must be fast because
    // the isolates are already up.
    let started = Instant::now();
    let html = engine.render_uncached("/first", "{}").await.unwrap();
    let first = started.elapsed();

    assert_eq!(html, "<main>/first</main>");
    assert!(
        first < Duration::from_millis(250),
        "the first render should not be paying for V8 start-up, took {first:?}"
    );
    assert_eq!(engine.worker_count(), 4, "all workers must be alive");
}

/// A bundle that loads but has no render function is a different failure, and
/// it is *not* a load failure: the bundle is fine, the contract is not. It
/// surfaces per render, which is right — it is the render that cannot proceed.
#[tokio::test]
async fn a_missing_render_function_is_a_render_error_not_a_load_error() {
    let engine = common::engine("globalThis.somethingElse = 1;");

    let err = engine
        .render_uncached("/x", "{}")
        .await
        .expect_err("there is no render function to call");
    assert!(
        err.to_string().contains("is not a function"),
        "got: {err}"
    );
}
