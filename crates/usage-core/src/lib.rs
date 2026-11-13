//! `usage-core`: everything about monitoring Claude usage that does not need a UI.
//!
//! See `docs/ARCHITECTURE.md`.

pub mod auth;
pub mod clock;
pub mod config;
pub mod diagnostics;
pub mod errors;
pub mod events;
pub mod model;
pub mod monitoring;
pub mod paths;
pub mod providers;
pub mod statusline_protocol;
pub mod storage;

pub use errors::{ErrorCategory, ErrorInfo, Result, UsageError};
