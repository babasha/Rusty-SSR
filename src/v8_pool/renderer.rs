//! HTML rendering via V8 runtime

use deno_core::v8;

use super::runtime::RuntimeState;

/// Render HTML via V8 runtime
///
/// Calls `globalThis.{render_function}(url, data)` and returns the result.
///
/// The render function is resolved to a `v8::Function` once and cached on
/// `state.render_fn`, and arguments are passed as native V8 values — so there
/// is no per-request script compilation and no string interpolation of the URL
/// or data into JS source.
///
/// # Arguments
/// * `url` - The URL path to render
/// * `data` - JSON string with data to pass to the render function
/// * `render_function` - Name of the global render function
/// * `state` - The thread-local V8 state (runtime + cached function handle)
pub fn render_html(
    url: &str,
    data: Option<&str>,
    render_function: &str,
    state: &mut RuntimeState,
) -> Result<String, String> {
    let data = data.unwrap_or("{}");

    // Validate data is valid JSON (prevents passing junk to the bundle) and
    // build a serde value we convert to a native V8 object below.
    let value: serde_json::Value =
        serde_json::from_str(data).map_err(|e| format!("Invalid JSON data: {}", e))?;

    // Resolve and cache the render function once per worker. A small one-off
    // script handles dotted names (e.g. "module.renderPage").
    if state.render_fn.is_none() {
        let resolved = state
            .runtime
            .execute_script(
                "<resolve-render-fn>",
                format!("globalThis.{}", render_function),
            )
            .map_err(|e| format!("Failed to resolve render function: {}", e))?;

        let func_global = {
            let scope = &mut state.runtime.handle_scope();
            let local = v8::Local::new(scope, resolved);
            let func = v8::Local::<v8::Function>::try_from(local).map_err(|_| {
                format!("globalThis.{} is not a function", render_function)
            })?;
            v8::Global::new(scope, func)
        };
        state.render_fn = Some(func_global);
    }

    // Call the cached function with (url, data) as native V8 values.
    let promise = {
        let func_global = state.render_fn.as_ref().expect("render_fn set above");
        let scope = &mut state.runtime.handle_scope();

        let func = v8::Local::new(scope, func_global);
        let recv: v8::Local<v8::Value> = v8::undefined(scope).into();
        let url_v8: v8::Local<v8::Value> = match v8::String::new(scope, url) {
            Some(s) => s.into(),
            None => return Err("URL too long for a V8 string".to_string()),
        };
        let data_v8 = serde_v8::to_v8(scope, &value)
            .map_err(|e| format!("Failed to convert data to V8: {}", e))?;

        // TryCatch so a synchronous throw surfaces with its message.
        let tc = &mut v8::TryCatch::new(scope);
        match func.call(tc, recv, &[url_v8, data_v8]) {
            Some(p) => v8::Global::new(tc, p),
            None => {
                let msg = tc
                    .exception()
                    .map(|e| e.to_rust_string_lossy(tc))
                    .unwrap_or_else(|| "render function threw".to_string());
                return Err(format!("JS render error: {}", msg));
            }
        }
    };

    // Drive the (possibly async) result to completion. A rejected promise
    // returns Err here and propagates out uncached.
    #[allow(deprecated)]
    let resolved = futures::executor::block_on(state.runtime.resolve_value(promise))
        .map_err(|e| format!("JS render error: {}", e))?;

    // Deserialize the result string.
    let scope = &mut state.runtime.handle_scope();
    let local = v8::Local::new(scope, resolved);
    serde_v8::from_v8::<String>(scope, local)
        .map_err(|e| format!("Result deserialization error: {}", e))
}
