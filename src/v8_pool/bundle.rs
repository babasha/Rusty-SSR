//! SSR Bundle loader

use std::path::Path;
use std::sync::OnceLock;

use crate::error::{SsrError, SsrResult};

/// Cached SSR bundle (loaded once at startup)
static SSR_BUNDLE: OnceLock<String> = OnceLock::new();

/// Browser polyfills for V8 compatibility
/// These mock browser APIs that don't exist in V8 isolates
const BROWSER_POLYFILLS: &str = r#"
// =========================================
// Rusty-SSR Browser Polyfills
// =========================================

// SSR detection flag
globalThis.__SSR__ = true;

// Basic globals
globalThis.window = globalThis;
globalThis.self = globalThis;

// --- Platform stubs (Tauri, Electron, Capacitor) ---
// Tauri IPC — prevents crash when @tauri-apps/api is bundled
globalThis.__TAURI_IPC__ = function() {};
globalThis.__TAURI_INTERNALS__ = {
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
globalThis.Capacitor = globalThis.Capacitor || { isNativePlatform: function() { return false; } };

// Minimal timers (no real scheduling; executes immediately)
let __rustyTimerId = 0;
globalThis.setTimeout = (cb, _ms, ...args) => {
    __rustyTimerId += 1;
    if (typeof cb === 'function') { cb(...args); }
    return __rustyTimerId;
};
globalThis.clearTimeout = () => {};
globalThis.setInterval = (cb, _ms, ...args) => {
    __rustyTimerId += 1;
    if (typeof cb === 'function') { cb(...args); }
    return __rustyTimerId;
};
globalThis.clearInterval = () => {};

// Document mock
globalThis.document = {
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
globalThis.navigator = {
    userAgent: 'Rusty-SSR/1.0',
    language: 'en-US',
    languages: ['en-US', 'en'],
    platform: 'Linux',
    onLine: true
};

// Location mock
globalThis.location = {
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
globalThis.requestAnimationFrame = (cb) => setTimeout(cb, 16);
globalThis.cancelAnimationFrame = (id) => clearTimeout(id);

// Performance mock
globalThis.performance = {
    now: () => Date.now(),
    mark: () => {},
    measure: () => {},
    getEntriesByName: () => [],
    getEntriesByType: () => []
};

// Storage mock
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
globalThis.localStorage = createStorage();
globalThis.sessionStorage = createStorage();

// Fetch mock (minimal - throws if actually used)
globalThis.fetch = async () => {
    throw new Error('fetch() is not available in SSR. Use data prop instead.');
};

// MutationObserver mock
globalThis.MutationObserver = class MutationObserver {
    constructor() {}
    observe() {}
    disconnect() {}
    takeRecords() { return []; }
};

// ResizeObserver mock
globalThis.ResizeObserver = class ResizeObserver {
    constructor() {}
    observe() {}
    unobserve() {}
    disconnect() {}
};

// IntersectionObserver mock
globalThis.IntersectionObserver = class IntersectionObserver {
    constructor() {}
    observe() {}
    unobserve() {}
    disconnect() {}
};

// matchMedia mock
globalThis.matchMedia = (query) => ({
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
globalThis.Image = class Image {
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

"#;

/// Initialize the SSR bundle from a file
///
/// This should be called once at application startup.
/// The bundle is cached and reused for all V8 workers.
/// Browser polyfills are automatically prepended.
pub fn init_bundle<P: AsRef<Path>>(path: P) -> SsrResult<()> {
    let path = path.as_ref();

    if SSR_BUNDLE.get().is_some() {
        return Ok(());
    }

    tracing::info!("📦 Loading SSR bundle from {:?}", path);

    let user_bundle = std::fs::read_to_string(path).map_err(|e| {
        SsrError::BundleLoad(format!("Failed to read SSR bundle from {:?}: {}", path, e))
    })?;

    let full_bundle = format!("{}\n{}", BROWSER_POLYFILLS, user_bundle);

    SSR_BUNDLE
        .set(full_bundle)
        .map_err(|_| SsrError::BundleLoad("Bundle already initialized".to_string()))?;

    Ok(())
}

/// Initialize the SSR bundle from a string
///
/// Use this if you want to embed the bundle or load it from elsewhere.
/// Browser polyfills are automatically prepended.
pub fn init_bundle_from_string(bundle: String) -> SsrResult<()> {
    let full_bundle = format!("{}\n{}", BROWSER_POLYFILLS, bundle);
    SSR_BUNDLE
        .set(full_bundle)
        .map_err(|_| SsrError::BundleLoad("Bundle already initialized".to_string()))?;
    Ok(())
}

/// Initialize the SSR bundle from a string WITHOUT polyfills
///
/// Use this if your bundle already includes all necessary globals.
pub fn init_bundle_raw(bundle: String) -> SsrResult<()> {
    SSR_BUNDLE
        .set(bundle)
        .map_err(|_| SsrError::BundleLoad("Bundle already initialized".to_string()))?;
    Ok(())
}

/// Get the cached SSR bundle
///
/// # Panics
/// Panics if the bundle has not been initialized.
pub fn get_bundle() -> &'static str {
    SSR_BUNDLE
        .get()
        .expect("SSR bundle not initialized. Call init_bundle() first.")
}

/// Check if the bundle is initialized
pub fn is_initialized() -> bool {
    SSR_BUNDLE.get().is_some()
}
