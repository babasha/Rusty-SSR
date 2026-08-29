//! What the pool reports about itself.
//!
//! A pool serves at most `workers / render_time` requests per second, and
//! throughput stops rising the moment every worker is busy — everything after
//! that is queue delay. So "how close am I to capacity" is *the* operational
//! question about this component, and until these metrics existed it had no
//! answer: `worker_count()` returned a constant and nothing else was exposed.
//!
//! These tests pin the numbers a dashboard would be built on. The point of each
//! is that it moves when the pool's state moves, because a gauge that is always
//! zero is worse than no gauge.

#![cfg(feature = "v8-pool")]

mod common;

use std::sync::Arc;
use std::time::Duration;

/// Busy-waits `ms` inside the render, so a test can hold workers deliberately.
fn slow_bundle(ms: u32) -> String {
    format!(
        "globalThis.renderPage = (url) => {{ \
           const end = Date.now() + {ms}; while (Date.now() < end) {{}} \
           return '<main>' + url + '</main>'; }};"
    )
}

const FAST: &str = "globalThis.renderPage = (url) => '<main>' + url + '</main>';";

// ── the gauges ───────────────────────────────────────────────────────────────

#[tokio::test]
async fn an_idle_pool_reports_itself_idle() {
    let engine = common::engine_with(FAST, |b| b.pool_size(4).queue_capacity(64));

    let m = engine.pool_metrics();
    assert_eq!(m.workers, 4, "all four workers loaded the bundle");
    assert_eq!(m.busy, 0);
    assert_eq!(m.queued, 0);
    assert_eq!(m.renders, 0, "nothing has been rendered yet");
    assert_eq!(m.saturation, 0.0);
    assert_eq!(m.queue_capacity, 64);
}

#[tokio::test]
async fn renders_are_counted_and_the_pool_returns_to_idle() {
    let engine = common::engine_with(FAST, |b| b.pool_size(2));

    for i in 0..25 {
        engine
            .render_uncached(&format!("/p/{i}"), "{}")
            .await
            .unwrap();
    }

    let m = engine.pool_metrics();
    assert_eq!(m.renders, 25, "every render is counted");
    assert_eq!(m.failed, 0);
    assert_eq!(m.timeouts, 0);
    assert_eq!(m.busy, 0, "nothing is in flight once the calls have returned");
    assert_eq!(m.queued, 0);
}

/// Saturation is the number a capacity alert is built on, so it has to actually
/// reach 100 when every worker is occupied — not asymptotically, not usually.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn saturation_reaches_full_when_every_worker_is_rendering() {
    let fixture = common::engine_with(&slow_bundle(400), |b| b.pool_size(2).queue_capacity(16));
    let engine = fixture.shared();

    // Occupy both workers.
    let busy: Vec<_> = (0..2)
        .map(|i| {
            let engine = Arc::clone(&engine);
            tokio::spawn(async move { engine.render_uncached(&format!("/slow/{i}"), "{}").await })
        })
        .collect();

    tokio::time::sleep(Duration::from_millis(150)).await;

    let m = engine.pool_metrics();
    assert_eq!(m.busy, 2, "both workers are inside a render");
    assert_eq!(m.workers, 2);
    assert!(
        (m.saturation - 100.0).abs() < f64::EPSILON,
        "saturation must read 100 when the pool is full, got {}",
        m.saturation
    );

    for b in busy {
        b.await.unwrap().unwrap();
    }

    assert_eq!(engine.pool_metrics().busy, 0, "and fall back afterwards");
}

