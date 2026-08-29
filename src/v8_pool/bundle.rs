//! SSR Bundle loader

use std::path::Path;

use crate::error::{SsrError, SsrResult};

/// Browser polyfills for V8 compatibility — the exact source prepended to your
/// bundle.
///
/// These mock browser APIs that don't exist in V8 isolates. Every global is
/// defined **defensively** (only when absent), so a user bundle that ships its
/// own implementation is never clobbered. The whole block can be skipped with
/// `.polyfills(false)` on the engine builder.
///
/// It is `pub` because testing an SSR bundle means running it in *this*
/// environment, and the alternative is what projects end up doing instead:
/// re-typing the prelude into a Node `vm` sandbox by hand. That copy drifts —
/// silently, because a drifted probe still passes — and the first you hear of
/// the drift is a blank page in production. Write the real thing out instead:
///
/// ```rust,no_run
/// std::fs::write("prelude.js", rusty_ssr::v8_pool::BROWSER_POLYFILLS).unwrap();
/// ```
///
/// Two things it deliberately does **not** define, so a bundle can
/// feature-detect rather than receive a wrong implementation:
/// `TextEncoder`/`TextDecoder`, and `atob`/`btoa`. If you are reaching for
/// base64 to move bytes into a render, prefer
/// [`render_with_bytes`](crate::SsrEngine::render_with_bytes) — it hands the
/// bundle a `Uint8Array` and there is nothing to decode.
pub const BROWSER_POLYFILLS: &str = r#"
// =========================================
// Rusty-SSR Browser Polyfills (non-clobbering)
// =========================================

// SSR detection flag
globalThis.__SSR__ = true;

// Basic globals
if (typeof globalThis.window === 'undefined') globalThis.window = globalThis;
if (typeof globalThis.self === 'undefined') globalThis.self = globalThis;

// --- Platform stubs (Tauri, Electron, Capacitor) ---
// Tauri IPC — prevents crash when @tauri-apps/api is bundled
if (typeof globalThis.__TAURI_IPC__ === 'undefined') globalThis.__TAURI_IPC__ = function() {};
if (typeof globalThis.__TAURI_INTERNALS__ === 'undefined') globalThis.__TAURI_INTERNALS__ = {
    invoke: function() { return Promise.reject(new Error('Tauri IPC not available in SSR')); },
    transformCallback: function() { return 0; },
    convertFileSrc: function(s) { return s; },
    metadata: { currentWindow: { label: 'main' }, currentWebview: { label: 'main' } }
};

// Electron stubs
if (!globalThis.process) {
    globalThis.process = { env: { NODE_ENV: 'production' }, platform: 'linux', versions: {} };
}

// Capacitor stub
if (typeof globalThis.Capacitor === 'undefined') {
    globalThis.Capacitor = { isNativePlatform: function() { return false; } };
}

// Minimal timers. SSR has no real event loop for user timers, but callbacks
// are deferred to a microtask rather than run synchronously inline: this keeps
// `await new Promise(r => setTimeout(r))` working (microtasks drain when the
// render promise resolves) while removing the re-entrancy hazard of executing
// the callback in the middle of the caller's stack. Delays are ignored and
// intervals fire at most once. clearTimeout/clearInterval cancel a pending id.
if (typeof globalThis.setTimeout === 'undefined') {
    let __rustyTimerId = 0;
    const __rustyCleared = new Set();
    const __rustySchedule = (cb, args) => {
        const id = ++__rustyTimerId;
        if (typeof cb === 'function') {
            Promise.resolve().then(() => { if (!__rustyCleared.has(id)) cb(...args); });
        }
        return id;
    };
    globalThis.setTimeout = (cb, _ms, ...args) => __rustySchedule(cb, args);
    globalThis.setInterval = (cb, _ms, ...args) => __rustySchedule(cb, args);
    globalThis.clearTimeout = (id) => { __rustyCleared.add(id); };
    globalThis.clearInterval = (id) => { __rustyCleared.add(id); };
}

