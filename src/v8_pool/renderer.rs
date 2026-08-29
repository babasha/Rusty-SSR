//! HTML rendering via V8 runtime

use deno_core::v8;

use super::runtime::RuntimeState;

/// What travels to the render function as its second argument.
///
/// Two shapes because they have genuinely different costs. JSON is convenient
/// and readable; bytes exist because the moment a payload is actually binary —
/// a protobuf, a MessagePack frame, an image — encoding it as JSON means base64,
/// and base64 means a third more bytes on the way in plus a decode inside V8
/// that the bundle has to implement itself (the prelude ships no `atob`, on
/// purpose). [`Bytes`](RenderPayload::Bytes) hands the bundle a `Uint8Array`
/// over the same buffer and skips all of it.
#[derive(Debug, Clone)]
pub enum RenderPayload {
    /// A JSON document. Parsed here to prove it is JSON, then converted to a
    /// native V8 value — the bundle receives an object, never a string.
    Json(String),
    /// Raw bytes, delivered as a `Uint8Array`. No encoding, no decoding, and
    /// the buffer is moved into V8 rather than copied.
    Bytes(Vec<u8>),
    /// Both: a small JSON envelope describing the payload, and the payload
    /// itself as bytes. `renderPage(url, data, bytes)`.
    ///
    /// This is the shape most real payloads actually have. A page's data is
    /// usually a few scalars — which query these rows answer, which page of it,
    /// how it was sorted — wrapped around one large binary blob. Forcing that
    /// into a single JSON argument means base64-ing the blob into a string
    /// field, and then the bundle spends real milliseconds decoding it back
    /// with a hand-written `atob` before it can begin rendering.
    ///
    /// Old bundles are unaffected: a `function(url, data)` simply ignores a
    /// third argument.
    JsonWithBytes {
        /// The envelope, delivered as an object.
        json: String,
        /// The payload, delivered as a `Uint8Array`.
        bytes: Vec<u8>,
    },
}

impl Default for RenderPayload {
    fn default() -> Self {
        Self::Json("{}".to_string())
    }
}

impl RenderPayload {
    /// Size in bytes, for logging and for the prefetch hint.
    pub fn len(&self) -> usize {
        match self {
            Self::Json(s) => s.len(),
            Self::Bytes(b) => b.len(),
            Self::JsonWithBytes { json, bytes } => json.len() + bytes.len(),
        }
    }

    /// Whether the payload carries nothing.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub(crate) fn as_ptr(&self) -> *const u8 {
        match self {
            Self::Json(s) => s.as_ptr(),
            Self::Bytes(b) => b.as_ptr(),
            // The bytes are the big half, and the big half is what benefits.
            Self::JsonWithBytes { bytes, .. } => bytes.as_ptr(),
        }
    }
}

/// Call `globalThis.__rustySsrReset()`, the per-request boundary.
///
/// Resolved once per worker and then cached, including the "there isn't one"
/// answer — a bundle loaded with `.polyfills(false)` and no hook of its own is
/// a legitimate configuration, and re-resolving on every render would make it a
/// per-request cost.
///
/// A throw propagates. Serving a request whose isolation failed means serving
/// it with the previous request's state still in place, and the previous
/// request belonged to somebody else.
fn reset_request_state(state: &mut RuntimeState) -> Result<(), String> {
    if state.reset_fn.is_none() {
        let resolved = state
            .runtime
            .execute_script("<resolve-ssr-reset>", "globalThis.__rustySsrReset")
            .map_err(|e| format!("Failed to resolve __rustySsrReset: {}", e))?;
        let scope = &mut state.runtime.handle_scope();
        let local = v8::Local::new(scope, resolved);
        state.reset_fn = Some(
            v8::Local::<v8::Function>::try_from(local)
                .ok()
                .map(|func| v8::Global::new(scope, func)),
        );
    }

    let Some(Some(reset_global)) = state.reset_fn.as_ref() else {
        return Ok(());
    };

    let scope = &mut state.runtime.handle_scope();
    let func = v8::Local::new(scope, reset_global);
    let recv: v8::Local<v8::Value> = v8::undefined(scope).into();
    let tc = &mut v8::TryCatch::new(scope);
    if func.call(tc, recv, &[]).is_none() {
        let msg = tc
            .exception()
            .map(|e| e.to_rust_string_lossy(tc))
            .unwrap_or_else(|| "reset hook threw".to_string());
        return Err(format!("SSR request reset failed: {}", msg));
    }
    Ok(())
}

