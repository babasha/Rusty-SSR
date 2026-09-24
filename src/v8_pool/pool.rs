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

/// Buckets in the render-duration histogram: exact below 8 µs, then four steps
/// per octave up to about an hour. See [`bucket_of`].
const HISTOGRAM_BUCKETS: usize = 128;

/// Which bucket a duration in microseconds falls in.
///
/// Four steps per power of two, so a reading is known to about 25% — enough to
/// answer "where is my p99" and to choose a `pool_size`, which is what this is
/// for. Storing a sample per render instead would be exact and would also mean
/// unbounded memory on the one path that must not allocate.
/// Indices 0..8 are exact microseconds; the octave scheme starts above them.
const EXACT_BELOW: usize = 8;

#[inline]
fn bucket_of(micros: u64) -> usize {
    if micros < EXACT_BELOW as u64 {
        return micros as usize;
    }
    // `micros` is in [2^octave, 2^(octave+1)); the top two bits below that
    // choose one of four steps within the octave.
    let octave = 63 - micros.leading_zeros() as usize;
    let sub = ((micros >> (octave - 2)) & 0b11) as usize;
    ((octave - 3) * 4 + sub + EXACT_BELOW).min(HISTOGRAM_BUCKETS - 1)
}

/// The lower bound, in microseconds, of the bucket at `index`.
fn bucket_floor(index: usize) -> u64 {
    if index < EXACT_BELOW {
        return index as u64;
    }
    let octave = (index - EXACT_BELOW) / 4 + 3;
    let sub = ((index - EXACT_BELOW) % 4) as u64;
    (4 + sub) << (octave - 2)
}

/// Everything the pool counts about itself.
///
/// The render path pays two clock reads and a handful of relaxed atomics per
/// render. That is deliberately the opposite of the decision taken for the
/// fragment cache, where the clock came *off* the hot path: a cache lookup costs
/// tens of nanoseconds and timing it doubled the work, while a render costs tens
/// of microseconds at the very least, so the same instrumentation is a tenth of
/// a percent here. Cost is relative to the thing being measured.
struct PoolStats {
    /// Workers currently inside a render.
    busy: AtomicUsize,
    /// Requests handed to the queue and not yet picked up.
    queued: AtomicUsize,
    /// Renders that finished, whatever the outcome.
    renders: AtomicU64,
    /// Renders that came back as an error from JS.
    failed: AtomicU64,
    /// Requests that never produced an answer in time — no worker was free, or
    /// the watchdog killed the render.
    timeouts: AtomicU64,
    /// Render durations, bucketed. See [`bucket_of`].
    histogram: [AtomicU64; HISTOGRAM_BUCKETS],
}

impl Default for PoolStats {
    fn default() -> Self {
        Self {
            busy: AtomicUsize::new(0),
            queued: AtomicUsize::new(0),
            renders: AtomicU64::new(0),
            failed: AtomicU64::new(0),
            timeouts: AtomicU64::new(0),
            histogram: std::array::from_fn(|_| AtomicU64::new(0)),
        }
    }
}

impl PoolStats {
    #[inline]
    fn record(&self, elapsed: Duration, ok: bool) {
        self.renders.fetch_add(1, Ordering::Relaxed);
        if !ok {
            self.failed.fetch_add(1, Ordering::Relaxed);
        }
        let idx = bucket_of(elapsed.as_micros().min(u64::MAX as u128) as u64);
        self.histogram[idx].fetch_add(1, Ordering::Relaxed);
    }

    /// The duration below which `percentile` of renders finished.
    ///
    /// Reported as the floor of the bucket the percentile lands in, so it never
    /// claims more precision than the histogram has.
    fn percentile(&self, counts: &[u64; HISTOGRAM_BUCKETS], total: u64, percentile: f64) -> Duration {
        if total == 0 {
            return Duration::ZERO;
        }
        let want = (total as f64 * percentile / 100.0).ceil() as u64;
        let mut seen = 0u64;
        for (i, n) in counts.iter().enumerate() {
            seen += n;
            if seen >= want {
                return Duration::from_micros(bucket_floor(i));
            }
        }
        Duration::from_micros(bucket_floor(HISTOGRAM_BUCKETS - 1))
    }
}