// Document mock
if (typeof globalThis.document === 'undefined') globalThis.document = {
    createElement: function(tag) {
        var el = {
            tagName: tag.toUpperCase(),
            style: {},
            className: '',
            id: '',
            innerHTML: '',
            textContent: '',
            children: [],
            childNodes: [],
            parentNode: null,
            dataset: {},
            // Attributes
            setAttribute: function(k, v) { el[k] = v; },
            getAttribute: function(k) { return el[k] !== undefined ? String(el[k]) : null; },
            removeAttribute: function() {},
            hasAttribute: function(k) { return el[k] !== undefined; },
            // DOM tree
            appendChild: function(child) { el.children.push(child); child.parentNode = el; return child; },
            removeChild: function(child) { return child; },
            insertBefore: function(child) { el.children.push(child); return child; },
            replaceChild: function(n) { return n; },
            cloneNode: function() { return globalThis.document.createElement(tag); },
            // Events
            addEventListener: function() {},
            removeEventListener: function() {},
            dispatchEvent: function() { return true; },
            // Class list
            classList: {
                _c: [],
                add: function() {},
                remove: function() {},
                toggle: function() {},
                contains: function() { return false; },
                replace: function() {}
            },
            // CSS — for <style> elements and CSS-in-JS
            sheet: (tag === 'style') ? {
                cssRules: [],
                insertRule: function() { return 0; },
                deleteRule: function() {},
                replaceSync: function() {}
            } : undefined,
            // Link/script attributes
            rel: '', href: '', src: '', type: '', media: '', crossOrigin: '',
            onload: null, onerror: null,
            // Dimensions (always zero in SSR)
            offsetWidth: 0, offsetHeight: 0,
            getBoundingClientRect: function() {
                return { top: 0, left: 0, right: 0, bottom: 0, width: 0, height: 0 };
            }
        };
        // Simulate async load for link/script elements
        if (tag === 'link' || tag === 'script') {
            setTimeout(function() { if (el.onload) el.onload(); }, 0);
        }
        return el;
    },
    createTextNode: function(text) { return { textContent: text, nodeType: 3 }; },
    createDocumentFragment: function() {
        return { children: [], appendChild: function(c) { this.children.push(c); return c; } };
    },
    createComment: function(text) { return { textContent: text, nodeType: 8 }; },
    getElementById: function() { return null; },
    querySelector: function() { return null; },
    querySelectorAll: function() { return []; },
    getElementsByTagName: function() { return []; },
    getElementsByClassName: function() { return []; },
    addEventListener: function() {},
    removeEventListener: function() {},
    createEvent: function() {
        return { initEvent: function() {} };
    },
    documentElement: {
        style: {},
        setAttribute: function() {},
        getAttribute: function() { return null; },
        classList: { add: function(){}, remove: function(){}, contains: function(){ return false; } }
    },
    head: {
        appendChild: function(c) { return c; },
        insertBefore: function(c) { return c; },
        querySelector: function() { return null; },
        querySelectorAll: function() { return []; }
    },
    body: {
        appendChild: function(c) { return c; },
        insertBefore: function(c) { return c; },
        querySelector: function() { return null; },
        querySelectorAll: function() { return []; }
    },
    cookie: '',
    readyState: 'complete',
    title: ''
};

// Navigator mock
if (typeof globalThis.navigator === 'undefined') globalThis.navigator = {
    userAgent: 'Rusty-SSR/1.0',
    language: 'en-US',
    languages: ['en-US', 'en'],
    platform: 'Linux',
    onLine: true
};

// Location mock
if (typeof globalThis.location === 'undefined') globalThis.location = {
    href: 'http://localhost/',
    origin: 'http://localhost',
    protocol: 'http:',
    host: 'localhost',
    hostname: 'localhost',
    port: '',
    pathname: '/',
    search: '',
    hash: ''
};

// Animation frame mocks
if (typeof globalThis.requestAnimationFrame === 'undefined') {
    globalThis.requestAnimationFrame = (cb) => setTimeout(cb, 16);
    globalThis.cancelAnimationFrame = (id) => clearTimeout(id);
}

// Performance mock
if (typeof globalThis.performance === 'undefined') globalThis.performance = {
    now: () => Date.now(),
    mark: () => {},
    measure: () => {},
    getEntriesByName: () => [],
    getEntriesByType: () => []
};

