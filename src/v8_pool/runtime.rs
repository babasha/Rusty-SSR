//! Thread-local V8 runtime management

use deno_core::{JsRuntime, RuntimeOptions};
use std::cell::RefCell;
use std::rc::Rc;

use super::bundle;

thread_local! {
    /// Thread-local V8 runtime (each worker thread has its own)
    static JS_RUNTIME: RefCell<Option<JsRuntime>> = const { RefCell::new(None) };
}

/// Initialize the V8 runtime in the current thread
///
/// This should be called once per worker thread.
/// The runtime loads the SSR bundle and is ready to render.
pub fn init_runtime() -> Result<(), String> {
    JS_RUNTIME.with(|runtime| {
        let mut runtime = runtime.borrow_mut();

        if runtime.is_none() {
            let mut js_runtime = JsRuntime::new(RuntimeOptions {
                module_loader: Some(Rc::new(deno_core::FsModuleLoader)),
                ..Default::default()
            });

            // Load the cached SSR bundle (zero-copy - uses &'static str)
            let bundle_code = bundle::get_bundle();

            js_runtime
                .execute_script("<ssr-bundle>", bundle_code)
                .map_err(|e| format!("Failed to load SSR bundle: {}", e))?;

            *runtime = Some(js_runtime);

            tracing::debug!(
                "✅ V8 runtime initialized in thread {:?}",
                std::thread::current().id()
            );
        }

        Ok(())
    })
}

/// Execute a function with access to the thread-local V8 runtime
pub fn with_runtime<F, R>(f: F) -> R
where
    F: FnOnce(&mut JsRuntime) -> R,
{
    JS_RUNTIME.with(|runtime| {
        let mut runtime = runtime.borrow_mut();
        let js_runtime = runtime
            .as_mut()
            .expect("V8 runtime not initialized. Call init_runtime() first.");
        f(js_runtime)
    })
}
