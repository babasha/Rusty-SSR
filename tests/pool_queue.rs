//! What the pool's work queue promises.
//!
//! Every render crosses this queue, so its guarantees are the engine's: each
//! request reaches exactly one worker, each answer comes back to the caller who
//! asked, a full queue makes callers wait rather than fail, and dropping the
//! pool stops the threads.
//!
//! Written when the queue changed shape — it was a `std::sync::mpsc::Receiver`
//! shared between workers behind a `Mutex`, with the blocking `recv()` called
//! while that mutex was held — so what follows is the behaviour that had to
//! survive the change, stated in terms a caller can see.

#![cfg(feature = "v8-pool")]

mod common;

use std::sync::Arc;
use std::time::{Duration, Instant};


fn engine_with(bundle: &str, pool_size: usize, queue_capacity: usize) -> common::Fixture {
    common::engine_with(bundle, |b| {
        b.pool_size(pool_size)
            .queue_capacity(queue_capacity)
            .request_timeout(Some(Duration::from_secs(10)))
    })
}

/// Echoes the URL, so a crossed response is visible rather than plausible.
const ECHO: &str = "globalThis.renderPage = (url) => '<main>' + url + '</main>';";

// ── every answer reaches the caller who asked for it ─────────────────────────

/// The property a work queue exists to provide, and the one whose failure is
/// worst: if two requests in flight can have their answers swapped, one visitor
/// is served another visitor's page. Many concurrent callers, distinct URLs,
/// several workers — every reply must match its request.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn no_request_is_answered_with_another_request_s_page() {
    let engine = engine_with(ECHO, 4, 64);
    let engine = engine.shared();

    const N: usize = 400;
    let tasks: Vec<_> = (0..N)
        .map(|i| {
            let engine = Arc::clone(&engine);
            tokio::spawn(async move {
                let url = format!("/page/{i}");
                let html = engine.render_uncached(&url, "{}").await.unwrap();
                (i, html)
            })
        })
        .collect();

    for task in tasks {
        let (i, html) = task.await.unwrap();
        assert_eq!(
            html,
            format!("<main>/page/{i}</main>"),
            "request {i} was answered with somebody else's page"
        );
    }
}

/// The same, with more callers than the queue can hold at once, so the
/// backpressure path is exercised while the answers are being checked.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn answers_stay_matched_when_the_queue_is_smaller_than_the_burst() {
    let engine = engine_with(ECHO, 2, 4);
    let engine = engine.shared();

    const N: usize = 200;
    let tasks: Vec<_> = (0..N)
        .map(|i| {
            let engine = Arc::clone(&engine);
            tokio::spawn(async move {
                let url = format!("/p/{i}");
                (i, engine.render_uncached(&url, "{}").await.unwrap())
            })
        })
        .collect();

    for task in tasks {
        let (i, html) = task.await.unwrap();
        assert_eq!(html, format!("<main>/p/{i}</main>"));
    }
}

/// Every request must be delivered exactly once. A queue that dropped or
/// duplicated tasks under contention would show up here as a count that is not
/// N — the bundle counts the calls it received.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn every_request_reaches_a_worker_exactly_once() {
    // One worker, so the count lives in one isolate and needs no sharing.
    let engine = engine_with(
        "globalThis.__n = 0; globalThis.renderPage = () => String(++globalThis.__n);",
        1,
        32,
    );
    let engine = engine.shared();

    const N: usize = 300;
    let tasks: Vec<_> = (0..N)
        .map(|i| {
            let engine = Arc::clone(&engine);
            tokio::spawn(async move { engine.render_uncached(&format!("/p/{i}"), "{}").await })
        })
        .collect();

    let mut seen: Vec<usize> = Vec::with_capacity(N);
    for task in tasks {
        seen.push(task.await.unwrap().unwrap().parse().unwrap());
    }

    seen.sort_unstable();
    let expected: Vec<usize> = (1..=N).collect();
    assert_eq!(
        seen, expected,
        "each request must be counted once — no drops, no double delivery"
    );
}

// ── the queue bounds the queue, not the concurrency ──────────────────────────

/// `queue_capacity` limits how many requests may be *waiting*. It must not
/// double as a cap on how many may be *running*: a worker frees its queue slot
/// when it picks the task up, not when the render finishes. With a capacity of
/// one and four workers, four renders still have to overlap — otherwise the
/// pool has silently become single-threaded for anyone who set a small queue.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_small_queue_does_not_serialise_the_workers() {
    // Each render busy-waits ~150ms. Four in parallel take ~150ms; four in
    // series take ~600ms.
    let bundle = "globalThis.renderPage = () => { \
        const end = Date.now() + 150; while (Date.now() < end) {} return '<i>x</i>'; };";
    let engine = engine_with(bundle, 4, 1);
    let engine = engine.shared();

    // Warm every isolate first, so V8 start-up is not part of the measurement.
    for _ in 0..4 {
        let _ = engine.render_uncached("/warm", "{}").await;
    }

    let started = Instant::now();
    let tasks: Vec<_> = (0..4)
        .map(|i| {
            let engine = Arc::clone(&engine);
            tokio::spawn(async move { engine.render_uncached(&format!("/p/{i}"), "{}").await })
        })
        .collect();
    for task in tasks {
        task.await.unwrap().unwrap();
    }
    let elapsed = started.elapsed();

    assert!(
        elapsed < Duration::from_millis(450),
        "four workers and a one-slot queue should overlap, took {elapsed:?} — \
         close to the {:?} that running them one at a time would cost",
        Duration::from_millis(600),
    );
}

