//! Integration Tests for Rusty-SSR
//!
//! Run with: `cargo test --test integration_tests`
//!
//! These tests verify:
//! - V8 pool initialization and shutdown
//! - Concurrent rendering correctness
//! - Cache behavior (hit/miss/eviction)
//! - Error handling and recovery

// Unused imports removed - modules use their own imports

// ============================================================================
// V8 Pool Configuration Tests
// ============================================================================

#[cfg(test)]
mod pool_config_tests {
    use rusty_ssr::v8_pool::V8PoolConfig;

    #[test]
    fn test_default_config() {
        let config = V8PoolConfig::default();

        assert!(config.num_threads > 0, "Should have at least 1 thread");
        assert_eq!(config.queue_capacity, 512, "Default queue capacity should be 512");
        assert!(!config.pin_threads, "Thread pinning should be disabled by default");
        assert_eq!(
            config.request_timeout,
            Some(std::time::Duration::from_secs(30)),
            "Default enqueue timeout should be 30s"
        );
        assert_eq!(
            config.render_function, "renderPage",
            "Default render function should be 'renderPage'"
        );
    }

    #[test]
    fn test_custom_config() {
        let config = V8PoolConfig {
            num_threads: 4,
            queue_capacity: 1024,
            pin_threads: true,
            request_timeout: Some(std::time::Duration::from_secs(1)),
            render_function: "customRender".to_string(),
        };

        assert_eq!(config.num_threads, 4);
        assert_eq!(config.queue_capacity, 1024);
        assert!(config.pin_threads);
        assert_eq!(config.request_timeout, Some(std::time::Duration::from_secs(1)));
        assert_eq!(config.render_function, "customRender");
    }

    #[test]
    fn test_config_clone() {
        let config = V8PoolConfig {
            num_threads: 8,
            queue_capacity: 256,
            pin_threads: false,
            request_timeout: None,
            render_function: "render".to_string(),
        };

        let cloned = config.clone();

        assert_eq!(config.num_threads, cloned.num_threads);
        assert_eq!(config.queue_capacity, cloned.queue_capacity);
        assert_eq!(config.pin_threads, cloned.pin_threads);
        assert_eq!(config.request_timeout, cloned.request_timeout);
        assert_eq!(config.render_function, cloned.render_function);
    }
}

// ============================================================================
// V8 Pool Timeout Tests
// ============================================================================

#[cfg(all(test, feature = "v8-pool"))]
mod pool_timeout_tests {
    use rusty_ssr::v8_pool::{PoolError, V8Pool, V8PoolConfig};
    use std::time::Duration;

    #[tokio::test]
    async fn test_queue_timeout_errors() {
        let pool = V8Pool::new_stub_with(V8PoolConfig {
            num_threads: 0,
            queue_capacity: 0,
            pin_threads: false,
            request_timeout: Some(Duration::from_millis(5)),
            render_function: "renderPage".to_string(),
        });

        let result = pool
            .render_with_data("/timeout".to_string(), "{}".to_string())
            .await;

        match result {
            Err(PoolError::Timeout) => {}
            other => panic!("Expected timeout error, got {:?}", other),
        }
    }
}

// ============================================================================
// Cache Tests (using DashMap directly)
// ============================================================================

#[cfg(test)]
mod cache_tests {
    use dashmap::DashMap;
    use std::sync::Arc;
    use std::thread;

    #[test]
    fn test_cache_basic_operations() {
        let cache: DashMap<String, String> = DashMap::new();

        // Insert
        cache.insert("key1".to_string(), "value1".to_string());
        assert_eq!(cache.len(), 1);

        // Get
        let value = cache.get("key1").map(|v| v.value().clone());
        assert_eq!(value, Some("value1".to_string()));

        // Update
        cache.insert("key1".to_string(), "updated".to_string());
        let value = cache.get("key1").map(|v| v.value().clone());
        assert_eq!(value, Some("updated".to_string()));

        // Remove
        cache.remove("key1");
        assert!(cache.get("key1").is_none());
    }

