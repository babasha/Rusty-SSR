//! Thread-local V8 runtime management

use deno_core::{v8, JsRuntime, RuntimeOptions};
use std::cell::RefCell;
use std::rc::Rc;



/// Per-thread V8 state: the runtime plus a cached handle to the render
/// function so it's resolved once instead of recompiled per request.
pub struct RuntimeState {
    /// The V8 runtime for this worker thread.
    pub runtime: JsRuntime,
    /// Cached `globalThis.<render_function>` handle (resolved on first render).
    pub render_fn: Option<v8::Global<v8::Function>>,
    /// Cached `globalThis.__rustySsrReset` handle — the per-request boundary the
    /// prelude installs.
    ///
    /// Two levels of `Option` on purpose. The outer says whether we have looked
    /// yet; the inner says what we found. A bundle loaded with
    /// `.polyfills(false)` and no hook of its own is a legitimate state, and
    /// without the outer flag we would re-resolve — and re-fail — on every
    /// single render.
    pub reset_fn: Option<Option<v8::Global<v8::Function>>>,
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
pub fn init_runtime(
    bundle_source: &str,
    max_heap_mb: Option<usize>,
    seal_globals: bool,
) -> Result<(), String> {
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

            // The pool's own bundle source, not a process-global one: two
            // engines in one process are two different applications and must be
            // able to render two different bundles.
            js_runtime
                .execute_script("<ssr-bundle>", bundle_source.to_string())
                .map_err(|e| format!("Failed to load SSR bundle: {}", e))?;

            // Record what `globalThis` holds now — after the bundle's top-level
            // code has run, before any request has. Everything added past this
            // point belongs to a request, and `__rustySsrReset` removes it.
            //
            // The timing is the whole trick: sealing inside the prelude would
            // miss every global the bundle itself defines, and sealing on the
            // first render would keep whatever that render happened to add.
            if seal_globals {
                js_runtime
                    .execute_script(
                        "<seal-globals>",
                        "globalThis.__rustySsrSealGlobals && globalThis.__rustySsrSealGlobals()",
                    )
                    .map_err(|e| format!("Failed to seal globals: {}", e))?;
            }

            *slot = Some(RuntimeState {
                runtime: js_runtime,
                render_fn: None,
                reset_fn: None,
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
