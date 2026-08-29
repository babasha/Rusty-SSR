//! HTML rendering via V8 runtime

use std::sync::atomic::{AtomicU8, Ordering};

use deno_core::v8;

use super::runtime::RuntimeState;

/// What the render function handed back — a string, or a promise of one.
///
/// Both work: the engine awaits a promise before returning, so a bundle may
/// declare `renderPage` sync or `async` and nothing downstream can tell the
/// difference from the result. That symmetry is exactly why this is worth
/// recording somewhere a human can see it.
///
/// A synchronous render function is not a bug, but it *is* a constraint its
/// author may not know they accepted. Every mainstream framework's sync
/// renderer throws when a component suspends, so a code-split route awaiting
/// its chunk cannot render — and the usual accommodation is to swap that route
/// for a placeholder, which serves crawlers an empty body under a
/// correct-looking `<title>` and says nothing. Being able to state "this
/// bundle's render function is synchronous" turns that from something you find
/// by looking at a page into something a deploy script can point at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderFnShape {
    /// No render has completed yet, so nothing has been observed.
    Unknown,
    /// Returned a value directly. Suspense-capable rendering is unavailable to
    /// this bundle.
    Sync,
    /// Returned a promise, which the engine drove to completion.
    Async,
}

impl std::fmt::Display for RenderFnShape {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Unknown => "unknown",
            Self::Sync => "sync",
            Self::Async => "async",
        })
    }
}

/// Record what the render function returned.
///
/// A relaxed store of one byte per render: this is a diagnostic and nothing on
/// the render path reads it. Overwritten each time rather than latched, so a
/// bundle replaced at runtime reports its current shape and not its first.
pub(crate) fn note_shape(slot: &AtomicU8, is_promise: bool) {
    slot.store(if is_promise { 2 } else { 1 }, Ordering::Relaxed);
}

/// Read back what [`note_shape`] last recorded.
pub(crate) fn read_shape(slot: &AtomicU8) -> RenderFnShape {
    match slot.load(Ordering::Relaxed) {
        1 => RenderFnShape::Sync,
        2 => RenderFnShape::Async,
        _ => RenderFnShape::Unknown,
    }
}

/// What travels to the render function as its second argument.
///
/// Two shapes because they have genuinely different costs. JSON is convenient
/// and readable; bytes exist because the moment a payload is actually binary —
/// a protobuf, a MessagePack frame, an image — encoding it as JSON means base64,
/// and base64 means a third more bytes on the way in plus a decode inside V8
/// on every render — real milliseconds for a payload of any size, spent undoing
/// an encoding that existed only to fit the argument list.
/// [`Bytes`](RenderPayload::Bytes) hands the bundle a `Uint8Array` over the same
/// buffer and skips all of it.
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
    /// field, and then the bundle spends real milliseconds decoding it back with
    /// `atob` before it can begin rendering.
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
    /// Size in bytes, for logging.
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
}

/// Call `globalThis.__rustySsrReset(url)`, the per-request boundary.
///
/// The URL goes with it because the boundary is also where `location` is set
/// for the request about to run: the engine is the only thing that knows the
/// URL, and a router reading `location.pathname` is how most applications
/// decide what to render.
///
/// Resolved once per worker and then cached, including the "there isn't one"
/// answer — a bundle loaded with `.polyfills(false)` and no hook of its own is
/// a legitimate configuration, and re-resolving on every render would make it a
/// per-request cost.
///
/// A throw propagates. Serving a request whose isolation failed means serving
/// it with the previous request's state still in place, and the previous
/// request belonged to somebody else.
fn reset_request_state(url: &str, state: &mut RuntimeState) -> Result<(), String> {
    if state.reset_fn.is_none() {
        state.reset_fn = Some(resolve_global_fn(state, "__rustySsrReset")?);
    }

    let Some(Some(reset_global)) = state.reset_fn.as_ref() else {
        return Ok(());
    };

    let scope = &mut state.runtime.handle_scope();
    let func = v8::Local::new(scope, reset_global);
    let recv: v8::Local<v8::Value> = v8::undefined(scope).into();
    let url_v8: v8::Local<v8::Value> = match v8::String::new(scope, url) {
        Some(s) => s.into(),
        None => return Err("URL too long for a V8 string".to_string()),
    };
    let tc = &mut v8::TryCatch::new(scope);
    if func.call(tc, recv, &[url_v8]).is_none() {
        return Err(format!(
            "SSR request reset failed: {}",
            caught_message(tc, "reset hook threw")
        ));
    }
    Ok(())
}

/// Resolve `globalThis.<path>` to a callable handle, or `None` when there is no
/// such global or it is not a function.
///
/// A one-off script rather than a property lookup because `path` may be dotted
/// (`"module.renderPage"`), and it runs once per worker: both callers cache the
/// answer, the negative one included. Written once because the two of them —
/// the render function and the request-boundary hook — were the same six lines
/// of scope juggling with different error text.
fn resolve_global_fn(
    state: &mut RuntimeState,
    path: &str,
) -> Result<Option<v8::Global<v8::Function>>, String> {
    let resolved = state
        .runtime
        .execute_script("<resolve-global-fn>", format!("globalThis.{}", path))
        .map_err(|e| format!("Failed to resolve globalThis.{}: {}", path, e))?;

    let scope = &mut state.runtime.handle_scope();
    let local = v8::Local::new(scope, resolved);
    Ok(v8::Local::<v8::Function>::try_from(local)
        .ok()
        .map(|func| v8::Global::new(scope, func)))
}

