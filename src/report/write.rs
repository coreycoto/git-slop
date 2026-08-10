use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};

use anyhow::{Context, Result, anyhow};
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::{Value, json};

use super::assembly::assemble_report;
use super::render::{render_compatibility_summary, render_terminal};
use crate::config;
use crate::health::render_health_from_report;
use crate::model::{
    Analysis, FileAnalysis, FindResult, FolderAnalysis, HealthRollup, ScopeIdentity,
};

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

include!("write/profile.rs");
include!("write/contract.rs");
include!("write/validation.rs");
include!("write/storage.rs");
