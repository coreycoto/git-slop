use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use crate::text::visible_controls;
use anyhow::{Result, anyhow, bail};
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};

mod compare;
mod explain;
mod github;
mod plan;
mod sarif;
mod verification;

pub use compare::{compare_payload_with_policy, render_compare_text};
pub use explain::{explain_payload, render_explain_summary_text, render_explain_text};
pub use github::{
    PromptPackOptions, health_json_payload, render_github_annotations, write_prompt_pack,
};
pub use plan::{plan_payload, render_plan_text};
pub use sarif::{render_json, sarif_payload};

const REPORT_SCHEMA_VERSION: i64 = 5;
const EXPLAIN_SCHEMA_VERSION: i64 = 2;
const PLAN_SCHEMA_VERSION: i64 = 2;
const COMPARE_SCHEMA_VERSION: i64 = 1;
const SARIF_SCHEMA_VERSION: i64 = 1;
const MAX_SLICE_FILES: usize = 5;

const RELATIONSHIP_KEYS: [&str; 5] = [
    "duplicate_neighborhoods",
    "near_duplicate_neighborhoods",
    "temporal_coupling_edges",
    "lexical_affinity_edges",
    "boundary_leakage_edges",
];
const CLUSTER_KEYS: [&str; 4] = [
    "duplicate_sets",
    "scattered_concepts",
    "boundary_leakage_clusters",
    "consolidation_candidates",
];

pub const EXPLAIN_BOUNDARY_NOTE: &str = "Interpretation boundary: this is structural evidence, not proof that an abstraction, boundary, or refactor is correct.";
pub const PLAN_BOUNDARY_NOTE: &str = "Plan boundary: this is a bounded proposal only. It does not mutate code, GitHub, or detector truth, and it does not guarantee correctness or safety.";
pub const COMPARE_BOUNDARY_NOTE: &str = "Compare boundary: this is a read-only comparison of two existing reports. It does not rerun the detector, imply causality, mutate repo state, or change detector scoring semantics.";
pub const SARIF_BOUNDARY_NOTE: &str = "SARIF export boundary: this is a deterministic projection of existing git-slop report evidence. It does not rerun the detector, upload results, mutate code, or change detector scoring semantics.";

include!("report_ops/readiness.rs");
include!("report_ops/access.rs");
include!("report_ops/check.rs");
include!("report_ops/evidence.rs");