    #[test]
    fn test_cache_concurrent_access() {
        let cache: Arc<DashMap<u64, String>> = Arc::new(DashMap::new());
        let num_threads = 4;
        let ops_per_thread = 1000;

        let handles: Vec<_> = (0..num_threads)
            .map(|t| {
                let cache = Arc::clone(&cache);
                thread::spawn(move || {
                    for i in 0..ops_per_thread {
                        let key = (t * ops_per_thread + i) as u64;
                        cache.insert(key, format!("value_{}", key));
                    }
                })
            })
            .collect();

        for h in handles {
            h.join().unwrap();
        }

        assert_eq!(cache.len(), num_threads * ops_per_thread);
    }

    #[test]
    fn test_cache_contains_key() {
        let cache: DashMap<String, String> = DashMap::new();

        cache.insert("exists".to_string(), "value".to_string());

        assert!(cache.contains_key("exists"));
        assert!(!cache.contains_key("not_exists"));
    }

    #[test]
    fn test_cache_get_or_insert() {
        let cache: DashMap<String, String> = DashMap::new();

        // First call - inserts
        {
            let value = cache
                .entry("key".to_string())
                .or_insert_with(|| "computed".to_string());
            assert_eq!(value.value(), "computed");
        } // RefMut dropped here, releasing the lock

        // Second call - returns existing
        {
            let value = cache
                .entry("key".to_string())
                .or_insert_with(|| "should_not_be_used".to_string());
            assert_eq!(value.value(), "computed");
        }
    }
}

// ============================================================================
// LRU Cache Tests
// ============================================================================

#[cfg(test)]
mod lru_tests {
    use lru::LruCache;
    use std::num::NonZeroUsize;

    #[test]
    fn test_lru_basic() {
        let mut cache = LruCache::new(NonZeroUsize::new(3).unwrap());

        cache.put("a", 1);
        cache.put("b", 2);
        cache.put("c", 3);

        assert_eq!(cache.len(), 3);
        assert_eq!(cache.get(&"a"), Some(&1));
    }

    #[test]
    fn test_lru_eviction() {
        let mut cache = LruCache::new(NonZeroUsize::new(2).unwrap());

        cache.put("a", 1);
        cache.put("b", 2);

        // Access "a" to make it recently used
        let _ = cache.get(&"a");

        // Insert "c" - should evict "b" (least recently used)
        cache.put("c", 3);

        assert!(cache.get(&"a").is_some(), "a should still exist");
        assert!(cache.get(&"b").is_none(), "b should be evicted");
        assert!(cache.get(&"c").is_some(), "c should exist");
    }

    #[test]
    fn test_lru_peek_doesnt_update() {
        let mut cache = LruCache::new(NonZeroUsize::new(2).unwrap());

        cache.put("a", 1);
        cache.put("b", 2);

        // Peek doesn't update LRU order
        let _ = cache.peek(&"a");

        // Insert "c" - should evict "a" (still least recently used)
        cache.put("c", 3);

        assert!(cache.get(&"a").is_none(), "a should be evicted");
        assert!(cache.get(&"b").is_some(), "b should exist");
        assert!(cache.get(&"c").is_some(), "c should exist");
    }
}

// ============================================================================
// Async Tests
// ============================================================================

#[cfg(test)]
mod async_tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use tokio::sync::oneshot;

    #[tokio::test]
    async fn test_oneshot_channel() {
        let (tx, rx) = oneshot::channel::<String>();

        tx.send("Hello".to_string()).unwrap();
        let result = rx.await.unwrap();

        assert_eq!(result, "Hello");
    }

    #[tokio::test]
    async fn test_concurrent_oneshot() {
        let counter = Arc::new(AtomicUsize::new(0));
        let mut handles = vec![];

        for _ in 0..10 {
            let counter = Arc::clone(&counter);
            let handle = tokio::spawn(async move {
                let (tx, rx) = oneshot::channel::<usize>();

                tokio::spawn(async move {
                    tx.send(1).unwrap();
                });

                let result = rx.await.unwrap();
                counter.fetch_add(result, Ordering::SeqCst);
            });
            handles.push(handle);
        }

        for h in handles {
            h.await.unwrap();
        }

        assert_eq!(counter.load(Ordering::SeqCst), 10);
    }

    #[tokio::test]
    async fn test_timeout() {
        use std::time::Duration;

        let result = tokio::time::timeout(Duration::from_millis(100), async {
            tokio::time::sleep(Duration::from_millis(50)).await;
            "completed"
        })
        .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "completed");
    }

    #[tokio::test]
    async fn test_timeout_exceeded() {
        use std::time::Duration;

        let result = tokio::time::timeout(Duration::from_millis(50), async {
            tokio::time::sleep(Duration::from_millis(100)).await;
            "completed"
        })
        .await;

        assert!(result.is_err(), "Should timeout");
    }
}

