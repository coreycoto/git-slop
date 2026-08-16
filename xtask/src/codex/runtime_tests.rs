use std::fs;

use serde_json::{Value as JsonValue, json};
use tempfile::TempDir;

use super::runtime_manifest::{
    EXPECTED_CHECKSUMS, EXPECTED_MARKETPLACE_NAME, EXPECTED_PLUGIN_SHA, EXPECTED_RELEASE_MANIFEST,
    EXPECTED_RUNTIME_ARCHIVE, EXPECTED_RUNTIME_MEMBER, EXPECTED_RUNTIME_REPOSITORY,
    EXPECTED_RUNTIME_SHA256, EXPECTED_RUNTIME_SIZE, EXPECTED_RUNTIME_TAG, EXPECTED_RUNTIME_TARGET,
    EXPECTED_RUNTIME_VERSION, INSTALLED_PLUGIN_NAME, validate_agent_plugin_wrapper_text,
    validate_marketplace_source_manifest,
};
use super::runtime_workflows::{AgentPluginWorkflowKind, validate_agent_plugin_workflow_text};
use super::{EXPECTED_PLUGIN_URL, validate_release_workflow};

include!("runtime_tests/group_1.rs");
include!("runtime_tests/group_2.rs");
include!("runtime_tests/group_3.rs");