/// A snapshot of what the pool is doing.
///
/// The two numbers that decide capacity are [`saturation`](Self::saturation) and
/// [`queue_pressure`](Self::queue_pressure). Throughput stops rising once every
/// worker is busy, and everything after that becomes queue delay — so a pool
/// sitting at 100% saturation with a filling queue is one that needs more
/// workers or fewer callers, and no other metric will say so.
#[derive(Clone, Debug, serde::Serialize)]
pub struct PoolMetrics {
    /// Workers that loaded the bundle and are serving.
    pub workers: usize,
    /// Workers inside a render right now.
    pub busy: usize,
    /// Requests waiting for a worker right now.
    pub queued: usize,
    /// What `queue_capacity` was set to.
    pub queue_capacity: usize,
    /// Renders completed since start, whatever the outcome.
    pub renders: u64,
    /// Of those, how many came back as a JS error.
    pub failed: u64,
    /// Requests that timed out waiting for a worker or for their render.
    pub timeouts: u64,
    /// `busy / workers`, as a percentage. At 100 the pool is the bottleneck.
    pub saturation: f64,
    /// `queued / queue_capacity`, as a percentage. Rising while saturation sits
    /// at 100 is the shape of a queue that will not drain.
    pub queue_pressure: f64,
    /// Median render duration.
    pub render_p50: Duration,
    /// 95th percentile render duration.
    pub render_p95: Duration,
    /// 99th percentile render duration.
    pub render_p99: Duration,
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
    response_tx: oneshot::Sender<Result<super::renderer::Rendered, String>>,
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
///     let pool = V8Pool::new(V8PoolConfig::default()).expect("bundle loads");
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
    /// What the pool counts about itself. See [`PoolMetrics`].
    stats: Arc<PoolStats>,
}