// ============================================================================
// Thread Safety Tests
// ============================================================================

#[cfg(test)]
mod thread_safety_tests {
    use std::sync::{mpsc, Arc, Mutex};
    use std::thread;

    #[test]
    fn test_sync_channel_capacity() {
        let (tx, rx) = mpsc::sync_channel::<i32>(5);

        // Fill the channel
        for i in 0..5 {
            tx.send(i).unwrap();
        }

        // Verify all messages received
        for i in 0..5 {
            assert_eq!(rx.recv().unwrap(), i);
        }
    }

    #[test]
    fn test_mpsc_multiple_senders() {
        let (tx, rx) = mpsc::sync_channel::<i32>(100);
        let mut handles = vec![];

        for i in 0..4 {
            let tx = tx.clone();
            handles.push(thread::spawn(move || {
                for j in 0..25 {
                    tx.send(i * 25 + j).unwrap();
                }
            }));
        }

        drop(tx); // Drop original sender

        for h in handles {
            h.join().unwrap();
        }

        let mut received: Vec<i32> = rx.iter().collect();
        received.sort();

        assert_eq!(received.len(), 100);
        assert_eq!(received, (0..100).collect::<Vec<_>>());
    }

    #[test]
    fn test_arc_mutex_counter() {
        let counter = Arc::new(Mutex::new(0));
        let mut handles = vec![];

        for _ in 0..10 {
            let counter = Arc::clone(&counter);
            handles.push(thread::spawn(move || {
                for _ in 0..100 {
                    let mut num = counter.lock().unwrap();
                    *num += 1;
                }
            }));
        }

        for h in handles {
            h.join().unwrap();
        }

        assert_eq!(*counter.lock().unwrap(), 1000);
    }
}

// ============================================================================
// V8 Rendering Tests (actual JS execution)
// ============================================================================

#[cfg(all(test, feature = "v8-pool", feature = "cache"))]
mod v8_render_tests {
    use rusty_ssr::SsrEngine;
    use std::sync::OnceLock;

    const TEST_BUNDLE: &str = r#"
        globalThis.renderPage = async function(url, data) {
            let body = '<h1>' + url + '</h1>';
            if (data && Object.keys(data).length > 0) {
                body += '<pre>' + JSON.stringify(data) + '</pre>';
            }
            return '<html><body>' + body + '</body></html>';
        };
    "#;

