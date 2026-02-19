//! Load test for Rusty SSR engine
//!
//! Run with: cargo run --example loadtest --release

use rusty_ssr::SsrEngine;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

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

const DURATION_SECS: u64 = 60;
const CONCURRENCY: usize = 32;
const URL_POOL_SIZE: usize = 500_000;
const CACHE_HIT_RATIO: usize = 0; // 0 = every request is a unique URL → always hits V8

#[tokio::main]
async fn main() {
    let dir = tempfile::tempdir().unwrap();
    let bundle_path = dir.path().join("loadtest-bundle.js");
    std::fs::write(&bundle_path, TEST_BUNDLE).unwrap();

    let pool_size = num_cpus::get();

    println!("=== Rusty SSR Load Test ===");
    println!("Duration:    {} seconds", DURATION_SECS);
    println!("Concurrency: {} tasks", CONCURRENCY);
    println!("V8 workers:  {}", pool_size);
    println!("URL pool:    {} unique URLs", URL_POOL_SIZE);
    println!("Cache ratio: {}% repeated URLs", CACHE_HIT_RATIO);
    println!();

    let engine = Arc::new(
        SsrEngine::builder()
            .bundle_path(&bundle_path)
            .pool_size(pool_size)
            .cache_size(1_000_000)
            .cache_ttl_secs(120)
            .build_engine()
            .expect("Failed to create engine"),
    );

    // Pre-generate URLs
    let urls: Vec<String> = (0..URL_POOL_SIZE)
        .map(|i| format!("/page/{}", i))
        .collect();

    // Pre-generate JSON payloads
    let payloads: Vec<String> = (0..10)
        .map(|i| {
            serde_json::json!({
                "items": (0..i*3).map(|j| format!("item-{}", j)).collect::<Vec<_>>()
            })
            .to_string()
        })
        .collect();

    println!("Warming up V8 workers...");
    // Warm up: render a few pages to initialize all workers
    for url in urls.iter().take(pool_size * 2) {
        let _ = engine.render(url).await;
    }
    println!("Warm-up done.\n");

    let stop = Arc::new(AtomicBool::new(false));
    let total_requests = Arc::new(AtomicU64::new(0));
    let total_errors = Arc::new(AtomicU64::new(0));
    let latencies = Arc::new(Mutex::new(Vec::with_capacity(500_000)));

    let start = Instant::now();

    // Spawn progress reporter
    let stop_c = Arc::clone(&stop);
    let total_c = Arc::clone(&total_requests);
    let errors_c = Arc::clone(&total_errors);
    let engine_c = Arc::clone(&engine);
    let progress = tokio::spawn(async move {
        let mut last_count = 0u64;
        loop {
            tokio::time::sleep(Duration::from_secs(5)).await;
            if stop_c.load(Ordering::Relaxed) {
                break;
            }
            let current = total_c.load(Ordering::Relaxed);
            let errors = errors_c.load(Ordering::Relaxed);
            let elapsed = start.elapsed().as_secs_f64();
            let rps = (current - last_count) as f64 / 5.0;
            let metrics = engine_c.cache_metrics();
            println!(
                "  [{:>3.0}s] {:>8} reqs | {:>8.0} rps | {:>4} errors | hit rate: {:.1}%",
                elapsed, current, rps, errors, metrics.hit_rate
            );
            last_count = current;
        }
    });

    let url_counter = Arc::new(AtomicU64::new(0));

    // Spawn worker tasks
    let mut handles = vec![];
    for task_id in 0..CONCURRENCY {
        let engine = Arc::clone(&engine);
        let stop = Arc::clone(&stop);
        let total_requests = Arc::clone(&total_requests);
        let total_errors = Arc::clone(&total_errors);
        let latencies = Arc::clone(&latencies);
        let urls = urls.clone();
        let payloads = payloads.clone();
        let url_counter = Arc::clone(&url_counter);

        handles.push(tokio::spawn(async move {
            let mut i = task_id;
            while !stop.load(Ordering::Relaxed) {
                // Pick URL: CACHE_HIT_RATIO% chance of reusing a "hot" URL (first 20)
                let url = if CACHE_HIT_RATIO > 0 && i % 100 < CACHE_HIT_RATIO {
                    urls[i % 20].clone() // hot set
                } else {
                    // Unique URL every time → guaranteed V8 render
                    let n = url_counter.fetch_add(1, Ordering::Relaxed);
                    format!("/render/{}", n)
                };
                let payload = &payloads[i % payloads.len()];

                let req_start = Instant::now();
                let result = engine.render_with_data(&url, payload).await;
                let latency = req_start.elapsed();

                match result {
                    Ok(_) => {
                        total_requests.fetch_add(1, Ordering::Relaxed);
                        latencies.lock().await.push(latency);
                    }
                    Err(_) => {
                        total_errors.fetch_add(1, Ordering::Relaxed);
                    }
                }

                i += CONCURRENCY;
            }
        }));
    }

    // Wait for duration
    tokio::time::sleep(Duration::from_secs(DURATION_SECS)).await;
    stop.store(true, Ordering::Relaxed);

    // Wait for all tasks
    for h in handles {
        let _ = h.await;
    }
    let _ = progress.await;

    let elapsed = start.elapsed();

    // Collect and sort latencies
    let mut lats = latencies.lock().await;
    lats.sort();
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
        let p50 = lats[lats.len() * 50 / 100];
        let p95 = lats[lats.len() * 95 / 100];
        let p99 = lats[lats.len() * 99 / 100];
        let max = lats[lats.len() - 1];
        let avg = Duration::from_nanos(
            (lats.iter().map(|d| d.as_nanos() as u64).sum::<u64>()) / lats.len() as u64,
        );

        println!("Latency:");
        println!("  avg:  {:>10.3?}", avg);
        println!("  p50:  {:>10.3?}", p50);
        println!("  p95:  {:>10.3?}", p95);
        println!("  p99:  {:>10.3?}", p99);
        println!("  max:  {:>10.3?}", max);
    }

    println!();
    println!("Cache:");
    println!("  lookups:    {}", metrics.lookups);
    println!("  hot hits:   {}", metrics.hot_hits);
    println!("  cold hits:  {}", metrics.cold_hits);
    println!("  misses:     {}", metrics.misses);
    println!("  hit rate:   {:.1}%", metrics.hit_rate);
    println!("  evictions:  {}", metrics.evictions);
    println!("  cold size:  {}/{}", metrics.cold_size, metrics.cold_capacity);
}