// Storage mock. NOTE this is real, working, in-memory storage — not a stub that
// forgets. That matters because the isolate is pooled: without the reset below,
// what one render writes here is readable by the next request on the same
// worker, which is a different visitor.
const createStorage = () => {
    const data = {};
    return {
        getItem: (key) => data[key] ?? null,
        setItem: (key, value) => { data[key] = String(value); },
        removeItem: (key) => { delete data[key]; },
        clear: () => { for (const k in data) delete data[k]; },
        get length() { return Object.keys(data).length; },
        key: (i) => Object.keys(data)[i] ?? null
    };
};
// Tracked by IDENTITY, not by a flag set here. The prelude runs before the
// bundle, so "was it undefined a moment ago" answers the wrong question: a
// bundle is perfectly entitled to install its own storage afterwards, and the
// reset below must not then throw that away. Comparing the object instead means
// we only ever replace the one we made.
let __rustyLocalStorage = null;
let __rustySessionStorage = null;
if (typeof globalThis.localStorage === 'undefined') {
    __rustyLocalStorage = createStorage();
    globalThis.localStorage = __rustyLocalStorage;
}
if (typeof globalThis.sessionStorage === 'undefined') {
    __rustySessionStorage = createStorage();
    globalThis.sessionStorage = __rustySessionStorage;
}

// Fetch mock (minimal - throws if actually used)
if (typeof globalThis.fetch === 'undefined') globalThis.fetch = async () => {
    throw new Error('fetch() is not available in SSR. Use data prop instead.');
};

// MutationObserver mock
if (typeof globalThis.MutationObserver === 'undefined') globalThis.MutationObserver = class MutationObserver {
    constructor() {}
    observe() {}
    disconnect() {}
    takeRecords() { return []; }
};

// ResizeObserver mock
if (typeof globalThis.ResizeObserver === 'undefined') globalThis.ResizeObserver = class ResizeObserver {
    constructor() {}
    observe() {}
    unobserve() {}
    disconnect() {}
};

// IntersectionObserver mock
if (typeof globalThis.IntersectionObserver === 'undefined') globalThis.IntersectionObserver = class IntersectionObserver {
    constructor() {}
    observe() {}
    unobserve() {}
    disconnect() {}
};

// matchMedia mock
if (typeof globalThis.matchMedia === 'undefined') globalThis.matchMedia = (query) => ({
    matches: false,
    media: query,
    onchange: null,
    addListener: () => {},
    removeListener: () => {},
    addEventListener: () => {},
    removeEventListener: () => {},
    dispatchEvent: () => false
});

// Image mock
if (typeof globalThis.Image === 'undefined') globalThis.Image = class Image {
    constructor() {
        this.src = '';
        this.onload = null;
        this.onerror = null;
    }
};

// Console (ensure it exists)
globalThis.console = globalThis.console || {
    log: () => {},
    warn: () => {},
    error: () => {},
    info: () => {},
    debug: () => {}
};