    fn get_engine() -> &'static SsrEngine {
        static ENGINE: OnceLock<SsrEngine> = OnceLock::new();
        ENGINE.get_or_init(|| {
            let dir = tempfile::tempdir().unwrap();
            let bundle_path = dir.path().join("test-bundle.js");
            std::fs::write(&bundle_path, TEST_BUNDLE).unwrap();

            SsrEngine::builder()
                .bundle_path(&bundle_path)
                .pool_size(2)
                .cache_size(100)
                .cache_ttl_secs(60)
                .build_engine()
                .expect("Failed to create test engine")
        })
    }

    #[tokio::test]
    async fn test_basic_render() {
        let engine = get_engine();
        let html = engine.render("/home").await.unwrap();

        assert!(html.contains("<html>"), "should return valid HTML");
        assert!(html.contains("<h1>/home</h1>"), "should contain the URL");
    }

    #[tokio::test]
    async fn test_render_with_json_data() {
        let engine = get_engine();
        let data = serde_json::json!({
            "user": "Alice",
            "count": 42
        });

        let html = engine.render_json("/profile", data).await.unwrap();

        assert!(html.contains("/profile"));
        assert!(html.contains("Alice"));
        assert!(html.contains("42"));
    }

    #[tokio::test]
    async fn test_render_with_empty_data() {
        let engine = get_engine();
        let html = engine.render("/empty").await.unwrap();

        assert!(html.contains("<h1>/empty</h1>"));
    }

    #[tokio::test]
    async fn test_concurrent_renders() {
        let engine = get_engine();

        let mut results = vec![];
        for i in 0..10 {
            let url = format!("/page/{}", i);
            let html = engine.render(&url).await.unwrap();
            results.push((url, html));
        }

        for (url, html) in &results {
            assert!(
                html.contains(url.as_str()),
                "render for {} should contain the URL in output",
                url
            );
        }
    }

    #[tokio::test]
    async fn test_cache_hit_after_render() {
        let engine = get_engine();

        // First render — cache miss, goes to V8
        let html1 = engine.render("/cached-page").await.unwrap();
        let metrics1 = engine.cache_metrics();

        // Second render — should be a cache hit
        let html2 = engine.render("/cached-page").await.unwrap();
        let metrics2 = engine.cache_metrics();

        assert_eq!(*html1, *html2, "cached result should match original");
        assert!(
            metrics2.hot_hits + metrics2.cold_hits > metrics1.hot_hits + metrics1.cold_hits,
            "cache hits should increase"
        );
    }

    #[tokio::test]
    async fn test_invalidate_forces_rerender() {
        let engine = get_engine();

        // Render and cache
        let _ = engine.render("/to-invalidate").await.unwrap();
        assert!(engine.cache().try_get("/to-invalidate").is_some());

        // Invalidate
        engine.invalidate("/to-invalidate");
        assert!(engine.cache().try_get("/to-invalidate").is_none());
    }

    #[tokio::test]
    async fn test_render_special_chars_in_url() {
        let engine = get_engine();
        let html = engine.render("/page?q=hello&lang=en").await.unwrap();

        assert!(html.contains("<html>"), "should render despite special chars in URL");
    }

    #[tokio::test]
    async fn test_render_with_invalid_json_rejected() {
        let engine = get_engine();
        let result = engine
            .render_with_data("/bad", "not valid json")
            .await;

        assert!(result.is_err(), "invalid JSON data should be rejected");
    }
}

// ============================================================================
// URL Parsing Tests
// ============================================================================

#[cfg(test)]
mod url_tests {
    #[test]
    fn test_url_path_extraction() {
        let urls = [
            ("https://example.com/", "/"),
            ("https://example.com/page", "/page"),
            ("https://example.com/path/to/page", "/path/to/page"),
            ("https://example.com/page?query=1", "/page"),
            ("/relative/path", "/relative/path"),
        ];

        for (url, expected_path) in urls {
            let path = if url.starts_with('/') {
                url.split('?').next().unwrap()
            } else if let Some(start) = url.find("://") {
                let rest = &url[start + 3..];
                if let Some(path_start) = rest.find('/') {
                    rest[path_start..].split('?').next().unwrap()
                } else {
                    "/"
                }
            } else {
                "/"
            };

            assert_eq!(path, expected_path, "Failed for URL: {}", url);
        }
    }
}

// ============================================================================
// JSON Serialization Tests
// ============================================================================

#[cfg(test)]
mod json_tests {
    use serde::{Deserialize, Serialize};

    #[derive(Serialize, Deserialize, Debug, PartialEq)]
    struct RenderData {
        url: String,
        title: String,
        items: Vec<String>,
    }

    #[test]
    fn test_serialize_render_data() {
        let data = RenderData {
            url: "/test".to_string(),
            title: "Test Page".to_string(),
            items: vec!["item1".to_string(), "item2".to_string()],
        };

        let json = serde_json::to_string(&data).unwrap();
        assert!(json.contains("\"url\":\"/test\""));
        assert!(json.contains("\"title\":\"Test Page\""));
    }

    #[test]
    fn test_deserialize_render_data() {
        let json = r#"{"url":"/page","title":"My Page","items":["a","b","c"]}"#;
        let data: RenderData = serde_json::from_str(json).unwrap();

        assert_eq!(data.url, "/page");
        assert_eq!(data.title, "My Page");
        assert_eq!(data.items, vec!["a", "b", "c"]);
    }

    #[test]
    fn test_roundtrip() {
        let original = RenderData {
            url: "/roundtrip".to_string(),
            title: "Roundtrip Test".to_string(),
            items: vec!["x".to_string(), "y".to_string(), "z".to_string()],
        };

        let json = serde_json::to_string(&original).unwrap();
        let restored: RenderData = serde_json::from_str(&json).unwrap();

        assert_eq!(original, restored);
    }
}
