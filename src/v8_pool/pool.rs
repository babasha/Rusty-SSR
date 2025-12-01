//! V8 Thread Pool implementation

use core_affinity::CoreId;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use tokio::sync::oneshot;

use super::{renderer, runtime};

/// Configuration for the V8 thread pool
#[derive(Debug, Clone)]
pub struct V8PoolConfig {
    /// Number of worker threads (default: number of CPUs)
    pub num_threads: usize,

    /// Size of the task queue
    pub queue_capacity: usize,

    /// Pin workers to specific CPU cores
    pub pin_threads: bool,

    /// Name of the render function in JS
    pub render_function: String,
}

impl Default for V8PoolConfig {
    fn default() -> Self {
        Self {
            num_threads: num_cpus::get(),
            queue_capacity: 512,
            pin_threads: false,
            render_function: "renderPage".to_string(),
        }
    }
}

/// Internal render request
struct RenderRequest {
    url: String,
    data: String,
    render_function: String,
    response_tx: oneshot::Sender<Result<String, String>>,
}

/// V8 Thread Pool for parallel SSR rendering
///
/// Each worker thread has its own V8 isolate, solving the `!Send + !Sync`
/// problem of V8 runtimes.
///
/// # Example
/// ```rust,ignore
/// use rusty_ssr::v8_pool::{V8Pool, V8PoolConfig};
///
/// #[tokio::main]
/// async fn main() {
///     let pool = V8Pool::new(V8PoolConfig::default());
///     let html = pool.render("https://example.com/page".to_string()).await;
/// }
/// ```
pub struct V8Pool {
    config: V8PoolConfig,
    request_tx: mpsc::SyncSender<RenderRequest>,
    #[allow(dead_code)]
    request_rx: Arc<Mutex<mpsc::Receiver<RenderRequest>>>,
    worker_count: Arc<Mutex<usize>>,
    #[allow(dead_code)]
    core_affinity: Option<Arc<Vec<CoreId>>>,
    #[allow(dead_code)]
    next_core: Arc<AtomicUsize>,
}

impl V8Pool {
    /// Create a new V8 thread pool
    pub fn new(config: V8PoolConfig) -> Self {
        tracing::info!("🔧 Creating V8 pool with {} threads", config.num_threads);

        let (request_tx, request_rx) = mpsc::sync_channel(config.queue_capacity);
        let request_rx = Arc::new(Mutex::new(request_rx));
        let worker_count = Arc::new(Mutex::new(0));

        let core_affinity = if config.pin_threads {
            core_affinity::get_core_ids().map(Arc::new)
        } else {
            None
        };

        let pool = Self {
            config: config.clone(),
            request_tx,
            request_rx: Arc::clone(&request_rx),
            worker_count: Arc::clone(&worker_count),
            core_affinity: core_affinity.clone(),
            next_core: Arc::new(AtomicUsize::new(0)),
        };

        // Spawn worker threads
        for i in 0..config.num_threads {
            spawn_worker(
                i,
                Arc::clone(&request_rx),
                Arc::clone(&worker_count),
                core_affinity.clone(),
                Arc::clone(&pool.next_core),
            );
        }

        tracing::info!("✅ Started {} V8 workers", config.num_threads);

        pool
    }

    /// Render a URL to HTML
    pub async fn render(&self, url: String) -> Result<String, String> {
        self.render_with_data(url, "{}".to_string()).await
    }

    /// Render a URL to HTML with custom data
    pub async fn render_with_data(&self, url: String, data: String) -> Result<String, String> {
        let (response_tx, response_rx) = oneshot::channel();

        self.request_tx
            .send(RenderRequest {
                url,
                data,
                render_function: self.config.render_function.clone(),
                response_tx,
            })
            .map_err(|_| "Failed to send render request: pool may be shutting down".to_string())?;

        response_rx
            .await
            .map_err(|_| "Failed to receive render response: worker may have crashed".to_string())?
    }

    /// Get the number of active workers
    pub fn worker_count(&self) -> usize {
        *self.worker_count.lock().unwrap()
    }

    /// Get the pool configuration
    pub fn config(&self) -> &V8PoolConfig {
        &self.config
    }
}

impl Drop for V8Pool {
    fn drop(&mut self) {
        tracing::info!("🛑 Shutting down V8 pool");
        // Channels will be dropped, workers will receive disconnect and exit
    }
}

/// Spawn a worker thread
fn spawn_worker(
    id: usize,
    request_rx: Arc<Mutex<mpsc::Receiver<RenderRequest>>>,
    worker_count: Arc<Mutex<usize>>,
    core_affinity: Option<Arc<Vec<CoreId>>>,
    next_core: Arc<AtomicUsize>,
) {
    // Increment worker count
    {
        let mut count = worker_count.lock().unwrap();
        *count += 1;
    }

    thread::spawn(move || {
        tracing::debug!("🟢 V8 worker {} started", id);

        // Pin to CPU core if requested
        if let Some(cores) = core_affinity {
            let idx = next_core.fetch_add(1, Ordering::Relaxed) % cores.len();
            if let Some(core_id) = cores.get(idx) {
                if core_affinity::set_for_current(*core_id) {
                    tracing::debug!("📌 Worker {} pinned to core {:?}", id, core_id.id);
                }
            }
        }

        // Initialize V8 runtime for this thread
        if let Err(e) = runtime::init_runtime() {
            tracing::error!("❌ Failed to initialize V8 for worker {}: {}", id, e);
            let mut count = worker_count.lock().unwrap();
            *count -= 1;
            return;
        }

        let mut requests_processed = 0usize;

        // Main worker loop
        loop {
            let request = {
                let rx = request_rx.lock().unwrap();
                match rx.recv() {
                    Ok(req) => Some(req),
                    Err(_) => {
                        tracing::debug!("🔴 Worker {} channel disconnected", id);
                        break;
                    }
                }
            };

            if let Some(req) = request {
                // Prefetch data for better cache performance
                prefetch_data(&req.data);

                // Render via V8
                let result = runtime::with_runtime(|js_runtime| {
                    renderer::render_html(
                        &req.url,
                        Some(&req.data),
                        &req.render_function,
                        js_runtime,
                    )
                });

                // Send response
                let _ = req.response_tx.send(result);

                requests_processed += 1;
            }
        }

        tracing::debug!(
            "🔴 Worker {} stopped (processed {} requests)",
            id,
            requests_processed
        );

        // Decrement worker count
        let mut count = worker_count.lock().unwrap();
        *count -= 1;
    });
}

/// Prefetch data into CPU cache
#[inline]
fn prefetch_data(data: &str) {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        unsafe {
            use core::arch::x86_64::{_mm_prefetch, _MM_HINT_T0};
            _mm_prefetch(data.as_ptr() as *const i8, _MM_HINT_T0);
        }
    }

    #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
    {
        // No-op prefetch for other architectures
        let _ = data.len();
    }
}

#[cfg(test)]
impl V8Pool {
    /// Create a stub pool for testing (no actual V8)
    pub fn new_stub() -> Self {
        let (request_tx, request_rx) = mpsc::sync_channel(0);
        Self {
            config: V8PoolConfig {
                num_threads: 0,
                queue_capacity: 0,
                pin_threads: false,
                render_function: "renderPage".to_string(),
            },
            request_tx,
            request_rx: Arc::new(Mutex::new(request_rx)),
            worker_count: Arc::new(Mutex::new(0)),
            core_affinity: None,
            next_core: Arc::new(AtomicUsize::new(0)),
        }
    }
}
