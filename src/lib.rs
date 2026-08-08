#[doc(hidden)]
pub mod analyze;
#[doc(hidden)]
pub mod build_info;
pub mod cli;
#[doc(hidden)]
pub mod config;
#[doc(hidden)]
pub mod git;
#[doc(hidden)]
pub mod health;
#[doc(hidden)]
pub mod history;
#[doc(hidden)]
pub mod inventory;
#[doc(hidden)]
pub mod model;
#[doc(hidden)]
pub mod overlays;
#[doc(hidden)]
pub mod report;
#[doc(hidden)]
pub mod report_ops;
#[doc(hidden)]
pub mod scoring;

// Supported library surface. Detector and report internals remain private so
// schema changes cannot accidentally become Rust API compatibility promises.
pub use analyze::{run_find, run_find_in, run_find_in_with_options, run_find_scoped};
pub use model::FindResult;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const PROJECT_NAME: &str = "git-slop";
