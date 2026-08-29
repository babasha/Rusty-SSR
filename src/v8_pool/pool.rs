//! V8 Thread Pool implementation

use core_affinity::CoreId;
use deno_core::v8::IsolateHandle;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex, OnceLock, PoisonError};
use std::thread;
use std::time::Duration;
use std::time::Instant;
use tokio::sync::{oneshot, OwnedSemaphorePermit, Semaphore};

use super::renderer::RenderPayload;
use super::{renderer, runtime};

/// The pool's work queue: many producers, many consumers, no lock held while
/// waiting.
///
/// This job used to be done by a `std::sync::mpsc::Receiver` behind a `Mutex`,
/// with the *blocking* `recv()` called while holding that mutex. It worked, but
/// it meant only one worker was ever genuinely waiting on the channel and the
/// other N−1 were parked on the lock behind it — so handing out a task was a
/// mutex hand-off plus a wake-up chain, and the workers took their turns in
/// lock-acquisition order rather than whichever was free.
///
/// Here the lock is held only for a push or a pop. Waiting happens on the
/// condvar, which releases it, so every idle worker is genuinely idle and
/// `notify_one` wakes exactly one of them.
struct WorkQueue {
    inner: Mutex<QueueInner>,
    /// Signalled when a task arrives, and broadcast when the pool closes.
    work: Condvar,
}

struct QueueInner {
    tasks: VecDeque<RenderRequest>,
    /// Set when the pool is dropped. Workers finish the queue and exit.
    closed: bool,
}

impl WorkQueue {
    fn new() -> Self {
        Self {
            inner: Mutex::new(QueueInner {
                tasks: VecDeque::new(),
                closed: false,
            }),
            work: Condvar::new(),
        }
    }

    /// Hand a task to whichever worker wakes first. Gives the task back if the
    /// pool has shut down.
    fn push(&self, task: RenderRequest) -> Result<(), RenderRequest> {
        let mut inner = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
        if inner.closed {
            return Err(task);
        }
        inner.tasks.push_back(task);
        drop(inner);
        self.work.notify_one();
        Ok(())
    }

    /// Wait for a task. `None` means the pool is closed and drained.
    fn pop(&self) -> Option<RenderRequest> {
        let mut inner = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
        loop {
            if let Some(task) = inner.tasks.pop_front() {
                return Some(task);
            }
            if inner.closed {
                return None;
            }
            inner = self
                .work
                .wait(inner)
                .unwrap_or_else(PoisonError::into_inner);
        }
    }

    /// Stop accepting work and wake every worker so they can notice.
    fn close(&self) {
        self.inner
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .closed = true;
        self.work.notify_all();
    }
}

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

    /// The composed bundle source (prelude + user bundle) every worker loads.
    ///
    /// Owned by the pool rather than read from a process-global, so two engines
    /// in one process can render two different applications — and so a test
    /// binary can hold more than one bundle.
    pub bundle: Arc<str>,

    /// Delete globals added since startup before every render. See
    /// [`SsrConfig::seal_globals`](crate::SsrConfig::seal_globals).
    pub seal_globals: bool,
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
            bundle: Arc::from(""),
            seal_globals: false,
        }
    }
}

/// Internal render request
struct RenderRequest {
    url: String,
    data: RenderPayload,
    response_tx: oneshot::Sender<Result<String, String>>,
    /// This task's slot in the bounded queue, released the moment a worker
    /// takes the task. The render function used to travel here too — a `String`
    /// cloned per request to carry a name that is the same for the life of the
    /// pool and is read exactly once per worker, since the resolved handle is
    /// then cached on the isolate.
    _slot: OwnedSemaphorePermit,
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
    queue: Arc<WorkQueue>,
    /// Free slots in the queue. Acquiring one is how a caller waits for room
    /// instead of spinning on a full channel; a worker releases it by taking
    /// the task.
    slots: Arc<Semaphore>,
    worker_count: Arc<AtomicUsize>,
    /// What the render function last returned — a value, or a promise the
    /// engine had to drive. Diagnostic only; see `RenderFnShape`.
    render_fn_shape: Arc<AtomicU8>,
    /// Render watchdog (present when `request_timeout` is set).
    watchdog: Option<Arc<Watchdog>>,
}

