//! SSR Bundle loader

use std::path::Path;
use std::sync::OnceLock;

use crate::error::{SsrError, SsrResult};

/// Cached SSR bundle (loaded once at startup)
static SSR_BUNDLE: OnceLock<String> = OnceLock::new();

/// Initialize the SSR bundle from a file
///
/// This should be called once at application startup.
/// The bundle is cached and reused for all V8 workers.
pub fn init_bundle<P: AsRef<Path>>(path: P) -> SsrResult<()> {
    let path = path.as_ref();

    SSR_BUNDLE.get_or_init(|| {
        tracing::info!("📦 Loading SSR bundle from {:?}", path);

        std::fs::read_to_string(path).unwrap_or_else(|e| {
            panic!("Failed to read SSR bundle from {:?}: {}", path, e);
        })
    });

    Ok(())
}

/// Initialize the SSR bundle from a string
///
/// Use this if you want to embed the bundle or load it from elsewhere.
pub fn init_bundle_from_string(bundle: String) -> SsrResult<()> {
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