impl V8Pool {
    /// Create a new V8 thread pool
    /// Create a new V8 thread pool, waiting for every worker to load the bundle.
    ///
    /// Returns `Err` if any worker could not, and that is the whole point of the
    /// signature. This used to return the pool unconditionally and leave each
    /// worker to log its own failure and exit, so a bundle with a *syntax
    /// error* produced: an engine that built successfully, a pool with zero
    /// workers, and every request hanging for the full `request_timeout` before
    /// failing with "Render timeout". The real message went to `tracing::error!`
    /// and was invisible to anyone without a subscriber installed.
    ///
    /// Waiting also moves isolate creation and bundle compilation out of the
    /// first request, which used to pay for both.
    pub fn new(config: V8PoolConfig) -> Result<Self, String> {
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
        let stats = Arc::new(PoolStats::default());

        let pool = Self {
            queue: Arc::clone(&queue),
            slots: Arc::clone(&slots),
            worker_count: Arc::clone(&worker_count),
            render_fn_shape: Arc::clone(&render_fn_shape),
            watchdog: watchdog.clone(),
            config: config.clone(),
            stats: Arc::clone(&stats),
        };

        // Spawn worker threads. Each reports the outcome of loading the bundle
        // before it starts serving, and this call does not return until they
        // all have.
        let (ready_tx, ready_rx) = std::sync::mpsc::channel::<Result<(), String>>();
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
                ready: ready_tx.clone(),
                stats: Arc::clone(&stats),
            });
        }
        drop(ready_tx);

        for _ in 0..config.num_threads {
            match ready_rx.recv() {
                Ok(Ok(())) => {}
                // A bundle that fails to load fails identically in every
                // worker, so the first message is the message.
                Ok(Err(e)) => return Err(e),
                // The thread went away without reporting — a panic during
                // isolate creation. Nothing else will say so.
                Err(_) => {
                    return Err("a V8 worker died before it finished starting".to_string())
                }
            }
        }

        // Spawn the watchdog thread.
        if let Some(wd) = &watchdog {
            spawn_watchdog(Arc::clone(wd));
        }

        tracing::info!("✅ Started {} V8 workers", config.num_threads);

        Ok(pool)
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
        self.render_collect(url, data).await.map(|r| r.html)
    }

    /// Render, and also return the code-split modules the render used — see
    /// [`Rendered`](super::renderer::Rendered).
    pub async fn render_collect(
        &self,
        url: String,
        data: RenderPayload,
    ) -> Result<super::renderer::Rendered, PoolError> {
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
                    Err(_elapsed) => {
                        // Never even got a queue slot: the pool is the
                        // bottleneck, and this is the counter that says so.
                        self.stats.timeouts.fetch_add(1, Ordering::Relaxed);
                        return Err(PoolError::Timeout);
                    }
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

        self.stats.queued.fetch_add(1, Ordering::Relaxed);
        if self.queue.push(request).is_err() {
            self.stats.queued.fetch_sub(1, Ordering::Relaxed);
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
                    Ok(Ok(Ok(rendered))) => Ok(rendered),
                    Ok(Ok(Err(msg))) => Err(PoolError::Render(msg)),
                    Ok(Err(_)) => Err(PoolError::WorkerCrashed),
                    Err(_elapsed) => {
                        self.stats.timeouts.fetch_add(1, Ordering::Relaxed);
                        Err(PoolError::Timeout)
                    }
                }
            }
            None => match response_rx.await {
                Ok(Ok(rendered)) => Ok(rendered),
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

    /// A snapshot of what the pool is doing right now.
    ///
    /// Throughput stops rising the moment every worker is busy — past that,
    /// added concurrency turns into queue delay and nothing else. So the two
    /// numbers to watch are [`saturation`](PoolMetrics::saturation) and
    /// [`queue_pressure`](PoolMetrics::queue_pressure): a pool pinned at 100%
    /// saturation with a filling queue needs more workers or fewer callers, and
    /// there is no other signal that says which.
    ///
    /// The render percentiles are the other half of that: `pool_size` is chosen
    /// against them, since a pool serves at most `workers / render_time`
    /// requests per second whatever else is true.
    ///
    /// ```rust,no_run
    /// # use rusty_ssr::SsrEngine;
    /// # fn example(engine: &SsrEngine) {
    /// let m = engine.pool_metrics();
    /// if m.saturation > 90.0 && m.queue_pressure > 50.0 {
    ///     tracing::warn!(
    ///         busy = m.busy, workers = m.workers, queued = m.queued,
    ///         p99 = ?m.render_p99, "SSR pool is the bottleneck"
    ///     );
    /// }
    /// # }
    /// ```
    pub fn metrics(&self) -> PoolMetrics {
        let s = &self.stats;
        let counts: [u64; HISTOGRAM_BUCKETS] =
            std::array::from_fn(|i| s.histogram[i].load(Ordering::Relaxed));
        let renders = s.renders.load(Ordering::Relaxed);
        let workers = self.worker_count.load(Ordering::Relaxed);
        let busy = s.busy.load(Ordering::Relaxed);
        let queued = s.queued.load(Ordering::Relaxed);
        let queue_capacity = self.config.queue_capacity;

        PoolMetrics {
            workers,
            busy,
            queued,
            queue_capacity,
            renders,
            failed: s.failed.load(Ordering::Relaxed),
            timeouts: s.timeouts.load(Ordering::Relaxed),
            saturation: if workers > 0 {
                busy as f64 / workers as f64 * 100.0
            } else {
                0.0
            },
            queue_pressure: if queue_capacity > 0 {
                (queued as f64 / queue_capacity as f64 * 100.0).min(100.0)
            } else {
                0.0
            },
            render_p50: s.percentile(&counts, renders, 50.0),
            render_p95: s.percentile(&counts, renders, 95.0),
            render_p99: s.percentile(&counts, renders, 99.0),
        }
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
    /// Reports the outcome of loading the bundle, once, before this worker
    /// starts serving. `V8Pool::new` waits on the other end.
    ready: std::sync::mpsc::Sender<Result<(), String>>,
    stats: Arc<PoolStats>,
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
        ready,
        stats,
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

        // Initialize V8 runtime for this thread (with optional heap cap).
        // Whatever happens, say so: `V8Pool::new` is waiting to hear, and a
        // failure reported only to the log is a failure nobody sees.
        if let Err(e) = runtime::init_runtime(&bundle, max_heap_mb, seal_globals) {
            tracing::error!("❌ Failed to initialize V8 for worker {}: {}", id, e);
            worker_count.fetch_sub(1, Ordering::Relaxed);
            let _ = ready.send(Err(e));
            return;
        }
        let _ = ready.send(Ok(()));

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
            stats.queued.fetch_sub(1, Ordering::Relaxed);

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
                stats.busy.fetch_add(1, Ordering::Relaxed);
                let render_started = Instant::now();
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
                stats.record(render_started.elapsed(), result.is_ok());
                stats.busy.fetch_sub(1, Ordering::Relaxed);

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
            stats: Arc::new(PoolStats::default()),
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

#[cfg(test)]
mod histogram_tests {
    use super::*;

    /// Every bucket must contain the values that map to it. This is the whole
    /// correctness of the histogram: if `bucket_of` and `bucket_floor` disagree,
    /// the percentiles are quietly wrong and nothing else notices.
    #[test]
    fn floors_and_indices_agree() {
        for micros in 0..100_000u64 {
            let idx = bucket_of(micros);
            let floor = bucket_floor(idx);
            assert!(
                floor <= micros,
                "{micros} µs landed in bucket {idx}, whose floor is {floor} µs"
            );
            if idx + 1 < HISTOGRAM_BUCKETS {
                let next = bucket_floor(idx + 1);
                assert!(
                    micros < next,
                    "{micros} µs landed in bucket {idx} but belongs at or above {next} µs"
                );
            }
        }
    }

    #[test]
    fn buckets_never_go_backwards() {
        let mut previous = bucket_of(0);
        for micros in 0..2_000_000u64 {
            let idx = bucket_of(micros);
            assert!(
                idx >= previous,
                "bucket index fell from {previous} to {idx} at {micros} µs"
            );
            previous = idx;
        }
    }

    #[test]
    fn floors_are_strictly_increasing() {
        for i in 1..HISTOGRAM_BUCKETS {
            assert!(
                bucket_floor(i) > bucket_floor(i - 1),
                "floor {} at index {i} does not exceed {} at {}",
                bucket_floor(i),
                bucket_floor(i - 1),
                i - 1
            );
        }
    }

    /// Under 8 µs the histogram is exact, which is what makes a cheap render
    /// distinguishable from a free one.
    #[test]
    fn small_values_are_exact() {
        for micros in 0..8u64 {
            assert_eq!(bucket_of(micros), micros as usize);
            assert_eq!(bucket_floor(micros as usize), micros);
        }
    }

    /// Four steps per octave: a reading is never more than 25% below the truth.
    #[test]
    fn resolution_is_within_a_quarter() {
        for micros in [8u64, 13, 100, 999, 5_000, 250_000, 3_000_000, 60_000_000] {
            let floor = bucket_floor(bucket_of(micros));
            let error = (micros - floor) as f64 / micros as f64;
            assert!(
                error < 0.25,
                "{micros} µs reported as {floor} µs — {:.1}% low",
                error * 100.0
            );
        }
    }

    /// An absurd duration must saturate rather than index out of bounds.
    #[test]
    fn enormous_durations_saturate() {
        assert!(bucket_of(u64::MAX) < HISTOGRAM_BUCKETS);
        assert!(bucket_of(u64::MAX / 2) < HISTOGRAM_BUCKETS);
    }

    #[test]
    fn percentiles_come_out_where_the_samples_are() {
        let stats = PoolStats::default();
        // 99 renders at ~1 ms, one at ~1 s: the shape of a pool with one
        // pathological page.
        for _ in 0..99 {
            stats.record(Duration::from_micros(1000), true);
        }
        stats.record(Duration::from_millis(1000), true);

        let counts: [u64; HISTOGRAM_BUCKETS] =
            std::array::from_fn(|i| stats.histogram[i].load(Ordering::Relaxed));

        let p50 = stats.percentile(&counts, 100, 50.0);
        let p99 = stats.percentile(&counts, 100, 99.0);
        let p100 = stats.percentile(&counts, 100, 100.0);

        assert!(
            p50 >= Duration::from_micros(750) && p50 <= Duration::from_micros(1000),
            "p50 should sit at the 1 ms mass, got {p50:?}"
        );
        assert!(
            p99 <= Duration::from_micros(1000),
            "99 of 100 samples are at 1 ms, so p99 is too: {p99:?}"
        );
        assert!(
            p100 >= Duration::from_millis(750),
            "the outlier must still be reachable at the top, got {p100:?}"
        );
    }

    #[test]
    fn percentiles_of_nothing_are_zero() {
        let stats = PoolStats::default();
        let counts: [u64; HISTOGRAM_BUCKETS] = std::array::from_fn(|_| 0);
        assert_eq!(stats.percentile(&counts, 0, 50.0), Duration::ZERO);
        assert_eq!(stats.percentile(&counts, 0, 99.0), Duration::ZERO);
    }
}
