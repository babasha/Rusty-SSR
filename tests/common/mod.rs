//! Shared setup for the integration tests.
//!
//! Every one of these files needs the same three steps — a temporary directory,
//! a bundle written into it, an engine pointed at that bundle — and each used to
//! carry its own copy, thirty-odd of them across eight files. They are not
//! interestingly different from one another, so a change to how an engine is
//! built (a new required option, a renamed builder method) meant thirty edits
//! and thirty chances to leave one behind.

#![allow(dead_code)]

use std::ops::Deref;
use std::sync::Arc;

use rusty_ssr::{SsrConfigBuilder, SsrEngine};
use tempfile::TempDir;

/// A bundle on disk, and the engine that loads it.
///
/// This holds the temporary directory as well, because dropping it deletes the
/// bundle. Some of these setups used to let the directory go at the end of the
/// helper that made it, and survived only because the bundle happens to be read
/// once, at build time — true today, and not a thing a test should rest on.
pub struct Fixture {
    engine: Arc<SsrEngine>,
    _dir: TempDir,
}

impl Fixture {
    /// The engine as an `Arc`, for tests that hand it to spawned tasks.
    ///
    /// The fixture itself has to outlive them: it owns the directory the bundle
    /// lives in.
    pub fn shared(&self) -> Arc<SsrEngine> {
        Arc::clone(&self.engine)
    }
}

impl Deref for Fixture {
    type Target = SsrEngine;

    fn deref(&self) -> &SsrEngine {
        &self.engine
    }
}

/// An engine running `source`, with one worker.
pub fn engine(source: &str) -> Fixture {
    engine_with(source, |b| b)
}

/// An engine running `source`, with `configure` applied to the builder.
///
/// One worker by default, and most of these tests want exactly that: they are
/// about what happens inside a single isolate across two renders, and with more
/// workers the second render can land on a fresh one and pass for the wrong
/// reason. Pass `.pool_size(n)` from `configure` where a test needs several.
pub fn engine_with(
    source: &str,
    configure: impl FnOnce(SsrConfigBuilder) -> SsrConfigBuilder,
) -> Fixture {
    try_engine_with(source, configure).expect("build engine")
}

/// [`engine_with`], for a test that expects the build itself to fail.
pub fn try_engine_with(
    source: &str,
    configure: impl FnOnce(SsrConfigBuilder) -> SsrConfigBuilder,
) -> Result<Fixture, rusty_ssr::SsrError> {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("bundle.js");
    std::fs::write(&path, source).expect("write bundle");

    let engine = configure(SsrEngine::builder().bundle_path(&path).pool_size(1)).build_engine()?;

    Ok(Fixture {
        engine: Arc::new(engine),
        _dir: dir,
    })
}
