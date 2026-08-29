//! Hot-path benchmarks — the paths a request actually walks.
//!
//! Run with: `cargo bench --bench hotpath_benchmark --all-features`
//!
//! Each group here exists because something on that path was measured and
//! changed. They are the evidence for those changes, so they deliberately
//! measure *through the public API* rather than the internals: what a caller
//! pays is the only number that matters.
//!
//! To compare a change:
//! ```text
//! git stash && cargo bench --bench hotpath_benchmark --all-features -- --save-baseline before
//! git stash pop && cargo bench --bench hotpath_benchmark --all-features -- --baseline before
//! ```

use std::sync::Arc;
use std::time::Duration;

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};

use rusty_ssr::cache::{BuiltPage, CachePolicy, PageCache, RenderKey, SsrCache};

// ── fragment cache ───────────────────────────────────────────────────────────

/// A cache hit is the whole point of the fragment cache, so its cost is the
/// number to watch. Anything on this path that is not "hash, compare, clone an
/// Arc" is overhead paid by every served request.
fn bench_fragment_cache_lookup(c: &mut Criterion) {
    let mut group = c.benchmark_group("fragment_cache");

    let cache = SsrCache::with_ttl(1000, 300);
    for i in 0..500 {
        cache.insert(&format!("/page/{i}"), Arc::from("<div>content</div>"));
    }

    // Hit in the ultra-hot tier: the last thing inserted is still in the array.
    group.bench_function("hit_hot", |b| {
        let key = "/page/499";
        b.iter(|| black_box(cache.try_get(black_box(key))))
    });

    // Hit that has aged out of this thread's hot tiers into the shared map.
    group.bench_function("hit_cold_then_promote", |b| {
        let mut n = 0u32;
        b.iter(|| {
            // Cycle over more keys than the hot tiers hold, so most lookups
            // reach the cold cache and pay the promotion.
            n = n.wrapping_add(1);
            let key = format!("/page/{}", n % 400);
            black_box(cache.try_get(black_box(&key)))
        })
    });

    group.bench_function("miss", |b| {
        b.iter(|| black_box(cache.try_get(black_box("/nothing/here"))))
    });

    group.finish();
}

/// Inserting into a cache that is already at capacity is the steady state for
/// any cache that matters — a cache with room to spare is one nobody is using.
fn bench_fragment_cache_insert(c: &mut Criterion) {
    let mut group = c.benchmark_group("fragment_cache_insert");
    group.throughput(Throughput::Elements(1));

    // Well under capacity: no eviction, so this is the cost of the insert path
    // itself (including whatever it does to decide it need not evict).
    group.bench_function("below_capacity", |b| {
        let cache = SsrCache::new(100_000);
        let mut n = 0u64;
        b.iter(|| {
            n += 1;
            cache.insert(&format!("/p/{n}"), Arc::from("<div>x</div>"));
        })
    });

    // At capacity: every insert must also decide whether to evict.
    group.bench_function("at_capacity", |b| {
        let cache = SsrCache::new(2000);
        for i in 0..2000 {
            cache.insert(&format!("/warm/{i}"), Arc::from("<div>x</div>"));
        }
        let mut n = 0u64;
        b.iter(|| {
            n += 1;
            cache.insert(&format!("/p/{n}"), Arc::from("<div>x</div>"));
        })
    });

    group.finish();
}

/// The cache is shared by every worker, so the number that decides whether it
/// scales is throughput under concurrent readers — not single-thread latency.
fn bench_fragment_cache_concurrent(c: &mut Criterion) {
    let mut group = c.benchmark_group("fragment_cache_concurrent");
    group.sample_size(20);

    // Enough work per iteration that starting the threads is a rounding error
    // rather than a share of the measurement.
    const OPS: u64 = 200_000;

    for threads in [1usize, 4, 8] {
        group.throughput(Throughput::Elements(OPS));
        group.bench_with_input(
            BenchmarkId::new("read_hits", threads),
            &threads,
            |b, &threads| {
                let cache = Arc::new(SsrCache::new(4000));
                // More distinct keys than any thread's hot tiers hold, so the
                // reads genuinely land in the shared cold cache.
                for i in 0..2000 {
                    cache.insert(&format!("/page/{i}"), Arc::from("<div>content</div>"));
                }

                b.iter(|| {
                    let per_thread = OPS as usize / threads;
                    let handles: Vec<_> = (0..threads)
                        .map(|t| {
                            let cache = Arc::clone(&cache);
                            std::thread::spawn(move || {
                                for i in 0..per_thread {
                                    let key = format!("/page/{}", (t * 977 + i * 13) % 2000);
                                    black_box(cache.try_get(&key));
                                }
                            })
                        })
                        .collect();
                    for h in handles {
                        h.join().unwrap();
                    }
                })
            },
        );
    }

    group.finish();
}

