//! Load test for the Rusty SSR engine.
//!
//! ```text
//! cargo run --release --example loadtest -- [options]
//!
//!   --duration <secs>     How long to drive load. Default: 30
//!   --concurrency <n>     Concurrent in-flight requests. Default: 32
//!   --hit-ratio <0..100>  Percent of requests drawn from a hot URL set, i.e.
//!                         served from the cache. Default: 0
//!   --pool-size <n>       V8 workers. Default: one per CPU
//!   --cache-size <n>      Fragment-cache capacity. Default: 300
//!   --payload <bytes>     Approximate size of the JSON payload. Default: 64
//! ```
//!
//! Two scenarios are worth running, and they measure entirely different things:
//!
//! - `--hit-ratio 0` — every URL is unique, so every request is a V8 render.
//!   This measures the pool: dispatch, isolate throughput, and how the whole
//!   thing behaves when the queue is the bottleneck.
//! - `--hit-ratio 100` — a small hot set that fits in the cache, so after
//!   warm-up almost nothing reaches V8. This measures the cache, and it is the
//!   number a "requests per second with caching" claim is actually about.
//!
//! ## On the harness itself
//!
//! Two things here exist to keep the harness out of the measurement, and both
//! were wrong in the version this replaced:
//!
//! - **Latencies are recorded per task and merged at the end.** They used to be
//!   pushed into one shared `Mutex<Vec<_>>` on every request, which serialises
//!   every driver task through one lock. That is survivable when a request
//!   costs milliseconds and completely dominant when it costs a hundred
//!   nanoseconds — which is to say, it corrupted the cache scenario, the one
//!   that matters most.
//! - **The URL pool is shared, not copied.** It used to be `.clone()`d into
//!   each task: with the old defaults, sixteen million `String`s allocated
//!   before the first request, none of which were ever read.

use rusty_ssr::SsrEngine;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

const TEST_BUNDLE: &str = r#"
    globalThis.renderPage = async function(url, data) {
        let items = '';
        if (data && data.items) {
            items = '<ul>' + data.items.map(i => '<li>' + i + '</li>').join('') + '</ul>';
        }
        return '<!DOCTYPE html><html><head><title>' + url + '</title></head>'
             + '<body><h1>' + url + '</h1>' + items
             + '<footer>Rendered at ' + Date.now() + '</footer></body></html>';
    };
"#;

/// How many distinct URLs the hot set holds. Deliberately smaller than the
/// default cache so a hit-ratio run genuinely hits rather than thrashing.
const HOT_SET: usize = 200;

/// Latency samples kept per driver task.
///
/// Not "every request": a cache hit costs tens of nanoseconds, so a single task
/// completes hundreds of millions of them in a half-minute run, and recording
/// each one asks for four gigabytes. It also stops being a measurement — the
/// harness would be timing its own allocator. See [`Latencies`].
const SAMPLES_PER_TASK: usize = 1 << 16;

/// Iterations between yields to the runtime.
///
/// A cache hit resolves without ever awaiting anything that yields, so a driver
/// task on that path runs to completion without giving the scheduler a turn.
/// With one such task per core, nothing else on the runtime — the progress
/// reporter, most obviously — is ever polled again.
const YIELD_EVERY: u64 = 1024;

/// A uniform sample of latencies, in bounded memory.
///
/// Keeps every request until it is full, then keeps every second, then every
/// fourth, and so on — halving what it holds each time it fills and doubling
/// the stride to match. What is left at the end is an even sample of the whole
/// run rather than of its first moments, and the memory never grows.
struct Latencies {
    kept: Vec<Duration>,
    stride: u64,
    seen: u64,
}

impl Latencies {
    fn new() -> Self {
        Self {
            kept: Vec::with_capacity(SAMPLES_PER_TASK),
            stride: 1,
            seen: 0,
        }
    }

    #[inline]
    fn record(&mut self, latency: Duration) {
        self.seen += 1;
        if !self.seen.is_multiple_of(self.stride) {
            return;
        }
        if self.kept.len() == SAMPLES_PER_TASK {
            // Full: thin what we have by half and take half as often from here.
            let mut i = 0;
            self.kept.retain(|_| {
                i += 1;
                i % 2 == 0
            });
            self.stride *= 2;
        }
        self.kept.push(latency);
    }
}

