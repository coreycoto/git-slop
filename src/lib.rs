pub mod analyze;
pub mod build_info;
pub mod cli;
pub mod config;
pub mod git;
pub mod health;
pub mod history;
pub mod inventory;
pub mod model;
pub mod overlays;
pub mod report;
pub mod report_ops;
pub mod scoring;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const PROJECT_NAME: &str = "git-slop";
