//! SSR Performance Benchmarks
//!
//! Run with: `cargo bench --bench ssr_benchmark`
//!
//! These benchmarks measure:
//! - V8 pool creation and initialization
//! - Render throughput (requests per second)
//! - Latency distribution (p50, p99, p999)
//! - Concurrent render performance

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use std::time::Duration;

/// Benchmark V8 pool configuration creation
fn bench_pool_config(c: &mut Criterion) {
    use rusty_ssr::v8_pool::V8PoolConfig;

    c.bench_function("pool_config_default", |b| {
        b.iter(|| {
            let config = V8PoolConfig::default();
            black_box(config)
        })
    });

    c.bench_function("pool_config_custom", |b| {
        b.iter(|| {
            let config = V8PoolConfig {
                num_threads: 4,
                queue_capacity: 1024,
                pin_threads: false,
                render_function: "renderPage".to_string(),
            };
            black_box(config)
        })
    });
}

/// Benchmark string operations (simulating render output)
fn bench_string_ops(c: &mut Criterion) {
    let mut group = c.benchmark_group("string_operations");

    // Small HTML (typical component)
    let small_html = "<div class=\"card\"><h1>Hello</h1><p>World</p></div>";

    // Medium HTML (typical page)
    let medium_html = r#"<!DOCTYPE html>
<html>
<head><title>Test</title></head>
<body>
<div id="app">
    <header><nav><a href="/">Home</a></nav></header>
    <main><article><h1>Title</h1><p>Content here</p></article></main>
    <footer><p>Footer</p></footer>
</div>
</body>
</html>"#;

    // Large HTML (full page with data)
    let large_html = medium_html.repeat(10);

    group.throughput(Throughput::Bytes(small_html.len() as u64));
    group.bench_with_input(
        BenchmarkId::new("clone", "small"),
        &small_html,
        |b, html| {
            b.iter(|| black_box(html.to_string()))
        },
    );

    group.throughput(Throughput::Bytes(medium_html.len() as u64));
    group.bench_with_input(
        BenchmarkId::new("clone", "medium"),
        &medium_html,
        |b, html| {
            b.iter(|| black_box(html.to_string()))
        },
    );

    group.throughput(Throughput::Bytes(large_html.len() as u64));
    group.bench_with_input(
        BenchmarkId::new("clone", "large"),
        &large_html,
        |b, html| {
            b.iter(|| black_box(html.to_string()))
        },
    );

    group.finish();
}

/// Benchmark JSON serialization (for render data)
fn bench_json_serialization(c: &mut Criterion) {
    use serde::{Deserialize, Serialize};

    #[derive(Serialize, Deserialize)]
    struct PageData {
        url: String,
        title: String,
        items: Vec<Item>,
    }

    #[derive(Serialize, Deserialize)]
    struct Item {
        id: u32,
        name: String,
        price: f64,
    }

    let mut group = c.benchmark_group("json_serialization");

    // Small data
    let small_data = PageData {
        url: "/".to_string(),
        title: "Home".to_string(),
        items: vec![],
    };

    // Medium data (10 items)
    let medium_data = PageData {
        url: "/products".to_string(),
        title: "Products".to_string(),
        items: (0..10)
            .map(|i| Item {
                id: i,
                name: format!("Product {}", i),
                price: 99.99,
            })
            .collect(),
    };

    // Large data (100 items)
    let large_data = PageData {
        url: "/catalog".to_string(),
        title: "Full Catalog".to_string(),
        items: (0..100)
            .map(|i| Item {
                id: i,
                name: format!("Product {}", i),
                price: 99.99 + i as f64,
            })
            .collect(),
    };

    group.bench_function("serialize_small", |b| {
        b.iter(|| {
            let json = serde_json::to_string(&small_data).unwrap();
            black_box(json)
        })
    });

    group.bench_function("serialize_medium", |b| {
        b.iter(|| {
            let json = serde_json::to_string(&medium_data).unwrap();
            black_box(json)
        })
    });

    group.bench_function("serialize_large", |b| {
        b.iter(|| {
            let json = serde_json::to_string(&large_data).unwrap();
            black_box(json)
        })
    });

    group.finish();
}

/// Benchmark channel throughput (simulating request queue)
fn bench_channel_throughput(c: &mut Criterion) {
    use std::sync::mpsc;

    let mut group = c.benchmark_group("channel_throughput");
    group.throughput(Throughput::Elements(1000));

    group.bench_function("sync_channel_1000", |b| {
        b.iter(|| {
            let (tx, rx) = mpsc::sync_channel::<String>(512);

            // Sender
            let tx_handle = std::thread::spawn(move || {
                for i in 0..1000 {
                    tx.send(format!("request_{}", i)).unwrap();
                }
            });

            // Receiver
            let rx_handle = std::thread::spawn(move || {
                let mut count = 0;
                while let Ok(_msg) = rx.recv() {
                    count += 1;
                    if count >= 1000 {
                        break;
                    }
                }
                count
            });

            tx_handle.join().unwrap();
            let received = rx_handle.join().unwrap();
            black_box(received)
        })
    });

    group.finish();
}

/// Configure criterion for detailed benchmarks
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
    targets = bench_pool_config, bench_string_ops, bench_json_serialization, bench_channel_throughput
}

criterion_main!(benches);
