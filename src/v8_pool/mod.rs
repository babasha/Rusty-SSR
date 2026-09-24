//! V8 Thread Pool for parallel SSR rendering
//!
//! This module provides a thread pool where each worker has its own V8 isolate,
//! solving the `!Send + !Sync` problem of V8 runtimes.
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────┐
//! │                   V8Pool                         │
//! │  ┌──────────┐                                   │
//! │  │ Channel  │◄──── render requests              │
//! │  └────┬─────┘                                   │
//! │       │                                         │
//! │  ┌────▼────┐  ┌─────────┐  ┌─────────┐        │
//! │  │Worker 0 │  │Worker 1 │  │Worker N │  ...    │
//! │  │ (V8)    │  │ (V8)    │  │ (V8)    │        │
//! │  └─────────┘  └─────────┘  └─────────┘        │
//! └─────────────────────────────────────────────────┘
//! ```

mod bundle;
mod pool;
mod renderer;
mod runtime;

pub use bundle::{compose, BROWSER_POLYFILLS};
pub use pool::{PoolError, PoolMetrics, V8Pool, V8PoolConfig};
pub use renderer::{RenderFnShape, RenderPayload, Rendered};