struct Options {
    duration: Duration,
    concurrency: usize,
    hit_ratio: usize,
    pool_size: usize,
    cache_size: usize,
    payload_bytes: usize,
}

impl Options {
    fn parse() -> Self {
        let args: Vec<String> = std::env::args().skip(1).collect();
        let value = |name: &str, default: usize| -> usize {
            args.iter()
                .position(|a| a == name)
                .and_then(|i| args.get(i + 1))
                .and_then(|v| v.parse().ok())
                .unwrap_or(default)
        };

        Self {
            duration: Duration::from_secs(value("--duration", 30) as u64),
            concurrency: value("--concurrency", 32),
            hit_ratio: value("--hit-ratio", 0).min(100),
            pool_size: value("--pool-size", num_cpus::get()),
            cache_size: value("--cache-size", 300),
            payload_bytes: value("--payload", 64),
        }
    }
}

#[tokio::main]
async fn main() {
    let opts = Options::parse();

    let dir = tempfile::tempdir().unwrap();
    let bundle_path = dir.path().join("loadtest-bundle.js");
    std::fs::write(&bundle_path, TEST_BUNDLE).unwrap();

    println!("=== Rusty SSR Load Test ===");
    println!("Duration:    {}s", opts.duration.as_secs());
    println!("Concurrency: {} in flight", opts.concurrency);
    println!("V8 workers:  {}", opts.pool_size);
    println!("Cache:       {} entries", opts.cache_size);
    println!(
        "Hit ratio:   {}% (hot set of {} URLs)",
        opts.hit_ratio, HOT_SET
    );
    println!("Payload:     ~{} bytes", opts.payload_bytes);
    println!();

    let engine = Arc::new(
        SsrEngine::builder()
            .bundle_path(&bundle_path)
            .pool_size(opts.pool_size)
            .cache_size(opts.cache_size)
            .cache_ttl_secs(120)
            .build_engine()
            .expect("Failed to create engine"),
    );

    // The hot set, shared rather than copied into every task.
    let hot: Arc<Vec<String>> = Arc::new((0..HOT_SET).map(|i| format!("/hot/{i}")).collect());

    // One payload of roughly the requested size, built once.
    let item_count = opts.payload_bytes / 12;
    let payload: Arc<String> = Arc::new(
        serde_json::json!({
            "items": (0..item_count).map(|j| format!("item-{j}")).collect::<Vec<_>>()
        })
        .to_string(),
    );
    println!("Payload is {} bytes of JSON.", payload.len());

    println!("Warming up {} workers and the hot set...", opts.pool_size);
    for url in hot.iter().take(HOT_SET.min(opts.cache_size)) {
        let _ = engine.render_with_data(url, &payload).await;
    }
    for i in 0..opts.pool_size * 2 {
        let _ = engine.render_with_data(&format!("/warm/{i}"), &payload).await;
    }
    println!("Warm-up done.\n");

    let stop = Arc::new(AtomicBool::new(false));
    let total_requests = Arc::new(AtomicU64::new(0));
    let total_errors = Arc::new(AtomicU64::new(0));
    let url_counter = Arc::new(AtomicU64::new(0));

    let start = Instant::now();

    // Progress reporter.
    let progress = {
        let stop = Arc::clone(&stop);
        let total = Arc::clone(&total_requests);
        let errors = Arc::clone(&total_errors);
        let engine = Arc::clone(&engine);
        tokio::spawn(async move {
            let mut last = 0u64;
            loop {
                tokio::time::sleep(Duration::from_secs(5)).await;
                if stop.load(Ordering::Relaxed) {
                    break;
                }
                let current = total.load(Ordering::Relaxed);
                let m = engine.cache_metrics();
                println!(
                    "  [{:>3.0}s] {:>9} reqs | {:>9.0} rps | {:>4} errors | hit rate: {:.1}%",
                    start.elapsed().as_secs_f64(),
                    current,
                    (current - last) as f64 / 5.0,
                    errors.load(Ordering::Relaxed),
                    m.hit_rate
                );
                last = current;
            }
        })
    };

    // Driver tasks. Each keeps its own latency vector; they are merged after
    // the run, so nothing on the request path touches a shared lock.
    let mut handles = Vec::with_capacity(opts.concurrency);
    for task_id in 0..opts.concurrency {
        let engine = Arc::clone(&engine);
        let stop = Arc::clone(&stop);
        let total_requests = Arc::clone(&total_requests);
        let total_errors = Arc::clone(&total_errors);
        let url_counter = Arc::clone(&url_counter);
        let hot = Arc::clone(&hot);
        let payload = Arc::clone(&payload);
        let hit_ratio = opts.hit_ratio;

        handles.push(tokio::spawn(async move {
            let mut latencies = Latencies::new();
            let mut i = task_id;
            let mut since_yield = 0u64;
            // Declared here so the unique-URL branch can be borrowed rather than
            // forcing the hot branch to clone just to match its type.
            let mut unique;

            while !stop.load(Ordering::Relaxed) {
                let url = if hit_ratio > 0 && i % 100 < hit_ratio {
                    hot[i % HOT_SET].as_str()
                } else {
                    // Unique every time → guaranteed V8 render.
                    unique = format!("/render/{}", url_counter.fetch_add(1, Ordering::Relaxed));
                    unique.as_str()
                };

                let req_start = Instant::now();
                let result = engine.render_with_data(url, &payload).await;
                let latency = req_start.elapsed();

                match result {
                    Ok(_) => {
                        total_requests.fetch_add(1, Ordering::Relaxed);
                        latencies.record(latency);
                    }
                    Err(_) => {
                        total_errors.fetch_add(1, Ordering::Relaxed);
                    }
                }

                i = i.wrapping_add(opts.concurrency);

                // Hand the runtime a turn. On the cache path nothing else here
                // ever would, and a starved reporter cannot report.
                since_yield += 1;
                if since_yield == YIELD_EVERY {
                    since_yield = 0;
                    tokio::task::yield_now().await;
                }
            }

            latencies.kept
        }));
    }

    tokio::time::sleep(opts.duration).await;
    stop.store(true, Ordering::Relaxed);

    let mut lats: Vec<Duration> = Vec::new();
    for h in handles {
        if let Ok(mut task_lats) = h.await {
            lats.append(&mut task_lats);
        }
    }
    let _ = progress.await;

    let elapsed = start.elapsed();
    lats.sort_unstable();

    let total = total_requests.load(Ordering::Relaxed);
    let errors = total_errors.load(Ordering::Relaxed);
    let metrics = engine.cache_metrics();

    println!();
    println!("=== Results ===");
    println!("Duration:      {:.2}s", elapsed.as_secs_f64());
    println!("Total reqs:    {}", total);
    println!("Errors:        {}", errors);
    println!(
        "Throughput:    {:.0} req/s",
        total as f64 / elapsed.as_secs_f64()
    );
    println!();

    if !lats.is_empty() {
        let at = |p: usize| lats[(lats.len() - 1) * p / 100];
        let avg = Duration::from_nanos(
            lats.iter().map(|d| d.as_nanos() as u64).sum::<u64>() / lats.len() as u64,
        );

        println!("Latency:");
        println!("  avg:  {:>12.3?}", avg);
        println!("  p50:  {:>12.3?}", at(50));
        println!("  p95:  {:>12.3?}", at(95));
        println!("  p99:  {:>12.3?}", at(99));
        println!("  max:  {:>12.3?}", lats[lats.len() - 1]);
    }

    println!();
    println!("Cache:");
    println!("  lookups:    {}", metrics.lookups);
    println!("  hot hits:   {}", metrics.hot_hits);
    println!("  cold hits:  {}", metrics.cold_hits);
    println!("  misses:     {}", metrics.misses);
    println!("  hit rate:   {:.1}%", metrics.hit_rate);
    println!("  evictions:  {}", metrics.evictions);
    println!(
        "  cold size:  {}/{}",
        metrics.cold_size, metrics.cold_capacity
    );
}