// ── backpressure ─────────────────────────────────────────────────────────────

/// A caller that cannot get a queue slot before its deadline gets `Timeout`,
/// and gets it *at* the deadline rather than long after. The old
/// implementation span on `try_send` until the deadline passed; the point of
/// this test is that the answer still arrives on time now that it waits.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_caller_that_cannot_be_queued_times_out_on_schedule() {
    let fixture = common::engine_with(
        "globalThis.renderPage = () => { \
         const end = Date.now() + 2000; while (Date.now() < end) {} return '<i>x</i>'; };",
        |b| {
            b.queue_capacity(1)
                .request_timeout(Some(Duration::from_millis(300)))
        },
    );
    let engine = fixture.shared();

    // Occupy the worker and the single queue slot.
    let busy: Vec<_> = (0..2)
        .map(|i| {
            let engine = Arc::clone(&engine);
            tokio::spawn(async move { engine.render_uncached(&format!("/busy/{i}"), "{}").await })
        })
        .collect();

    // Give them a moment to actually take the worker and the slot.
    tokio::time::sleep(Duration::from_millis(100)).await;

    let started = Instant::now();
    let result = engine.render_uncached("/late", "{}").await;
    let waited = started.elapsed();

    assert!(result.is_err(), "a request that never got a slot must not succeed");
    assert!(
        waited < Duration::from_millis(1500),
        "the timeout must fire on its own schedule, waited {waited:?}"
    );

    for b in busy {
        let _ = b.await;
    }
}

/// Backpressure must be temporary: once the queue drains, later requests go
/// through. A permit that is not returned would wedge the pool permanently
/// after the first burst, and nothing else in the suite would notice.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_pool_keeps_working_after_the_queue_has_been_full() {
    let engine = engine_with(ECHO, 2, 2);
    let engine = engine.shared();

    // Several bursts far larger than the queue.
    for round in 0..5 {
        let tasks: Vec<_> = (0..50)
            .map(|i| {
                let engine = Arc::clone(&engine);
                tokio::spawn(async move {
                    engine
                        .render_uncached(&format!("/r{round}/p{i}"), "{}")
                        .await
                })
            })
            .collect();
        for task in tasks {
            task.await.unwrap().expect("every request must eventually be served");
        }
    }

    // And a plain one afterwards still works.
    assert_eq!(
        engine.render_uncached("/after", "{}").await.unwrap(),
        "<main>/after</main>"
    );
}

// ── workers ──────────────────────────────────────────────────────────────────

#[tokio::test]
async fn the_pool_reports_the_workers_it_started() {
    let engine = engine_with(ECHO, 3, 8);
    assert_eq!(engine.worker_count(), 3);
}

/// Dropping the engine has to stop the worker threads. Nothing observable
/// remains to assert on afterwards, so this asserts the thing that would break
/// if they did not: the process must be able to build and drop many pools
/// without accumulating threads that never exit.
#[tokio::test]
async fn pools_can_be_created_and_dropped_repeatedly() {
    for _ in 0..8 {
        let engine = engine_with(ECHO, 2, 4);
        assert_eq!(engine.render_uncached("/x", "{}").await.unwrap(), "<main>/x</main>");
        drop(engine);
    }

    // A fresh pool after all that still works.
    let engine = engine_with(ECHO, 2, 4);
    assert_eq!(engine.render_uncached("/y", "{}").await.unwrap(), "<main>/y</main>");
}

/// A render that throws must not take the worker down with it — the pool would
/// silently shrink, and a pool of one would stop answering entirely.
#[tokio::test]
async fn a_throwing_render_leaves_the_worker_usable() {
    let engine = engine_with(
        "globalThis.renderPage = (url) => { \
         if (url === '/boom') throw new Error('kaboom'); return '<main>' + url + '</main>'; };",
        1,
        8,
    );

    for _ in 0..5 {
        let err = engine.render_uncached("/boom", "{}").await.unwrap_err();
        assert!(err.to_string().contains("kaboom"), "got: {err}");
        assert_eq!(
            engine.render_uncached("/ok", "{}").await.unwrap(),
            "<main>/ok</main>",
            "the worker must keep serving after a throw"
        );
    }

    assert_eq!(engine.worker_count(), 1, "the worker must still be alive");
}
