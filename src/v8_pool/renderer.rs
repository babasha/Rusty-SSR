//! HTML rendering via V8 runtime

use deno_core::{v8, JsRuntime};

/// Render HTML via V8 runtime
///
/// Calls `globalThis.{render_function}(url, data)` and returns the result.
///
/// # Arguments
/// * `url` - The URL path to render
/// * `data` - JSON string with data to pass to the render function
/// * `render_function` - Name of the global render function
/// * `js_runtime` - The V8 runtime to use
pub fn render_html(
    url: &str,
    data: Option<&str>,
    render_function: &str,
    js_runtime: &mut JsRuntime,
) -> Result<String, String> {
    let data = data.unwrap_or("{}");

    // Escape URL for JavaScript string
    let escaped_url = url.replace('\\', "\\\\").replace('"', "\\\"");

    let render_code = format!(
        r#"
        (async function() {{
            try {{
                if (typeof globalThis.{fn} !== 'function') {{
                    throw new Error('Render function globalThis.{fn} not found');
                }}
                return await globalThis.{fn}("{url}", {data});
            }} catch (error) {{
                console.error("Render error:", error);
                return `<html><body><h1>SSR Error</h1><pre>${{error.stack || error.message}}</pre></body></html>`;
            }}
        }})()
        "#,
        fn = render_function,
        url = escaped_url,
        data = data
    );

    // Execute the JS code
    let global = js_runtime
        .execute_script("<render>", render_code)
        .map_err(|e| format!("JS execute error: {}", e))?;

    // Wait for the promise to resolve
    #[allow(deprecated)]
    let resolved = futures::executor::block_on(js_runtime.resolve_value(global))
        .map_err(|e| format!("Promise resolution error: {}", e))?;

    // Deserialize the result
    let scope = &mut js_runtime.handle_scope();
    let local = v8::Local::new(scope, resolved);

    serde_v8::from_v8::<String>(scope, local)
        .map_err(|e| format!("Result deserialization error: {}", e))
}
