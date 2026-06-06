//! V8 Thread Pool implementation

use core_affinity::CoreId;
use deno_core::v8::IsolateHandle;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, Mutex, OnceLock};
use std::thread;
use std::time::Duration;
use std::time::Instant;
use tokio::sync::oneshot;

use super::{renderer, runtime};

/// Per-worker termination state for the render watchdog.
struct WorkerWatch {
    /// Deadline of the in-flight render, as nanos since `Watchdog::start`;
    /// 0 means the worker is idle.
    deadline_nanos: AtomicU64,
    /// The worker's isolate handle, set once after V8 init.
    handle: OnceLock<IsolateHandle>,
}

/// Watchdog that terminates renders exceeding `request_timeout`, so a runaway
/// (e.g. a non-allocating `while(true)`) frees its worker instead of wedging it
/// permanently. Only created when `request_timeout` is set.
struct Watchdog {
    start: Instant,
    timeout_nanos: u64,
    slots: Vec<WorkerWatch>,
    shutdown: AtomicBool,
}

impl Watchdog {
    #[inline]
    fn now(&self) -> u64 {
        self.start.elapsed().as_nanos() as u64
    }
}

/// Configuration for the V8 thread pool
#[derive(Debug, Clone)]
pub struct V8PoolConfig {
    /// Number of worker threads (default: number of CPUs)
    pub num_threads: usize,

    /// Size of the task queue
    pub queue_capacity: usize,

    /// Pin workers to specific CPU cores
    pub pin_threads: bool,

    /// Timeout for the whole render request — enqueueing *and* waiting for the
    /// worker's response (None = wait indefinitely).
    pub request_timeout: Option<Duration>,

    /// Name of the render function in JS
    pub render_function: String,

    /// Maximum V8 heap size per isolate, in megabytes (None = unbounded)
    pub max_heap_mb: Option<usize>,
}

impl Default for V8PoolConfig {
    fn default() -> Self {
        Self {
            num_threads: num_cpus::get(),
            queue_capacity: 512,
            pin_threads: false,
            request_timeout: Some(Duration::from_secs(30)),
            render_function: "renderPage".to_string(),
            max_heap_mb: None,
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

/// Errors returned by the V8 pool
#[derive(Debug, Clone)]
pub enum PoolError {
    /// Timed out waiting to enqueue work
    Timeout,
    /// Pool is not accepting new work
    Disconnected,
    /// Worker crashed or dropped the response channel
    WorkerCrashed,
    /// Rendering failed inside V8
    Render(String),
}

impl std::fmt::Display for PoolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PoolError::Timeout => write!(f, "Timed out waiting for a free V8 worker"),
            PoolError::Disconnected => write!(f, "V8 pool is not accepting requests"),
            PoolError::WorkerCrashed => write!(f, "V8 worker crashed while rendering"),
            PoolError::Render(msg) => write!(f, "{}", msg),
        }
    }
}

impl std::error::Error for PoolError {}

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
    /// Render watchdog (present when `request_timeout` is set).
    watchdog: Option<Arc<Watchdog>>,
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

        // Render watchdog — one deadline slot per worker. Only when a timeout
        // is configured (no timeout = renders may run unbounded by request).
        let watchdog = config.request_timeout.map(|t| {
            let mut slots = Vec::with_capacity(config.num_threads);
            for _ in 0..config.num_threads {
                slots.push(WorkerWatch {
                    deadline_nanos: AtomicU64::new(0),
                    handle: OnceLock::new(),
                });
            }
            Arc::new(Watchdog {
                start: Instant::now(),
                timeout_nanos: t.as_nanos() as u64,
                slots,
                shutdown: AtomicBool::new(false),
            })
        });

        let pool = Self {
            config: config.clone(),
            request_tx,
            request_rx: Arc::clone(&request_rx),
            worker_count: Arc::clone(&worker_count),
            core_affinity: core_affinity.clone(),
            next_core: Arc::new(AtomicUsize::new(0)),
            watchdog: watchdog.clone(),
        };

        // Spawn worker threads
        for i in 0..config.num_threads {
            spawn_worker(
                i,
                Arc::clone(&request_rx),
                Arc::clone(&worker_count),
                core_affinity.clone(),
                Arc::clone(&pool.next_core),
                config.max_heap_mb,
                watchdog.clone(),
            );
        }

        // Spawn the watchdog thread.
        if let Some(wd) = &watchdog {
            spawn_watchdog(Arc::clone(wd));
        }

        tracing::info!("✅ Started {} V8 workers", config.num_threads);

        pool
    }

    /// Render a URL to HTML
    pub async fn render(&self, url: String) -> Result<String, PoolError> {
        self.render_with_data(url, "{}".to_string()).await
    }

    /// Render a URL to HTML with custom data
    pub async fn render_with_data(&self, url: String, data: String) -> Result<String, PoolError> {
        let (response_tx, response_rx) = oneshot::channel();

        let request = RenderRequest {
            url,
            data,
            render_function: self.config.render_function.clone(),
            response_tx,
        };

        let deadline = self.config.request_timeout.map(|t| Instant::now() + t);
        let mut req = request;

        loop {
            match self.request_tx.try_send(req) {
                Ok(()) => break,
                Err(mpsc::TrySendError::Full(r)) => {
                    if let Some(dl) = deadline {
                        if Instant::now() >= dl {
                            return Err(PoolError::Timeout);
                        }
                    }
                    req = r;
                    tokio::task::yield_now().await;
                    continue;
                }
                Err(mpsc::TrySendError::Disconnected(_)) => {
                    return Err(PoolError::Disconnected);
                }
            }
        }

        // Wait for the response, bounded by the same deadline that bounded
        // enqueueing. Without this an infinite-loop render (or a request that
        // was queued but never picked up) would hang the caller forever; now it
        // returns `Timeout`. The watchdog thread separately terminates the
        // runaway render at the same deadline so the worker is reclaimed (a
        // pure non-allocating `while(true)` no longer wedges it permanently).
        match deadline {
            Some(dl) => {
                let remaining = dl.saturating_duration_since(Instant::now());
                match tokio::time::timeout(remaining, response_rx).await {
                    Ok(Ok(Ok(html))) => Ok(html),
                    Ok(Ok(Err(msg))) => Err(PoolError::Render(msg)),
                    Ok(Err(_)) => Err(PoolError::WorkerCrashed),
                    Err(_elapsed) => Err(PoolError::Timeout),
                }
            }
            None => match response_rx.await {
                Ok(Ok(html)) => Ok(html),
                Ok(Err(msg)) => Err(PoolError::Render(msg)),
                Err(_) => Err(PoolError::WorkerCrashed),
            },
        }
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
        // Channels will be dropped, workers will receive disconnect and exit.
        // Signal the watchdog thread to stop.
        if let Some(wd) = &self.watchdog {
            wd.shutdown.store(true, Ordering::Relaxed);
        }
    }
}