// ── page cache ───────────────────────────────────────────────────────────────

/// The page-cache hit is the cheapest answer the engine can give, so every
/// avoidable allocation on it is visible. A key with variants is the realistic
/// shape: host and locale are what make one URL several documents.
fn bench_page_cache(c: &mut Criterion) {
    let mut group = c.benchmark_group("page_cache");

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    let cache = Arc::new(PageCache::new(CachePolicy::ttl(
        1000,
        Duration::from_secs(300),
    )));
    let key = RenderKey::new("/venda/blumenau")
        .variant("host", "morada.test")
        .variant("locale", "pt-BR")
        .variant("device", "mobile");
    cache.store(&key, BuiltPage::ok("<!doctype html><body>page</body>"));

    // Bare lookup, no build step.
    group.bench_function("get_hit", |b| {
        b.iter(|| black_box(cache.get(black_box(&key))))
    });

    // The method an HTTP handler actually calls on every request.
    group.bench_function("get_or_build_hit", |b| {
        b.to_async(&rt).iter(|| {
            let cache = Arc::clone(&cache);
            let key = key.clone();
            async move {
                black_box(
                    cache
                        .get_or_build(&key, || async {
                            unreachable!("benchmark key is always warm")
                        })
                        .await,
                )
            }
        })
    });

    // A miss pays the build plus the single-flight bookkeeping.
    group.bench_function("get_or_build_miss", |b| {
        let mut n = 0u64;
        b.to_async(&rt).iter(|| {
            n += 1;
            let cache = Arc::clone(&cache);
            let key = RenderKey::new(format!("/cold/{n}")).variant("host", "morada.test");
            async move {
                black_box(
                    cache
                        .get_or_build(&key, || async {
                            Ok(BuiltPage::ok("<!doctype html><body>built</body>"))
                        })
                        .await,
                )
            }
        })
    });

    group.finish();
}

// ── engine: template assembly and rendering ──────────────────────────────────

const TEMPLATE_SHELL: &str = r#"<!doctype html>
<html lang="pt-BR">
<head>
<meta charset="utf-8" />
<title><!--ssr:title--></title>
<!--ssr:head-->
<!--ssr:css-->
<!--seo-->
</head>
<body>
<div id="root"><!--ssr:outlet--></div>
<!--ssr:scripts-->
<script><!--ssr:state--></script>
</body>
</html>
"#;

/// A page is assembled on every uncached render, and the template is the whole
/// document — so the cost here scales with page size and with how many
/// placeholders the caller uses.
fn bench_template_assembly(c: &mut Criterion) {
    use std::io::Write;

    let mut group = c.benchmark_group("template_assembly");

    let dir = tempfile::tempdir().unwrap();
    let template_path = dir.path().join("index.html");
    // Deliberately a big document — ~600 kB of shell.
    //
    // Every measurement here goes through a real render, which costs ~20 µs of
    // cross-thread dispatch to a V8 worker whatever the template is. On a small
    // shell the assembly is a couple of microseconds and sits entirely inside
    // that noise, so the group reported the scheduler rather than the scan. At
    // this size the scan dominates and the number means something.
    let filler = "<meta name=\"x\" content=\"y\" />\n".repeat(20_000);
    let template = TEMPLATE_SHELL.replace("<!--ssr:head-->", &filler);
    std::fs::write(&template_path, &template).unwrap();

    let bundle_path = dir.path().join("bundle.js");
    let mut f = std::fs::File::create(&bundle_path).unwrap();
    writeln!(f, "globalThis.renderPage = (url) => '<main>' + url + '</main>';").unwrap();
    drop(f);

    let engine = rusty_ssr::SsrEngine::builder()
        .bundle_path(&bundle_path)
        .html_template(&template_path)
        .pool_size(1)
        .build_engine()
        .expect("engine");

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    let fragment_small = "<main>/home</main>";
    let fragment_big = format!("<main>{}</main>", "<p>row</p>".repeat(2000));

    for (label, fragment) in [("small", fragment_small), ("large", fragment_big.as_str())] {
        // One placeholder — what render_to_html does.
        group.bench_with_input(
            BenchmarkId::new("one_placeholder", label),
            fragment,
            |b, fragment| {
                b.to_async(&rt).iter(|| async {
                    black_box(
                        engine
                            .render_to_html_uncached_with_replacements("/home", "{}", &[])
                            .await
                            .map(|s| s.len() + fragment.len()),
                    )
                })
            },
        );

        // Five placeholders — what a page with per-request SEO does.
        group.bench_with_input(
            BenchmarkId::new("five_placeholders", label),
            fragment,
            |b, _fragment| {
                b.to_async(&rt).iter(|| async {
                    black_box(
                        engine
                            .render_to_html_uncached_with_replacements(
                                "/home",
                                "{}",
                                &[
                                    ("<!--ssr:title-->", "Listing #42"),
                                    ("<!--seo-->", "<meta property=\"og:title\" content=\"x\" />"),
                                    ("<!--ssr:css-->", "<link rel=\"stylesheet\" href=\"/a.css\" />"),
                                    ("<!--ssr:scripts-->", "<script src=\"/a.js\"></script>"),
                                    ("<!--ssr:state-->", "window.__STATE__={};"),
                                ],
                            )
                            .await
                            .map(|s| s.len()),
                    )
                })
            },
        );
    }

    group.finish();
}