/// Queue depth is the other half: saturation says the pool is full, queue
/// pressure says how much is piling up behind it. A dashboard needs both to
/// tell "busy" from "losing".
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn queued_requests_are_visible_while_they_wait() {
    let fixture = common::engine_with(&slow_bundle(500), |b| b.pool_size(1).queue_capacity(32));
    let engine = fixture.shared();

    // One render occupies the single worker; the rest must queue behind it.
    let inflight: Vec<_> = (0..6)
        .map(|i| {
            let engine = Arc::clone(&engine);
            tokio::spawn(async move { engine.render_uncached(&format!("/q/{i}"), "{}").await })
        })
        .collect();

    tokio::time::sleep(Duration::from_millis(200)).await;

    let m = engine.pool_metrics();
    assert_eq!(m.busy, 1, "the one worker is rendering");
    assert!(
        m.queued >= 4,
        "the others must be visible as queued, got {}",
        m.queued
    );
    assert!(
        m.queue_pressure > 0.0 && m.queue_pressure <= 100.0,
        "queue pressure must be a live percentage, got {}",
        m.queue_pressure
    );

    for t in inflight {
        t.await.unwrap().unwrap();
    }

    let m = engine.pool_metrics();
    assert_eq!(m.queued, 0, "and drain to nothing");
    assert_eq!(m.renders, 6);
}

// ── the histogram ────────────────────────────────────────────────────────────

/// `pool_size` is chosen against render duration, so the percentiles have to be
/// right to within the resolution they claim (four buckets per octave, so 25%).
#[tokio::test]
async fn render_percentiles_reflect_what_renders_actually_cost() {
    let engine = common::engine_with(&slow_bundle(20), |b| b.pool_size(1));

    for i in 0..20 {
        engine
            .render_uncached(&format!("/p/{i}"), "{}")
            .await
            .unwrap();
    }

    let m = engine.pool_metrics();
    assert_eq!(m.renders, 20);

    // Each render busy-waits ~20 ms. The bucket floor is at most 25% below the
    // true value, and scheduling only ever adds time.
    assert!(
        m.render_p50 >= Duration::from_millis(14) && m.render_p50 <= Duration::from_millis(60),
        "p50 should land near the 20 ms each render takes, got {:?}",
        m.render_p50
    );
    assert!(
        m.render_p99 >= m.render_p50,
        "p99 cannot be below p50: {:?} vs {:?}",
        m.render_p99,
        m.render_p50
    );
}

#[tokio::test]
async fn percentiles_are_zero_before_anything_has_rendered() {
    let engine = common::engine_with(FAST, |b| b.pool_size(1));
    let m = engine.pool_metrics();
    assert_eq!(m.render_p50, Duration::ZERO);
    assert_eq!(m.render_p99, Duration::ZERO);
}

// ── failures ─────────────────────────────────────────────────────────────────

#[tokio::test]
async fn a_throwing_render_is_counted_as_failed_but_still_a_render() {
    let engine = common::engine_with(
        "globalThis.renderPage = (url) => { if (url === '/bad') throw new Error('x'); return '<p>ok</p>'; };",
        |b| b.pool_size(1),
    );

    engine.render_uncached("/good", "{}").await.unwrap();
    assert!(engine.render_uncached("/bad", "{}").await.is_err());
    engine.render_uncached("/good", "{}").await.unwrap();

    let m = engine.pool_metrics();
    assert_eq!(m.renders, 3, "a failed render still occupied a worker");
    assert_eq!(m.failed, 1, "and is counted as a failure");
    assert_eq!(m.timeouts, 0, "a throw is not a timeout");
}

/// The counter that distinguishes "slow" from "over capacity". A request that
/// never got served is invisible in render counts by definition — it never
/// became a render.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn requests_that_never_get_served_are_counted_as_timeouts() {
    let fixture = common::engine_with(&slow_bundle(3000), |b| {
        b.pool_size(1)
            .queue_capacity(1)
            .request_timeout(Some(Duration::from_millis(250)))
    });
    let engine = fixture.shared();

    // Fill the worker and the single queue slot.
    let busy: Vec<_> = (0..2)
        .map(|i| {
            let engine = Arc::clone(&engine);
            tokio::spawn(async move { engine.render_uncached(&format!("/busy/{i}"), "{}").await })
        })
        .collect();
    tokio::time::sleep(Duration::from_millis(100)).await;

    assert!(
        engine.render_uncached("/late", "{}").await.is_err(),
        "there was nowhere for this request to go"
    );

    assert!(
        engine.pool_metrics().timeouts >= 1,
        "a request that timed out must show up as one"
    );

    for b in busy {
        let _ = b.await;
    }
}
