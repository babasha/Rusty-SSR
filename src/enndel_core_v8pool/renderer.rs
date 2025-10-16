use deno_core::{v8, JsRuntime};

/// Рендерит HTML через V8 runtime
pub fn render_html(url: &str, products_json: Option<&str>, js_runtime: &mut JsRuntime) -> Result<String, String> {
    let products_data = products_json.unwrap_or("[]");

    let render_code = format!(
        r#"
        (async function() {{
            try {{
                return await globalThis.renderPage("{}", {});
            }} catch (error) {{
                console.error("Render error:", error);
                return `<html><body><h1>SSR Error</h1><pre>${{error.stack}}</pre></body></html>`;
            }}
        }})()
        "#,
        url,
        products_data
    );

    // Выполняем JS код
    let global = js_runtime
        .execute_script("<render>", render_code)
        .map_err(|e| format!("Execute error: {}", e))?;

    // Используем futures::executor вместо создания нового runtime
    #[allow(deprecated)]
    let resolved = futures::executor::block_on(js_runtime.resolve_value(global))
        .map_err(|e| format!("Promise error: {}", e))?;

    // Десериализуем результат
    let scope = &mut js_runtime.handle_scope();
    let local = v8::Local::new(scope, resolved);

    serde_v8::from_v8::<String>(scope, local).map_err(|e| format!("Deserialize error: {}", e))
}