impl V8Pool {
    /// Create a new V8 thread pool
    pub fn new(config: V8PoolConfig) -> Self {
        tracing::info!("🔧 Creating V8 pool with {} threads", config.num_threads);

        let queue = Arc::new(WorkQueue::new());
        let slots = Arc::new(Semaphore::new(config.queue_capacity));
        let worker_count = Arc::new(AtomicUsize::new(0));

        let core_affinity: Option<Arc<Vec<CoreId>>> = if config.pin_threads {
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

        let render_fn_shape = Arc::new(AtomicU8::new(0));

        let pool = Self {
            queue: Arc::clone(&queue),
            slots: Arc::clone(&slots),
            worker_count: Arc::clone(&worker_count),
            render_fn_shape: Arc::clone(&render_fn_shape),
            watchdog: watchdog.clone(),
            config: config.clone(),
        };

        // Spawn worker threads
        let render_function: Arc<str> = Arc::from(config.render_function.as_str());
        for i in 0..config.num_threads {
            spawn_worker(WorkerSetup {
                id: i,
                queue: Arc::clone(&queue),
                worker_count: Arc::clone(&worker_count),
                core_affinity: core_affinity.clone(),
                max_heap_mb: config.max_heap_mb,
                bundle: Arc::clone(&config.bundle),
                render_function: Arc::clone(&render_function),
                seal_globals: config.seal_globals,
                render_fn_shape: Arc::clone(&render_fn_shape),
                watchdog: watchdog.clone(),
            });
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

    /// Render a URL to HTML with a JSON payload.
    pub async fn render_with_data(&self, url: String, data: String) -> Result<String, PoolError> {
        self.render_with_payload(url, RenderPayload::Json(data)).await
    }

    /// Render a URL to HTML with raw bytes, delivered as a `Uint8Array`.
    pub async fn render_with_bytes(&self, url: String, data: Vec<u8>) -> Result<String, PoolError> {
        self.render_with_payload(url, RenderPayload::Bytes(data)).await
    }

    /// Render with a JSON envelope and a binary payload beside it:
    /// `renderPage(url, data, bytes)`.
    pub async fn render_with_json_and_bytes(
        &self,
        url: String,
        json: String,
        bytes: Vec<u8>,
    ) -> Result<String, PoolError> {
        self.render_with_payload(url, RenderPayload::JsonWithBytes { json, bytes })
            .await
    }

    /// Render a URL to HTML with whichever payload shape the caller has.
    pub async fn render_with_payload(
        &self,
        url: String,
        data: RenderPayload,
    ) -> Result<String, PoolError> {
        let (response_tx, response_rx) = oneshot::channel();
        let deadline = self.config.request_timeout.map(|t| Instant::now() + t);

        // Wait for room in the queue.
        //
        // This used to be a `try_send`/`yield_now` loop, which under sustained
        // backpressure is a runtime thread spinning at full tilt for the whole
        // timeout — burning exactly the CPU the workers need to drain the
        // queue it is waiting on. Acquiring a permit parks the task instead,
        // and it is woken by the worker that frees the slot.
        let slot = match deadline {
            Some(dl) => {
                let remaining = dl.saturating_duration_since(Instant::now());
                match tokio::time::timeout(remaining, self.slots.clone().acquire_owned()).await {
                    Ok(Ok(permit)) => permit,
                    // The semaphore is only ever closed by shutdown.
                    Ok(Err(_)) => return Err(PoolError::Disconnected),
                    Err(_elapsed) => return Err(PoolError::Timeout),
                }
            }
            None => match self.slots.clone().acquire_owned().await {
                Ok(permit) => permit,
                Err(_) => return Err(PoolError::Disconnected),
            },
        };

        let request = RenderRequest {
            url,
            data,
            response_tx,
            _slot: slot,
        };

        if self.queue.push(request).is_err() {
            return Err(PoolError::Disconnected);
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

    /// What the render function last returned — see
    /// [`RenderFnShape`](super::renderer::RenderFnShape).
    ///
    /// `Unknown` until a render has completed; the value is observed rather
    /// than declared, so there is nothing to read before one has.
    pub fn render_fn_shape(&self) -> super::renderer::RenderFnShape {
        super::renderer::read_shape(&self.render_fn_shape)
    }

    /// Get the number of active workers
    pub fn worker_count(&self) -> usize {
        self.worker_count.load(Ordering::Relaxed)
    }

    /// Get the pool configuration
    pub fn config(&self) -> &V8PoolConfig {
        &self.config
    }
}

impl Drop for V8Pool {
    fn drop(&mut self) {
        tracing::info!("🛑 Shutting down V8 pool");
        // Close the queue so workers drain what is left and exit, and close the
        // semaphore so a caller waiting for room is told the pool is gone
        // rather than waiting for a slot nobody will ever free.
        self.queue.close();
        self.slots.close();
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

/// Everything a worker thread needs, in one place.
///
/// It was ten positional arguments, which is the point at which two `Arc<...>`
/// of the same type next to each other can be swapped without the compiler
/// noticing.
struct WorkerSetup {
    id: usize,
    queue: Arc<WorkQueue>,
    worker_count: Arc<AtomicUsize>,
    core_affinity: Option<Arc<Vec<CoreId>>>,
    max_heap_mb: Option<usize>,
    bundle: Arc<str>,
    render_function: Arc<str>,
    seal_globals: bool,
    render_fn_shape: Arc<AtomicU8>,
    watchdog: Option<Arc<Watchdog>>,
}

/// Spawn a worker thread
fn spawn_worker(setup: WorkerSetup) {
    let WorkerSetup {
        id,
        queue,
        worker_count,
        core_affinity,
        max_heap_mb,
        bundle,
        render_function,
        seal_globals,
        render_fn_shape,
        watchdog,
    } = setup;

    worker_count.fetch_add(1, Ordering::Relaxed);

    thread::spawn(move || {
        tracing::debug!("🟢 V8 worker {} started", id);

        // Pin to CPU core if requested. Keyed on the worker's own index rather
        // than a shared counter it races other workers to increment, so worker
        // N lands on the same core every run.
        if let Some(cores) = core_affinity {
            if let Some(core_id) = cores.get(id % cores.len()) {
                if core_affinity::set_for_current(*core_id) {
                    tracing::debug!("📌 Worker {} pinned to core {:?}", id, core_id.id);
                }
            }
        }

        // Initialize V8 runtime for this thread (with optional heap cap)
        if let Err(e) = runtime::init_runtime(&bundle, max_heap_mb, seal_globals) {
            tracing::error!("❌ Failed to initialize V8 for worker {}: {}", id, e);
            worker_count.fetch_sub(1, Ordering::Relaxed);
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
            let Some(req) = queue.pop() else {
                tracing::debug!("🔴 Worker {} queue closed", id);
                break;
            };

            {
                // Destructured so the payload can be *moved* into the render
                // rather than cloned. A bytes payload becomes V8's backing
                // store directly, and cloning it here would undo exactly the
                // copy that shape exists to avoid.
                let RenderRequest { url, data, response_tx, _slot } = req;

                // The queue slot is free the moment the task leaves the queue —
                // it bounds the queue, not the render. Holding it until the
                // render finished would silently turn `queue_capacity` into a
                // concurrency limit.
                drop(_slot);

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
                            &url,
                            data,
                            &render_function,
                            state,
                            &render_fn_shape,
                        )
                    }))
                    .unwrap_or_else(|_| Err("render panicked".to_string()))
                });

                // Disarm the watchdog.
                if let Some(wd) = &watchdog {
                    wd.slots[id].deadline_nanos.store(0, Ordering::Relaxed);
                }

                // Send response
                let _ = response_tx.send(result);

                requests_processed += 1;
            }
        }

        tracing::debug!(
            "🔴 Worker {} stopped (processed {} requests)",
            id,
            requests_processed
        );

        // Decrement worker count
        worker_count.fetch_sub(1, Ordering::Relaxed);
    });
}

impl V8Pool {
    /// Create a stub pool for testing (no actual V8)
    #[allow(dead_code)]
    pub fn new_stub_with(config: V8PoolConfig) -> Self {
        let slots = Arc::new(Semaphore::new(config.queue_capacity));
        Self {
            config,
            queue: Arc::new(WorkQueue::new()),
            slots,
            worker_count: Arc::new(AtomicUsize::new(0)),
            render_fn_shape: Arc::new(AtomicU8::new(0)),
            watchdog: None,
        }
    }

    /// Create a stub pool with default test config (no workers)
    #[allow(dead_code)]
    pub fn new_stub() -> Self {
        Self::new_stub_with(V8PoolConfig {
            num_threads: 0,
            queue_capacity: 0,
            request_timeout: Some(Duration::from_millis(10)),
            ..Default::default()
        })
    }
}