/// Whatever a `TryCatch` caught, as a message, falling back to `fallback` when
/// V8 left nothing readable behind.
///
/// Three call sites want exactly this, and each writing its own is how one of
/// them ends up reporting "render failed" with the actual reason discarded.
fn caught_message(tc: &mut v8::TryCatch<'_, v8::HandleScope<'_>>, fallback: &str) -> String {
    tc.exception()
        .map(|e| e.to_rust_string_lossy(tc))
        .unwrap_or_else(|| fallback.to_string())
}

/// Parse a JSON document into a native V8 value, using V8's own parser.
///
/// The old route was `serde_json::from_str` into a `serde_json::Value` and then
/// `serde_v8::to_v8` walking that tree to build the V8 one — the payload parsed
/// twice, once into a Rust tree nobody ever reads, with an allocation per
/// string, array and object along the way. `v8::json::parse` does it in a
/// single pass inside the isolate, straight into the objects the bundle
/// receives.
///
/// It validates in the same pass, which is what the separate `serde_json` parse
/// was also there for: malformed input is a `SyntaxError` caught here, and the
/// bundle never sees it.
fn json_to_v8<'s>(
    tc: &mut v8::TryCatch<'_, v8::HandleScope<'s>>,
    json: &str,
) -> Result<v8::Local<'s, v8::Value>, String> {
    let source = v8::String::new(tc, json)
        .ok_or_else(|| "Invalid JSON data: payload too long for a V8 string".to_string())?;

    v8::json::parse(tc, source)
        .ok_or_else(|| format!("Invalid JSON data: {}", caught_message(tc, "malformed JSON")))
}

/// Wrap `bytes` in a `Uint8Array` V8 can read.
///
/// Moved, not copied: the `Vec`'s allocation becomes the array buffer's backing
/// store, so a payload of any size costs one pointer here rather than a memcpy
/// of the whole thing.
fn to_uint8<'s>(
    scope: &mut v8::HandleScope<'s>,
    bytes: Vec<u8>,
) -> Result<v8::Local<'s, v8::Value>, String> {
    let len = bytes.len();
    let store = v8::ArrayBuffer::new_backing_store_from_vec(bytes).make_shared();
    let buffer = v8::ArrayBuffer::with_backing_store(scope, &store);
    v8::Uint8Array::new(scope, buffer, 0, len)
        .map(|array| array.into())
        .ok_or_else(|| "could not build a Uint8Array for the payload".to_string())
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
/// * `shape` - Diagnostic slot recording whether the render function returned a
///   promise; see [`RenderFnShape`]. Shared across workers, written per render.
pub fn render_html(
    url: &str,
    payload: RenderPayload,
    render_function: &str,
    state: &mut RuntimeState,
    shape: &AtomicU8,
) -> Result<String, String> {
    // Draw the request boundary before anything else runs. See the prelude's
    // `__rustySsrReset` for why a pooled isolate needs one.
    reset_request_state(url, state)?;

    // Resolve and cache the render function once per worker.
    if state.render_fn.is_none() {
        state.render_fn = Some(
            resolve_global_fn(state, render_function)?
                .ok_or_else(|| format!("globalThis.{} is not a function", render_function))?,
        );
    }

    // Call the cached function with (url, data) as native V8 values.
    let promise = {
        let func_global = state.render_fn.as_ref().expect("render_fn set above");
        let scope = &mut state.runtime.handle_scope();
        let func = v8::Local::new(scope, func_global);

        // One `TryCatch` over everything that can throw. It has to be in place
        // before the payload is parsed, not merely before the call, because
        // V8's JSON parser reports a malformed document by throwing.
        let tc = &mut v8::TryCatch::new(scope);

        let recv: v8::Local<v8::Value> = v8::undefined(tc).into();
        let url_v8: v8::Local<v8::Value> = match v8::String::new(tc, url) {
            Some(s) => s.into(),
            None => return Err("URL too long for a V8 string".to_string()),
        };

        let (data_v8, bytes_v8): (v8::Local<v8::Value>, Option<v8::Local<v8::Value>>) =
            match payload {
                RenderPayload::Json(json) => (json_to_v8(tc, &json)?, None),
                RenderPayload::Bytes(bytes) => (to_uint8(tc, bytes)?, None),
                RenderPayload::JsonWithBytes { json, bytes } => {
                    let data = json_to_v8(tc, &json)?;
                    (data, Some(to_uint8(tc, bytes)?))
                }
            };

        // Two shapes, two fixed-size argument lists. Building a `Vec` for this
        // was a heap allocation on every single render to hold two or three
        // pointers.
        let called = match bytes_v8 {
            Some(bytes) => func.call(tc, recv, &[url_v8, data_v8, bytes]),
            None => func.call(tc, recv, &[url_v8, data_v8]),
        };

        match called {
            Some(p) => {
                // Sync or async? Both are supported and neither is visible in
                // the result, which is why it is recorded here — at the one
                // point in the process where the answer exists. See
                // `RenderFnShape` for why anybody cares.
                note_shape(shape, p.is_promise());
                v8::Global::new(tc, p)
            }
            None => {
                return Err(format!(
                    "JS render error: {}",
                    caught_message(tc, "render function threw")
                ));
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