/// Wrap `bytes` in a `Uint8Array` V8 can read.
///
/// Moved, not copied: the `Vec`'s allocation becomes the array buffer's backing
/// store, so a payload of any size costs one pointer here rather than a memcpy
/// of the whole thing.
fn to_uint8<'s>(
    scope: &mut v8::HandleScope<'s>,
    bytes: Vec<u8>,
) -> Option<v8::Local<'s, v8::Uint8Array>> {
    let len = bytes.len();
    let store = v8::ArrayBuffer::new_backing_store_from_vec(bytes).make_shared();
    let buffer = v8::ArrayBuffer::with_backing_store(scope, &store);
    v8::Uint8Array::new(scope, buffer, 0, len)
}

/// Render HTML via V8 runtime
///
/// Calls `globalThis.{render_function}(url, data)` — plus a third `bytes`
/// argument when the payload carries one — and returns the result.
///
/// The render function is resolved to a `v8::Function` once and cached on
/// `state.render_fn`, and arguments are passed as native V8 values — so there
/// is no per-request script compilation and no string interpolation of the URL
/// or data into JS source.
///
/// # Arguments
/// * `url` - The URL path to render
/// * `payload` - What to hand the render function as its second argument
/// * `render_function` - Name of the global render function
/// * `state` - The thread-local V8 state (runtime + cached function handle)
pub fn render_html(
    url: &str,
    payload: RenderPayload,
    render_function: &str,
    state: &mut RuntimeState,
) -> Result<String, String> {
    // Validate JSON up front (prevents passing junk to the bundle) and build a
    // serde value we convert to a native V8 object below. Bytes need no such
    // check — they are not source and cannot be misread as any.
    let json = match &payload {
        RenderPayload::Json(data) | RenderPayload::JsonWithBytes { json: data, .. } => Some(
            serde_json::from_str::<serde_json::Value>(data)
                .map_err(|e| format!("Invalid JSON data: {}", e))?,
        ),
        RenderPayload::Bytes(_) => None,
    };

    // Draw the request boundary before anything else runs. See the prelude's
    // `__rustySsrReset` for why a pooled isolate needs one.
    reset_request_state(state)?;

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
        let (data_v8, bytes_v8): (v8::Local<v8::Value>, Option<v8::Local<v8::Value>>) =
            match (json, payload) {
                (Some(value), RenderPayload::JsonWithBytes { bytes, .. }) => {
                    let data = serde_v8::to_v8(scope, &value)
                        .map_err(|e| format!("Failed to convert data to V8: {}", e))?;
                    match to_uint8(scope, bytes) {
                        Some(array) => (data, Some(array.into())),
                        None => {
                            return Err("could not build a Uint8Array for the payload".to_string())
                        }
                    }
                }
                (Some(value), _) => (
                    serde_v8::to_v8(scope, &value)
                        .map_err(|e| format!("Failed to convert data to V8: {}", e))?,
                    None,
                ),
                (None, RenderPayload::Bytes(bytes)) => match to_uint8(scope, bytes) {
                    Some(array) => (array.into(), None),
                    None => {
                        return Err("could not build a Uint8Array for the payload".to_string())
                    }
                },
                // Unreachable: `json` is Some for every variant carrying JSON.
                (None, _) => (v8::undefined(scope).into(), None),
            };

        // TryCatch so a synchronous throw surfaces with its message.
        let tc = &mut v8::TryCatch::new(scope);
        let args: Vec<v8::Local<v8::Value>> = match bytes_v8 {
            Some(bytes) => vec![url_v8, data_v8, bytes],
            None => vec![url_v8, data_v8],
        };
        match func.call(tc, recv, &args) {
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

#[cfg(test)]
mod tests {
    use super::*;

    /// `len` feeds the prefetch hint and the logs. For the combined shape it
    /// has to count both halves, or a 90 kB payload with a 200-byte envelope
    /// would be reported as 200 bytes.
    #[test]
    fn a_payload_reports_its_whole_size() {
        assert_eq!(RenderPayload::Json("{}".into()).len(), 2);
        assert_eq!(RenderPayload::Bytes(vec![0; 10]).len(), 10);
        assert_eq!(
            RenderPayload::JsonWithBytes { json: "{}".into(), bytes: vec![0; 10] }.len(),
            12
        );
    }

    /// The default is what the no-data render path sends, and it has to be
    /// valid JSON — the renderer parses it before the bundle sees it.
    #[test]
    fn the_default_payload_is_an_empty_json_object() {
        match RenderPayload::default() {
            RenderPayload::Json(json) => {
                assert_eq!(json, "{}");
                serde_json::from_str::<serde_json::Value>(&json).unwrap();
            }
            other => panic!("unexpected default: {other:?}"),
        }
    }

    #[test]
    fn only_a_zero_length_payload_is_empty() {
        assert!(RenderPayload::Json(String::new()).is_empty());
        assert!(RenderPayload::Bytes(Vec::new()).is_empty());
        assert!(!RenderPayload::Bytes(vec![0]).is_empty());
    }
}
