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
//!   --rows <n>            Listings in the payload; this is the dial that
//!                         decides what one render costs. Default: 5
//!   --bundle <path>       Render with a real bundle instead of the built-in
//!                         stub. The stub measures engine overhead; a real
//!                         framework bundle measures the pool.
//!   --quiet               Result lines only
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
        let items = "";
        if (data && data.rows) {
            items = "<ul>" + data.rows.map(r => "<li>" + r.title + "</li>").join("") + "</ul>";
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
    rows: usize,
    bundle: Option<String>,
    quiet: bool,
}

impl Options {
    fn parse() -> Self {
        let args: Vec<String> = std::env::args().skip(1).collect();
        let text = |name: &str| -> Option<String> {
            args.iter()
                .position(|a| a == name)
                .and_then(|i| args.get(i + 1))
                .cloned()
        };
        let value = |name: &str, default: usize| -> usize {
            text(name).and_then(|v| v.parse().ok()).unwrap_or(default)
        };

        Self {
            duration: Duration::from_secs(value("--duration", 30) as u64),
            concurrency: value("--concurrency", 32),
            hit_ratio: value("--hit-ratio", 0).min(100),
            pool_size: value("--pool-size", num_cpus::get()),
            cache_size: value("--cache-size", 300),
            rows: value("--rows", 5),
            bundle: text("--bundle"),
            quiet: args.iter().any(|a| a == "--quiet"),
        }
    }
}

/// The payload a render is given: a page's worth of listings.
///
/// `rows` is the dial that decides what a render *costs*, which is the whole
/// point of pointing this at a real bundle. A framework rendering eighty cards
/// is doing the work an actual page does; a bundle that concatenates a string
/// is measuring the engine's own overhead and nothing else.
fn build_payload(rows: usize) -> String {
    const DISTRICTS: [&str; 4] = ["Velha", "Garcia", "Itoupava", "Centro"];
    let listings: Vec<serde_json::Value> = (0..rows)
        .map(|i| {
            serde_json::json!({
                "id": i,
                "slug": format!("apartamento-{i}-blumenau"),
                "title": format!("Apartamento {} quartos — Edifício {i}", 1 + i % 4),
                "district": DISTRICTS[i % 4],
                "city": "Blumenau",
                "photo": format!("/media/{i}.jpg"),
                "bedrooms": 1 + i % 4,
                "bathrooms": 1 + i % 3,
                "parking": i % 3,
                "area": 45 + (i * 7) % 160,
                "price": 25_000_000i64 + (i as i64 * 137_000),
                "discount": if i % 5 == 0 { 10 } else { 0 },
                "featured": i % 7 == 0,
            })
        })
        .collect();

    serde_json::json!({
        "heading": "Apartamentos à venda em Blumenau",
        "tipo": "venda",
        "rows": listings,
    })
    .to_string()
}

#[tokio::main]
async fn main() {
    let opts = Options::parse();

    let dir = tempfile::tempdir().unwrap();
    let bundle_path = match &opts.bundle {
        Some(path) => std::path::PathBuf::from(path),
        None => {
            let p = dir.path().join("loadtest-bundle.js");
            std::fs::write(&p, TEST_BUNDLE).unwrap();
            p
        }
    };
    let bundle_bytes = std::fs::metadata(&bundle_path).map(|m| m.len()).unwrap_or(0);

    println!("=== Rusty SSR Load Test ===");
    println!("Duration:    {}s", opts.duration.as_secs());
    println!("Concurrency: {} in flight", opts.concurrency);
    println!("V8 workers:  {}", opts.pool_size);
    println!("Cache:       {} entries", opts.cache_size);
    println!(
        "Hit ratio:   {}% (hot set of {} URLs)",
        opts.hit_ratio, HOT_SET
    );
    println!(
        "Bundle:      {} ({} bytes)",
        match &opts.bundle {
            Some(p) => p.as_str(),
            None => "built-in stub",
        },
        bundle_bytes
    );
    println!("Rows:        {} listings per render", opts.rows);
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

    let payload: Arc<String> = Arc::new(build_payload(opts.rows));

    // What one render actually costs, measured before any load is applied:
    // the pool's ceiling is `pool_size / render_time`, so this is the number
    // every throughput figure below is a consequence of.
    // V8 starts interpreted and only tiers up after a few hundred calls, so the
    // first renders cost several times what the steady state does. Measuring
    // without discarding them reports the warm-up, not the workload — at
    // `--rows 1` that was the difference between 315 µs and the real figure.
    const PROBE_WARMUP: u32 = 400;
    const PROBE_RENDERS: u32 = 200;
    let mut probe_bytes = 0usize;
    for i in 0..PROBE_WARMUP {
        if let Err(e) = engine.render_uncached(&format!("/warmup/{i}"), &payload).await {
            eprintln!("probe render failed: {e}");
            std::process::exit(1);
        }
    }
    let probe_start = Instant::now();
    for i in 0..PROBE_RENDERS {
        match engine.render_uncached(&format!("/probe/{i}"), &payload).await {
            Ok(html) => probe_bytes = html.len(),
            Err(e) => {
                eprintln!("probe render failed: {e}");
                std::process::exit(1);
            }
        }
    }
    let per_render = probe_start.elapsed() / PROBE_RENDERS;
    println!(
        "Payload {} bytes → {} bytes of HTML, {:.3?} per render (single-threaded)",
        payload.len(),
        probe_bytes,
        per_render
    );
    println!(
        "Pool ceiling at this cost: {:.0} renders/s across {} workers",
        opts.pool_size as f64 / per_render.as_secs_f64(),
        opts.pool_size
    );

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

    // Progress reporter. Skipped under --quiet, which is what a sweep wants:
    // one line per configuration rather than one every five seconds.
    let progress = if opts.quiet {
        tokio::spawn(async {})
    } else {
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
                let p = engine.pool_metrics();
                println!(
                    "  [{:>3.0}s] {:>9} reqs | {:>9.0} rps | {:>4} errors | hit {:.1}% | pool {:>3.0}% busy ({}/{}), q={} | p99 {:?}",
                    start.elapsed().as_secs_f64(),
                    current,
                    (current - last) as f64 / 5.0,
                    errors.load(Ordering::Relaxed),
                    m.hit_rate,
                    p.saturation,
                    p.busy, p.workers, p.queued,
                    p.render_p99
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
    let p = engine.pool_metrics();
    println!("Pool:");
    println!("  workers:    {}", p.workers);
    println!("  renders:    {} ({} failed, {} timed out)", p.renders, p.failed, p.timeouts);
    println!("  render p50: {:?}", p.render_p50);
    println!("  render p95: {:?}", p.render_p95);
    println!("  render p99: {:?}", p.render_p99);
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
