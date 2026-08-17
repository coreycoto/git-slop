use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use chrono::Utc;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::policy::CompiledPolicySet;
use crate::text::visible_controls;

use super::{ADVICE_SCHEMA_VERSION, ProviderResult, ValidatedResponse};

pub struct AdviceRun {
    pub artifact: Value,
    pub markdown: String,
}

pub struct AdviceTimings {
    pub context_elapsed_ms: u128,
    pub provider_elapsed_ms: u128,
    pub validation_elapsed_ms: u128,
    pub time_to_validated_artifact_ms: u128,
}

fn sha256(bytes: impl AsRef<[u8]>) -> String {
    hex::encode(Sha256::digest(bytes.as_ref()))
}

impl AdviceRun {
    pub fn new(
        input: &Value,
        policies: &CompiledPolicySet,
        provider: &ProviderResult,
        validated: &ValidatedResponse,
        timings: AdviceTimings,
    ) -> Result<Self> {
        let generated_at = Utc::now().to_rfc3339();
        let response_digest = sha256(serde_json::to_vec(&provider.response)?);
        let artifact = json!({
            "schema_version": ADVICE_SCHEMA_VERSION,
            "command": "advise",
            "generated_at": generated_at,
            "report": input.get("report").cloned().unwrap_or(Value::Null),
            "selector": input.get("selector").cloned().unwrap_or(Value::Null),
            "candidate_ids": input.pointer("/reference_index/candidates").cloned().unwrap_or_else(|| json!([])),
            "context": {
                "builder_version": input.get("context_builder_version").cloned().unwrap_or(Value::Null),
                "digest": input.get("context_digest").cloned().unwrap_or(Value::Null),
                "limits": input.get("limits").cloned().unwrap_or(Value::Null),
                "missing_evidence": input.get("missing_evidence").cloned().unwrap_or_else(|| json!([])),
                "reference_index": input.get("reference_index").cloned().unwrap_or(Value::Null),
            },
            "policies": {
                "resolution_digest": &policies.resolution_digest,
                "packs": &policies.packs,
                "conflicts": &policies.conflicts,
            },
            "provider": &provider.metadata,
            "timing": {
                "context_elapsed_ms": timings.context_elapsed_ms,
                "provider_elapsed_ms": timings.provider_elapsed_ms,
                "validation_elapsed_ms": timings.validation_elapsed_ms,
                "time_to_validated_artifact_ms": timings.time_to_validated_artifact_ms
            },
            "response_sha256": response_digest,
            "evaluation": validated,
            "validation": {
                "status": "valid",
                "aggregate_recomputed": true,
                "references_validated": true,
                "warnings": &validated.warnings,
            },
            "boundary": "Policy-guided advice is non-mutating and advisory. It cannot rewrite detector truth or change git slop check results."
        });
        let schema: Value = serde_json::from_str(include_str!("../../schemas/advice-1.json"))?;
        let validator = jsonschema::draft202012::options()
            .should_validate_formats(true)
            .build(&schema)
            .context("embedded advice artifact schema is invalid")?;
        if let Some(error) = validator.iter_errors(&artifact).next() {
            bail!(
                "generated advice artifact does not match schema v{ADVICE_SCHEMA_VERSION} at {}: {}",
                error.instance_path(),
                error
            );
        }
        let markdown = render_advice_markdown(&artifact);
        Ok(Self { artifact, markdown })
    }
}

include!("artifact/render.rs");
include!("artifact/persistence.rs");
include!("artifact/validation.rs");
include!("artifact/state.rs");
include!("artifact/tests.rs");
