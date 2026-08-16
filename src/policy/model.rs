use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Verdict {
    Approve,
    Abstain,
    Revise,
    Reject,
}

impl Verdict {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Approve => "approve",
            Self::Abstain => "abstain",
            Self::Revise => "revise",
            Self::Reject => "reject",
        }
    }

    fn rank(self) -> u8 {
        match self {
            Self::Approve => 0,
            Self::Abstain => 1,
            Self::Revise => 2,
            Self::Reject => 3,
        }
    }
}

pub fn aggregate_verdict(verdicts: impl IntoIterator<Item = Verdict>) -> Verdict {
    verdicts
        .into_iter()
        .max_by_key(|verdict| verdict.rank())
        .unwrap_or(Verdict::Abstain)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackRule {
    pub id: String,
    pub text: String,
    pub applicability: Vec<String>,
    pub severity: String,
    pub consequence: Verdict,
    pub required_evidence: Vec<String>,
    pub insufficient_evidence: Verdict,
    #[serde(default)]
    pub remediation: Option<String>,
    #[serde(default)]
    pub conflicts_with: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackManifest {
    pub schema_version: u64,
    pub id: String,
    pub name: String,
    pub description: String,
    pub version: String,
    pub license: String,
    pub min_git_slop_version: String,
    pub entrypoints: Vec<String>,
    pub applicability: Vec<String>,
    #[serde(default)]
    pub tests: Vec<String>,
    pub rules: Vec<PackRule>,
}

#[derive(Debug, Clone, Serialize)]
pub struct FileDigest {
    pub path: String,
    pub sha256: String,
    pub bytes: usize,
}

#[derive(Debug, Clone)]
pub struct ResolvedPack {
    pub manifest: PackManifest,
    pub root: PathBuf,
    pub content_digest: String,
    pub entrypoint_digests: Vec<FileDigest>,
    pub test_digests: Vec<FileDigest>,
    pub test_text: Vec<(String, String)>,
    pub source_type: String,
    pub source_revision: String,
    pub built_in: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EntrypointLock {
    pub path: String,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackLock {
    pub id: String,
    pub version: String,
    pub schema_version: u64,
    pub source_type: String,
    pub source_revision: String,
    pub content_digest: String,
    pub entrypoints: Vec<EntrypointLock>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PolicyLock {
    pub schema_version: u64,
    pub resolution_digest: String,
    pub packs: Vec<PackLock>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PolicyConflict {
    pub left_rule_id: String,
    pub right_rule_id: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CompiledPolicySet {
    pub schema_version: u64,
    pub resolution_digest: String,
    pub packs: Vec<PackLock>,
    pub rules: Vec<PackRule>,
    pub conflicts: Vec<PolicyConflict>,
}
