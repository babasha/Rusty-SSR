//! What the fragment cache promises, pinned down before anything is optimised.
//!
//! The tiers behind [`SsrCache`] are private, and every optimisation applied to
//! them is a change to *how* they keep their promises — the promises themselves
//! must not move. So these tests are written entirely against the public API and
//! deliberately say nothing about hot tiers, shard counts or eviction order:
//! they assert the things a caller can actually depend on, which is exactly the
//! set an optimisation is allowed to preserve and nothing more.
//!
//! The one that matters most is `size_never_drifts`. Bookkeeping that is kept
//! alongside a collection instead of being derived from it is the classic place
//! for a cache to start lying about itself, and a cache that lies about its size
//! evicts at the wrong time — or never.

#![cfg(feature = "cache")]

use std::sync::Arc;
use std::thread;
use std::time::Duration;

use rusty_ssr::cache::SsrCache;

fn html(s: &str) -> Arc<str> {
    Arc::from(s)
}

// ── the basic contract ───────────────────────────────────────────────────────

#[test]
fn stores_and_returns_exactly_what_was_put_in() {
    let cache = SsrCache::new(100);
    cache.insert("/a", html("<p>A</p>"));

    assert_eq!(cache.try_get("/a").as_deref(), Some("<p>A</p>"));
    assert!(cache.try_get("/b").is_none());
}

/// Two lookups of one key must return the same content whichever tier answers.
/// Reading repeatedly is what moves an entry between tiers, so this is the
/// cheapest way to catch a promotion path that loses or swaps content.
#[test]
fn repeated_lookups_agree_with_each_other() {
    let cache = SsrCache::new(1000);
    for i in 0..300 {
        cache.insert(&format!("/p/{i}"), html(&format!("<p>{i}</p>")));
    }

    // Walk the whole set several times: far more keys than any thread-local
    // tier holds, so entries are repeatedly demoted and promoted.
    for _ in 0..4 {
        for i in 0..300 {
            let got = cache.try_get(&format!("/p/{i}"));
            assert_eq!(
                got.as_deref(),
                Some(format!("<p>{i}</p>").as_str()),
                "key /p/{i} came back wrong"
            );
        }
    }
}

/// A 64-bit hash collision must degrade to a miss. It cannot be provoked
/// through the public API, so this stands in for it: distinct keys must never
/// answer for one another, however many of them share a cache.
#[test]
fn distinct_keys_never_answer_for_each_other() {
    let cache = SsrCache::new(5000);
    for i in 0..2000 {
        cache.insert(&format!("/k/{i}"), html(&format!("v{i}")));
    }
    for i in 0..2000 {
        if let Some(got) = cache.try_get(&format!("/k/{i}")) {
            assert_eq!(&*got, &format!("v{i}"), "key /k/{i} returned another key's value");
        }
    }
}

// ── size accounting ──────────────────────────────────────────────────────────

/// The cache must never exceed the capacity it was given. This is the invariant
/// eviction exists to maintain, and the one that breaks first if the size the
/// cache believes in stops matching the size it has.
#[test]
fn size_stays_within_capacity_under_sustained_inserts() {
    let cache = SsrCache::new(500);
    for i in 0..20_000u64 {
        cache.insert(&format!("/p/{i}"), html("x"));
        assert!(
            cache.size() <= 500,
            "cache grew past its cap at insert {i}: size={}",
            cache.size()
        );
    }
}

/// Size must track reality across every operation that changes it — inserts,
/// overwrites, single invalidations, prefix invalidations and clears. An
/// overwrite is the interesting one: it replaces an entry rather than adding
/// one, and counting it as an addition is how a maintained counter starts to
/// drift upward until the cache evicts constantly.
#[test]
fn size_never_drifts() {
    let cache = SsrCache::new(10_000);

    // Fresh inserts count.
    for i in 0..100 {
        cache.insert(&format!("/p/{i}"), html("v1"));
    }
    assert_eq!(cache.size(), 100, "100 distinct inserts");

    // Overwrites do not.
    for i in 0..100 {
        cache.insert(&format!("/p/{i}"), html("v2"));
    }
    assert_eq!(cache.size(), 100, "re-inserting the same 100 keys must not grow the cache");
    assert_eq!(cache.try_get("/p/7").as_deref(), Some("v2"), "overwrite must win");

    // Single invalidation removes exactly one.
    cache.invalidate("/p/0");
    assert_eq!(cache.size(), 99, "one invalidation removes one entry");

    // Invalidating something absent removes nothing.
    cache.invalidate("/p/0");
    cache.invalidate("/nothing/here");
    assert_eq!(cache.size(), 99, "invalidating an absent key must not change the size");

    // Prefix invalidation removes exactly the matches.
    for i in 0..10 {
        cache.insert(&format!("/products/{i}"), html("p"));
    }
    assert_eq!(cache.size(), 109);
    let removed = cache.invalidate_prefix("/products");
    assert_eq!(removed, 10, "prefix invalidation reports what it removed");
    assert_eq!(cache.size(), 99, "and removes exactly that many");

    // Clear resets to nothing.
    cache.clear();
    assert_eq!(cache.size(), 0, "clear empties the cache");

    // And the counter is still usable afterwards.
    cache.insert("/after-clear", html("v"));
    assert_eq!(cache.size(), 1, "size still counts correctly after a clear");
    assert_eq!(cache.try_get("/after-clear").as_deref(), Some("v"));
}

