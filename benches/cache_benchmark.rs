//! Cache Performance Benchmarks
//!
//! Run with: `cargo bench --bench cache_benchmark`
//!
//! These benchmarks measure:
//! - DashMap concurrent read/write performance
//! - LRU cache eviction overhead
//! - Thread-local cache access patterns
//! - Cache hit/miss ratios impact

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use dashmap::DashMap;
use std::sync::Arc;
use std::time::Duration;

/// Benchmark DashMap concurrent access
fn bench_dashmap_concurrent(c: &mut Criterion) {
    let mut group = c.benchmark_group("dashmap_concurrent");

    for num_threads in [1, 2, 4, 8] {
        group.throughput(Throughput::Elements(10000));
        group.bench_with_input(
            BenchmarkId::new("read_write", num_threads),
            &num_threads,
            |b, &threads| {
                let map: Arc<DashMap<String, String>> = Arc::new(DashMap::new());

                // Pre-populate with some data
                for i in 0..1000 {
                    map.insert(format!("key_{}", i), format!("value_{}", i));
                }

                b.iter(|| {
                    let handles: Vec<_> = (0..threads)
                        .map(|t| {
                            let map = Arc::clone(&map);
                            std::thread::spawn(move || {
                                for i in 0..10000 / threads {
                                    let key = format!("key_{}", (t * 1000 + i) % 1000);
                                    // 80% reads, 20% writes
                                    if i % 5 == 0 {
                                        map.insert(key.clone(), format!("new_value_{}", i));
                                    } else {
                                        let _ = map.get(&key);
                                    }
                                }
                            })
                        })
                        .collect();

                    for h in handles {
                        h.join().unwrap();
                    }

                    black_box(map.len())
                })
            },
        );
    }

    group.finish();
}

/// Benchmark DashMap sharding performance
fn bench_dashmap_sharding(c: &mut Criterion) {
    let mut group = c.benchmark_group("dashmap_sharding");

    // Test different key distributions
    group.bench_function("sequential_keys", |b| {
        let map: DashMap<u64, String> = DashMap::new();

        b.iter(|| {
            for i in 0..10000u64 {
                map.insert(i, format!("value_{}", i));
            }
            black_box(map.len())
        })
    });

    group.bench_function("random_keys", |b| {
        let map: DashMap<u64, String> = DashMap::new();

        b.iter(|| {
            // Simulate random-ish keys (URL hashes)
            for i in 0..10000u64 {
                let key = i.wrapping_mul(2654435761); // Knuth multiplicative hash
                map.insert(key, format!("value_{}", i));
            }
            black_box(map.len())
        })
    });

    group.finish();
}

/// Benchmark cache hit performance (simulating L1/L2 cache)
fn bench_cache_hits(c: &mut Criterion) {
    let mut group = c.benchmark_group("cache_performance");

    // Simulate L1 cache (thread-local, small)
    group.bench_function("l1_cache_hit", |b| {
        let cache: Vec<(String, String)> = (0..128)
            .map(|i| (format!("url_{}", i), format!("<html>{}</html>", i)))
            .collect();

        b.iter(|| {
            // Simulate cache lookup
            let url = "url_50";
            let result = cache.iter().find(|(k, _)| k == url);
            black_box(result)
        })
    });

    // Simulate L2 cache (shared DashMap)
    group.bench_function("l2_cache_hit", |b| {
        let cache: DashMap<String, String> = DashMap::new();
        for i in 0..10000 {
            cache.insert(format!("url_{}", i), format!("<html>{}</html>", i));
        }

        b.iter(|| {
            let result = cache.get("url_5000");
            black_box(result)
        })
    });

    // Simulate cache miss + insert
    group.bench_function("cache_miss_insert", |b| {
        let cache: DashMap<String, String> = DashMap::new();
        let mut counter = 0u64;

        b.iter(|| {
            counter += 1;
            let key = format!("new_url_{}", counter);
            let value = format!("<html>content_{}</html>", counter);
            cache.insert(key.clone(), value);
            let result = cache.get(&key);
            black_box(result)
        })
    });

    group.finish();
}

/// Benchmark LRU eviction overhead
fn bench_lru_eviction(c: &mut Criterion) {
    use lru::LruCache;
    use std::num::NonZeroUsize;

    let mut group = c.benchmark_group("lru_eviction");

    for cache_size in [128, 512, 2048] {
        group.bench_with_input(
            BenchmarkId::new("eviction", cache_size),
            &cache_size,
            |b, &size| {
                let mut cache = LruCache::new(NonZeroUsize::new(size).unwrap());

                // Fill cache to capacity
                for i in 0..size {
                    cache.put(format!("key_{}", i), format!("value_{}", i));
                }

                let mut counter = size;

                b.iter(|| {
                    // This will trigger eviction
                    counter += 1;
                    cache.put(format!("key_{}", counter), format!("value_{}", counter));

                    // Access some existing key to test LRU ordering
                    let _ = cache.get(&format!("key_{}", counter - 10));

                    black_box(cache.len())
                })
            },
        );
    }

    group.finish();
}

/// Benchmark Arc<str> vs String cloning
fn bench_arc_vs_string(c: &mut Criterion) {
    use std::sync::Arc;

    let mut group = c.benchmark_group("arc_vs_string");

    let html = "<html><head><title>Test</title></head><body><div>Content</div></body></html>";

    group.bench_function("string_clone", |b| {
        let s = html.to_string();
        b.iter(|| {
            let cloned = s.clone();
            black_box(cloned)
        })
    });

    group.bench_function("arc_str_clone", |b| {
        let s: Arc<str> = html.into();
        b.iter(|| {
            let cloned = Arc::clone(&s);
            black_box(cloned)
        })
    });

    // Large HTML
    let large_html = html.repeat(100);

    group.bench_function("string_clone_large", |b| {
        let s = large_html.clone();
        b.iter(|| {
            let cloned = s.clone();
            black_box(cloned)
        })
    });

    group.bench_function("arc_str_clone_large", |b| {
        let s: Arc<str> = large_html.as_str().into();
        b.iter(|| {
            let cloned = Arc::clone(&s);
            black_box(cloned)
        })
    });

    group.finish();
}

/// Configure criterion
fn criterion_config() -> Criterion {
    Criterion::default()
        .sample_size(100)
        .measurement_time(Duration::from_secs(5))
        .warm_up_time(Duration::from_secs(2))
        .with_plots()
}

criterion_group! {
    name = benches;
    config = criterion_config();
    targets = bench_dashmap_concurrent, bench_dashmap_sharding, bench_cache_hits, bench_lru_eviction, bench_arc_vs_string
}

criterion_main!(benches);