/// Background thread that terminates renders which exceed `request_timeout`.
fn spawn_watchdog(wd: Arc<Watchdog>) {
    const CHECK_INTERVAL: Duration = Duration::from_millis(50);
    thread::spawn(move || {
        loop {
            thread::sleep(CHECK_INTERVAL);
            if wd.shutdown.load(Ordering::Relaxed) {
                break;
            }
            let now = wd.now();
            for slot in &wd.slots {
                let deadline = slot.deadline_nanos.load(Ordering::Relaxed);
                if deadline != 0 && now >= deadline {
                    if let Some(handle) = slot.handle.get() {
                        // Interrupt the runaway render; it surfaces as an Err and
                        // the worker frees up. Idempotent if already terminating.
                        handle.terminate_execution();
                    }
                }
            }
        }
    });
}

/// Spawn a worker thread
fn spawn_worker(
    id: usize,
    request_rx: Arc<Mutex<mpsc::Receiver<RenderRequest>>>,
    worker_count: Arc<Mutex<usize>>,
    core_affinity: Option<Arc<Vec<CoreId>>>,
    next_core: Arc<AtomicUsize>,
    max_heap_mb: Option<usize>,
    watchdog: Option<Arc<Watchdog>>,
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

        // Initialize V8 runtime for this thread (with optional heap cap)
        if let Err(e) = runtime::init_runtime(max_heap_mb) {
            tracing::error!("❌ Failed to initialize V8 for worker {}: {}", id, e);
            let mut count = worker_count.lock().unwrap();
            *count -= 1;
            return;
        }

        // Register this worker's isolate handle with the watchdog so it can
        // terminate a runaway render from another thread.
        if let Some(wd) = &watchdog {
            let _ = wd.slots[id].handle.set(runtime::isolate_handle());
        }
        let has_watchdog = watchdog.is_some();

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

                // Arm the watchdog for this render's deadline.
                if let Some(wd) = &watchdog {
                    wd.slots[id]
                        .deadline_nanos
                        .store(wd.now() + wd.timeout_nanos, Ordering::Relaxed);
                }

                // Render via V8, catching panics so a single bad render can't
                // kill the worker thread (which would permanently shrink the
                // pool). A caught panic becomes an error response; the worker
                // keeps serving subsequent requests.
                let result = runtime::with_runtime(|state| {
                    // Clear any stray termination flag a watchdog may have set
                    // between renders (race), so it can't abort this fresh one.
                    if has_watchdog {
                        state.runtime.v8_isolate().cancel_terminate_execution();
                    }
                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        renderer::render_html(
                            &req.url,
                            Some(&req.data),
                            &req.render_function,
                            state,
                        )
                    }))
                    .unwrap_or_else(|_| Err("render panicked".to_string()))
                });

                // Disarm the watchdog.
                if let Some(wd) = &watchdog {
                    wd.slots[id].deadline_nanos.store(0, Ordering::Relaxed);
                }

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

impl V8Pool {
    /// Create a stub pool for testing (no actual V8)
    #[allow(dead_code)]
    pub fn new_stub_with(config: V8PoolConfig) -> Self {
        let (request_tx, request_rx) = mpsc::sync_channel(config.queue_capacity);
        Self {
            config,
            request_tx,
            request_rx: Arc::new(Mutex::new(request_rx)),
            worker_count: Arc::new(Mutex::new(0)),
            core_affinity: None,
            next_core: Arc::new(AtomicUsize::new(0)),
            watchdog: None,
        }
    }

    /// Create a stub pool with default test config (no workers)
    #[allow(dead_code)]
    pub fn new_stub() -> Self {
        Self::new_stub_with(V8PoolConfig {
            num_threads: 0,
            queue_capacity: 0,
            pin_threads: false,
            request_timeout: Some(Duration::from_millis(10)),
            render_function: "renderPage".to_string(),
            max_heap_mb: None,
        })
    }
}
