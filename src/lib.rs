#![recursion_limit = "512"]

mod analyze;
pub(crate) mod baseline;
pub(crate) mod build_info;
pub(crate) mod cache;
pub mod cli;
pub(crate) mod config;
pub(crate) mod error;
pub(crate) mod estimate;
pub(crate) mod git;
pub(crate) mod health;
pub(crate) mod history;
pub(crate) mod inventory;
mod model;
pub(crate) mod overlays;
pub(crate) mod report;
pub(crate) mod report_ops;
pub(crate) mod scoring;
pub(crate) mod text;

// Supported library surface. Detector and report internals remain private so
// schema changes cannot accidentally become Rust API compatibility promises.
pub use analyze::{
    FindOptions, run_find, run_find_in, run_find_in_with_options, run_find_scoped,
    run_find_with_options,
};
pub use model::FindResult;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const PROJECT_NAME: &str = "git-slop";