/// Concurrent inserts of the *same* keys must not inflate the size: each key
/// exists once however many threads raced to put it there.
#[test]
fn size_survives_concurrent_inserts_of_the_same_keys() {
    let cache = Arc::new(SsrCache::new(10_000));

    let handles: Vec<_> = (0..8)
        .map(|_| {
            let cache = Arc::clone(&cache);
            thread::spawn(move || {
                for i in 0..200 {
                    cache.insert(&format!("/shared/{i}"), html("v"));
                }
            })
        })
        .collect();
    for h in handles {
        h.join().unwrap();
    }

    assert_eq!(
        cache.size(),
        200,
        "8 threads inserting the same 200 keys must leave 200 entries, not {}",
        cache.size()
    );
}

// ── eviction ─────────────────────────────────────────────────────────────────

/// Eviction must prefer entries nobody is asking for. This is the whole reason
/// the cache tracks access at all, so it has to survive any change to *how*
/// that tracking is done.
#[test]
fn eviction_prefers_entries_nobody_reads() {
    let cache = SsrCache::new(200);

    // Fill with a working set we will keep touching.
    for i in 0..50 {
        cache.insert(&format!("/hot/{i}"), html("hot"));
    }

    // Now churn far more keys through the cache than it can hold, re-reading
    // the working set as we go.
    for i in 0..4000u64 {
        cache.insert(&format!("/churn/{i}"), html("churn"));
        if i % 4 == 0 {
            for j in 0..50 {
                let _ = cache.try_get(&format!("/hot/{j}"));
            }
        }
    }

    let survivors = (0..50)
        .filter(|j| cache.try_get(&format!("/hot/{j}")).is_some())
        .count();

    // Not all 50 need survive — the cache is approximate and the churn is
    // brutal — but a policy that ignored access entirely would leave almost
    // none, which is the regression worth catching.
    assert!(
        survivors >= 20,
        "a repeatedly-read working set should mostly survive churn, only {survivors}/50 did"
    );
}

// ── TTL ──────────────────────────────────────────────────────────────────────

#[test]
fn entries_expire_after_their_ttl() {
    let cache = SsrCache::with_ttl(100, 1);
    cache.insert("/short", html("v"));
    assert!(cache.try_get("/short").is_some(), "fresh entry is served");

    thread::sleep(Duration::from_millis(1100));
    assert!(cache.try_get("/short").is_none(), "expired entry must not be served");
}

#[test]
fn a_zero_ttl_means_no_expiry() {
    let cache = SsrCache::with_ttl(100, 0);
    cache.insert("/forever", html("v"));
    thread::sleep(Duration::from_millis(50));
    assert!(cache.try_get("/forever").is_some());
}

// ── metrics ──────────────────────────────────────────────────────────────────

/// The counters callers actually chart. Hit rate is derived from them, so they
/// have to add up: every lookup is a hit or a miss, and nothing else.
#[test]
fn every_lookup_is_counted_exactly_once() {
    let cache = SsrCache::new(100);
    cache.insert("/a", html("v"));

    for _ in 0..500 {
        let _ = cache.try_get("/a");
    }
    for _ in 0..300 {
        let _ = cache.try_get("/absent");
    }

    let m = cache.metrics();
    assert_eq!(m.lookups, 800, "every try_get is one lookup");
    assert_eq!(m.misses, 300, "every absent key is one miss");
    assert_eq!(
        m.hot_hits + m.cold_hits,
        500,
        "every present key is one hit, in one tier or the other"
    );
    assert_eq!(
        m.lookups,
        m.hot_hits + m.cold_hits + m.misses,
        "lookups must be exactly hits plus misses"
    );
    assert!((m.hit_rate - 62.5).abs() < 0.01, "hit rate is hits/lookups, got {}", m.hit_rate);
    assert_eq!(m.insertions, 1);
}

