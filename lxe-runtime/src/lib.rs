//! LXE Runtime Library
//!
//! This library provides the installer runtime components:
//! - Installation logic with polkit support
//! - Async extraction
//! - GUI wizard
//! - Installation state detection

// Re-export from lxe-common for convenience
pub use lxe_common::{config, metadata, signing, paths, payload};

// Runtime-specific modules
pub mod installer;
pub mod extractor;
pub mod polkit;
pub mod state;
pub mod ui;
pub mod manifest;
pub mod libloader;