/// Handing the bundle its data is pure overhead — it is work done before the
/// render begins, and it scales with payload size. This is the group that says
/// whether a large JSON envelope is cheap or expensive to deliver.
fn bench_render_payload(c: &mut Criterion) {
    use std::io::Write;

    let mut group = c.benchmark_group("render_payload");
    group.sample_size(30);

    let dir = tempfile::tempdir().unwrap();
    let bundle_path = dir.path().join("bundle.js");
    let mut f = std::fs::File::create(&bundle_path).unwrap();
    // Touches the data so the conversion cannot be optimised away, but does no
    // real rendering work — we are measuring delivery, not the framework.
    writeln!(
        f,
        "globalThis.renderPage = (url, data) => '<main>' + (data && data.rows ? data.rows.length : 0) + '</main>';"
    )
    .unwrap();
    drop(f);

    let engine = rusty_ssr::SsrEngine::builder()
        .bundle_path(&bundle_path)
        .pool_size(1)
        .build_engine()
        .expect("engine");

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    // Empty, small, and the size a real catalogue page actually sends.
    let empty = "{}".to_string();
    let small = serde_json::json!({ "city": "Blumenau", "page": 1, "sort": "relevance" })
        .to_string();
    let rows: Vec<serde_json::Value> = (0..400)
        .map(|i| {
            serde_json::json!({
                "id": i,
                "title": format!("Apartamento {i} — 2 quartos"),
                "price": 450_000 + i * 137,
                "district": "Velha",
                "photos": ["a.jpg", "b.jpg", "c.jpg"],
            })
        })
        .collect();
    let large = serde_json::json!({ "city": "Blumenau", "rows": rows }).to_string();

    for (label, data) in [
        ("empty_2b", empty.as_str()),
        ("small_64b", small.as_str()),
        ("large_60kb", large.as_str()),
    ] {
        group.throughput(Throughput::Bytes(data.len() as u64));
        group.bench_with_input(BenchmarkId::new("json", label), data, |b, data| {
            b.to_async(&rt)
                .iter(|| async { black_box(engine.render_uncached("/catalog", data).await) })
        });
    }

    group.finish();
}

/// Every render crosses the pool's queue, so dispatch cost is paid per request
/// no matter how fast the render itself is. With more concurrent callers than
/// workers, this is where a bad handoff shows up.
fn bench_pool_dispatch(c: &mut Criterion) {
    use std::io::Write;

    let mut group = c.benchmark_group("pool_dispatch");
    group.sample_size(20);

    let dir = tempfile::tempdir().unwrap();
    let bundle_path = dir.path().join("bundle.js");
    let mut f = std::fs::File::create(&bundle_path).unwrap();
    // Deliberately trivial: we want the queue's cost, not V8's.
    writeln!(f, "globalThis.renderPage = () => '<i>x</i>';").unwrap();
    drop(f);

    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(4)
        .enable_all()
        .build()
        .unwrap();

    for workers in [1usize, 4] {
        let engine = Arc::new(
            rusty_ssr::SsrEngine::builder()
                .bundle_path(&bundle_path)
                .pool_size(workers)
                .build_engine()
                .expect("engine"),
        );

        const IN_FLIGHT: usize = 64;
        group.throughput(Throughput::Elements(IN_FLIGHT as u64));
        group.bench_with_input(
            BenchmarkId::new("concurrent_renders", workers),
            &workers,
            |b, _| {
                b.to_async(&rt).iter(|| {
                    let engine = Arc::clone(&engine);
                    async move {
                        let tasks: Vec<_> = (0..IN_FLIGHT)
                            .map(|i| {
                                let engine = Arc::clone(&engine);
                                tokio::spawn(async move {
                                    engine.render_uncached(&format!("/p/{i}"), "{}").await
                                })
                            })
                            .collect();
                        for t in tasks {
                            black_box(t.await.unwrap().ok());
                        }
                    }
                })
            },
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_fragment_cache_lookup,
    bench_fragment_cache_insert,
    bench_fragment_cache_concurrent,
    bench_page_cache,
    bench_template_assembly,
    bench_render_payload,
    bench_pool_dispatch,
);
criterion_main!(benches);