#[test]
fn metrics_survive_concurrent_traffic() {
    let cache = Arc::new(SsrCache::new(5000));
    for i in 0..100 {
        cache.insert(&format!("/p/{i}"), html("v"));
    }

    let handles: Vec<_> = (0..8)
        .map(|t| {
            let cache = Arc::clone(&cache);
            thread::spawn(move || {
                for i in 0..1000 {
                    // Half the keys exist, half do not.
                    let key = if i % 2 == 0 {
                        format!("/p/{}", (t * 7 + i) % 100)
                    } else {
                        format!("/missing/{i}")
                    };
                    let _ = cache.try_get(&key);
                }
            })
        })
        .collect();
    for h in handles {
        h.join().unwrap();
    }

    let m = cache.metrics();
    assert_eq!(m.lookups, 8000, "no lookup may be lost to a race");
    assert_eq!(
        m.lookups,
        m.hot_hits + m.cold_hits + m.misses,
        "hits plus misses must still account for every lookup under concurrency"
    );
    assert_eq!(m.misses, 4000, "exactly half the lookups were for absent keys");
}

#[test]
fn clear_resets_the_counters() {
    let cache = SsrCache::new(100);
    cache.insert("/a", html("v"));
    let _ = cache.try_get("/a");

    cache.clear();

    let m = cache.metrics();
    assert_eq!(m.insertions, 0);
    assert_eq!(m.hot_hits + m.cold_hits, 0);
    assert_eq!(m.cold_size, 0);
}

/// Capacity is reported as configured, whatever the cache is currently holding.
#[test]
fn capacity_is_what_was_asked_for() {
    let cache = SsrCache::new(777);
    assert_eq!(cache.metrics().cold_capacity, 777);
    cache.insert("/a", html("v"));
    assert_eq!(cache.metrics().cold_size, 1);
}

// ── invalidation reaches every tier ──────────────────────────────────────────

/// An invalidated entry must be gone for the thread that cached it too, not
/// just for everyone else. Reading it first is what puts it in this thread's
/// hot tier, which is the copy an invalidation is most likely to miss.
#[test]
fn invalidation_reaches_the_reading_thread() {
    let cache = SsrCache::new(100);
    cache.insert("/a", html("v1"));
    assert!(cache.try_get("/a").is_some(), "warm the reading thread's tier");

    cache.invalidate("/a");
    assert!(cache.try_get("/a").is_none(), "invalidated entry must not be served from any tier");
}

#[test]
fn prefix_invalidation_reaches_the_reading_thread() {
    let cache = SsrCache::new(100);
    cache.insert("/products/1", html("p1"));
    cache.insert("/products/2", html("p2"));
    cache.insert("/about", html("a"));
    for k in ["/products/1", "/products/2", "/about"] {
        assert!(cache.try_get(k).is_some());
    }

    assert_eq!(cache.invalidate_prefix("/products"), 2);

    assert!(cache.try_get("/products/1").is_none());
    assert!(cache.try_get("/products/2").is_none());
    assert!(cache.try_get("/about").is_some(), "a non-matching key must survive");
}

/// Invalidation must not blind other threads to entries it did not remove.
#[test]
fn invalidation_leaves_other_entries_readable_everywhere() {
    let cache = Arc::new(SsrCache::new(1000));
    for i in 0..50 {
        cache.insert(&format!("/keep/{i}"), html("v"));
    }
    cache.insert("/drop", html("v"));

    // Warm several threads' local tiers.
    let handles: Vec<_> = (0..4)
        .map(|_| {
            let cache = Arc::clone(&cache);
            thread::spawn(move || {
                for i in 0..50 {
                    assert!(cache.try_get(&format!("/keep/{i}")).is_some());
                }
            })
        })
        .collect();
    for h in handles {
        h.join().unwrap();
    }

    cache.invalidate("/drop");

    let handles: Vec<_> = (0..4)
        .map(|_| {
            let cache = Arc::clone(&cache);
            thread::spawn(move || {
                assert!(cache.try_get("/drop").is_none(), "invalidated key must be gone");
                for i in 0..50 {
                    assert!(
                        cache.try_get(&format!("/keep/{i}")).is_some(),
                        "/keep/{i} must still be readable after an unrelated invalidation"
                    );
                }
            })
        })
        .collect();
    for h in handles {
        h.join().unwrap();
    }
}
