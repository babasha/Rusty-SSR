use deno_core::{JsRuntime, RuntimeOptions};
use std::cell::RefCell;
use std::rc::Rc;

use super::bundle;

thread_local! {
    static JS_RUNTIME: RefCell<Option<JsRuntime>> = RefCell::new(None);
}

/// Инициализирует V8 runtime в текущем потоке (только 1 раз)
pub fn init_runtime() -> Result<(), String> {
    JS_RUNTIME.with(|runtime| {
        let mut runtime = runtime.borrow_mut();
        if runtime.is_none() {
            let mut js_runtime = JsRuntime::new(RuntimeOptions {
                module_loader: Some(Rc::new(deno_core::FsModuleLoader)),
                ..Default::default()
            });

            // Загружаем кэшированный SSR бандл (zero-copy - используем &'static str)
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

/// Выполняет функцию с доступом к thread-local V8 runtime
pub fn with_runtime<F, R>(f: F) -> R
where
    F: FnOnce(&mut JsRuntime) -> R,
{
    JS_RUNTIME.with(|runtime| {
        let mut runtime = runtime.borrow_mut();
        let js_runtime = runtime.as_mut().expect("Runtime not initialized");
        f(js_runtime)
    })
}
