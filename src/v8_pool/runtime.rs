//! Thread-local V8 runtime management

use deno_core::{v8, JsRuntime, RuntimeOptions};
use std::cell::RefCell;
use std::rc::Rc;

use super::bundle;

/// Per-thread V8 state: the runtime plus a cached handle to the render
/// function so it's resolved once instead of recompiled per request.
pub struct RuntimeState {
    /// The V8 runtime for this worker thread.
    pub runtime: JsRuntime,
    /// Cached `globalThis.<render_function>` handle (resolved on first render).
    pub render_fn: Option<v8::Global<v8::Function>>,
}

thread_local! {
    /// Thread-local V8 state (each worker thread has its own)
    static JS_RUNTIME: RefCell<Option<RuntimeState>> = const { RefCell::new(None) };
}

/// Initialize the V8 runtime in the current thread
///
/// This should be called once per worker thread.
/// The runtime loads the SSR bundle and is ready to render.
///
/// `max_heap_mb` caps the isolate's heap. When a render approaches the cap,
/// its execution is terminated (surfacing as an `Err` that is not cached)
/// instead of the whole process aborting on OOM.
pub fn init_runtime(max_heap_mb: Option<usize>) -> Result<(), String> {
    JS_RUNTIME.with(|slot| {
        let mut slot = slot.borrow_mut();

        if slot.is_none() {
            let create_params = max_heap_mb
                .map(|mb| v8::Isolate::create_params().heap_limits(0, mb * 1024 * 1024));

            let mut js_runtime = JsRuntime::new(RuntimeOptions {
                module_loader: Some(Rc::new(deno_core::FsModuleLoader)),
                create_params,
                ..Default::default()
            });

            // Graceful heap-cap enforcement: when V8 nears the limit,
            // terminate the running script (the render returns an error and is
            // not cached) and report a higher limit so V8 doesn't OOM-abort
            // the process before the termination takes effect.
            if max_heap_mb.is_some() {
                let handle = js_runtime.v8_isolate().thread_safe_handle();
                js_runtime.add_near_heap_limit_callback(move |current_limit, _initial| {
                    handle.terminate_execution();
                    current_limit * 2
                });
            }

            // Load the cached SSR bundle (zero-copy - uses &'static str)
            let bundle_code = bundle::get_bundle();

            js_runtime
                .execute_script("<ssr-bundle>", bundle_code)
                .map_err(|e| format!("Failed to load SSR bundle: {}", e))?;

            *slot = Some(RuntimeState {
                runtime: js_runtime,
                render_fn: None,
            });

            tracing::debug!(
                "✅ V8 runtime initialized in thread {:?}",
                std::thread::current().id()
            );
        }

        Ok(())
    })
}

/// Get a thread-safe handle to this thread's isolate.
///
/// Used by the pool watchdog to terminate a runaway render from another thread.
/// Must be called after [`init_runtime`].
pub fn isolate_handle() -> v8::IsolateHandle {
    with_runtime(|state| state.runtime.v8_isolate().thread_safe_handle())
}

/// Execute a function with access to the thread-local V8 state
pub fn with_runtime<F, R>(f: F) -> R
where
    F: FnOnce(&mut RuntimeState) -> R,
{
    JS_RUNTIME.with(|slot| {
        let mut slot = slot.borrow_mut();
        let state = slot
            .as_mut()
            .expect("V8 runtime not initialized. Call init_runtime() first.");
        f(state)
    })
}