// --- WHATWG URL / URLSearchParams (minimal) ---
// Base deno_core ships no web platform APIs, so routing code that does
// `new URL(...)` / `new URLSearchParams(location.search)` would throw.
// These are small but spec-shaped implementations sufficient for SSR
// routing decisions. (TextEncoder/TextDecoder are intentionally NOT
// shimmed — a wrong UTF-8 impl is worse than an absent one; bundles can
// feature-detect.)
if (typeof globalThis.URLSearchParams === 'undefined') {
    globalThis.URLSearchParams = class URLSearchParams {
        constructor(init) {
            this._p = new Map();
            if (typeof init !== 'string' || init.length === 0) return;
            const raw = init.charAt(0) === '?' ? init.slice(1) : init;
            for (const pair of raw.split('&')) {
                if (!pair) continue;
                const eq = pair.indexOf('=');
                const rk = eq === -1 ? pair : pair.slice(0, eq);
                const rv = eq === -1 ? '' : pair.slice(eq + 1);
                let k, v;
                try { k = decodeURIComponent(rk.replace(/\+/g, ' ')); } catch (_e) { k = rk; }
                try { v = decodeURIComponent(rv.replace(/\+/g, ' ')); } catch (_e) { v = rv; }
                const list = this._p.get(k);
                if (list) list.push(v); else this._p.set(k, [v]);
            }
        }
        get(k) { const l = this._p.get(k); return l && l.length ? l[0] : null; }
        getAll(k) { const l = this._p.get(k); return l ? l.slice() : []; }
        has(k) { return this._p.has(k); }
        set(k, v) { this._p.set(k, [String(v)]); }
        append(k, v) { const l = this._p.get(k); if (l) l.push(String(v)); else this._p.set(k, [String(v)]); }
        delete(k) { this._p.delete(k); }
        forEach(cb, thisArg) { for (const [k, vs] of this._p) for (const v of vs) cb.call(thisArg, v, k, this); }
        toString() {
            const parts = [];
            for (const [k, vs] of this._p) for (const v of vs) parts.push(encodeURIComponent(k) + '=' + encodeURIComponent(v));
            return parts.join('&');
        }
        *[Symbol.iterator]() { for (const [k, vs] of this._p) for (const v of vs) yield [k, v]; }
        keys() { const out = []; for (const [k, vs] of this._p) for (const _v of vs) out.push(k); return out[Symbol.iterator](); }
        values() { const out = []; for (const [_k, vs] of this._p) for (const v of vs) out.push(v); return out[Symbol.iterator](); }
    };
}
if (typeof globalThis.URL === 'undefined') {
    globalThis.URL = class URL {
        constructor(url, base) {
            let href = String(url);
            if (base && !/^[a-zA-Z][a-zA-Z0-9+.-]*:/.test(href)) {
                const b = String(base).replace(/[?#].*$/, '').replace(/\/+$/, '');
                href = b + (href.charAt(0) === '/' ? href : '/' + href);
            }
            this.href = href;
            const m = /^([^:/?#]+:)?(?:\/\/([^/?#]*))?([^?#]*)(\?[^#]*)?(#.*)?$/.exec(href) || [];
            this.protocol = m[1] || '';
            this.host = m[2] || '';
            this.hostname = (m[2] || '').replace(/:\d+$/, '');
            this.port = ((m[2] || '').match(/:(\d+)$/) || [])[1] || '';
            this.pathname = m[3] || '/';
            this.search = m[4] || '';
            this.hash = m[5] || '';
            this.origin = (this.protocol && this.host) ? (this.protocol + '//' + this.host) : '';
            this.searchParams = new globalThis.URLSearchParams(this.search);
        }
        toString() { return this.href; }
    };
}

// queueMicrotask — drives microtask-scheduled callbacks via a resolved promise.
if (typeof globalThis.queueMicrotask === 'undefined') {
    globalThis.queueMicrotask = (cb) => { Promise.resolve().then(cb); };
}

// --- The request boundary ---
//
// The isolate is POOLED. `globalThis` outlives a render, so without this the
// next request handled by this worker starts inside the previous request's
// leftovers. Two distinct problems, and the second is the serious one:
//
//   - correctness: a module-level cache in the bundle answers request B with
//     request A's data, which looks exactly like a caching bug and is nearly
//     impossible to reproduce, because it depends on which worker picked you up;
//   - privacy: A and B are different people. Anything the render stashed —
//     a session, a profile, a cart — is readable by the next visitor.
//
// rusty-ssr calls this immediately before every render. It resets the storage
// objects it owns (a bundle that brought its own is left alone), then hands off
// to `globalThis.onSsrRequest` if the bundle defines one. THAT is where a
// bundle clears its own module state; define it next to whatever you cache.
//
// A throw here fails the render rather than being swallowed: a request that
// could not be isolated must not be served with someone else's state in it.
globalThis.__rustySsrReset = function () {
    if (__rustyLocalStorage !== null && globalThis.localStorage === __rustyLocalStorage) {
        __rustyLocalStorage = createStorage();
        globalThis.localStorage = __rustyLocalStorage;
    }
    if (__rustySessionStorage !== null && globalThis.sessionStorage === __rustySessionStorage) {
        __rustySessionStorage = createStorage();
        globalThis.sessionStorage = __rustySessionStorage;
    }
    if (typeof globalThis.onSsrRequest === 'function') globalThis.onSsrRequest();
};

"#;

/// Read a bundle from disk and prepend the prelude, without touching any global
/// state.
///
/// This is what [`SsrEngine`](crate::SsrEngine) uses. The `init_bundle*`
/// functions above put the composed source in a process-global `OnceLock`, and
/// that global is why two engines could never have two different bundles: the
/// second `init` quietly returned the first one's source, so the second engine
/// rendered with the wrong code. It also meant a test file could hold only one
/// bundle however many cases it had, which is a strange thing for a library to
/// impose on the people testing against it.
///
/// The composed string is handed to the pool, which hands it to each worker.
/// Nothing is shared between engines.
pub fn compose<P: AsRef<Path>>(path: P, polyfills: bool) -> SsrResult<String> {
    let path = path.as_ref();
    tracing::info!("📦 Loading SSR bundle from {:?} (polyfills={})", path, polyfills);

    let user_bundle = std::fs::read_to_string(path).map_err(|e| {
        SsrError::BundleLoad(format!("Failed to read SSR bundle from {:?}: {}", path, e))
    })?;

    Ok(if polyfills {
        format!("{}\n{}", BROWSER_POLYFILLS, user_bundle)
    } else {
        user_bundle
    })
}
