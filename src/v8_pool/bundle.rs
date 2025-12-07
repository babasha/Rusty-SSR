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

// Basic globals
globalThis.window = globalThis;
globalThis.self = globalThis;

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
    createElement: (tag) => ({
        tagName: tag.toUpperCase(),
        style: {},
        setAttribute: () => {},
        getAttribute: () => null,
        appendChild: () => {},
        removeChild: () => {},
        addEventListener: () => {},
        removeEventListener: () => {},
        classList: {
            add: () => {},
            remove: () => {},
            toggle: () => {},
            contains: () => false
        }
    }),
    createTextNode: (text) => ({ textContent: text }),
    getElementById: () => null,
    querySelector: () => null,
    querySelectorAll: () => [],
    addEventListener: () => {},
    removeEventListener: () => {},
    documentElement: { style: {} },
    head: { appendChild: () => {} },
    body: { appendChild: () => {} }
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
