use std::path::PathBuf;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use clap::ValueEnum;
use serde::Serialize;
use serde_json::{Value, json};

mod http;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ProviderKind {
    OpenaiCompatible,
    Ollama,
    Mock,
}

impl ProviderKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::OpenaiCompatible => "openai-compatible",
            Self::Ollama => "ollama",
            Self::Mock => "mock",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum ReasoningEffort {
    Low,
    Medium,
    High,
}

impl ReasoningEffort {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
        }
    }
}

pub struct ProviderConfig {
    pub kind: ProviderKind,
    pub endpoint: String,
    pub model: String,
    pub runtime_model: String,
    pub reasoning_effort: ReasoningEffort,
    pub connect_timeout: Duration,
    pub timeout: Duration,
    pub max_response_bytes: usize,
    pub max_output_tokens: usize,
    pub context_window_tokens: usize,
    pub runtime_label: Option<String>,
    pub model_digest: Option<String>,
    pub mock_response: Option<PathBuf>,
    pub resource_guard: Option<super::RuntimeResourceGuard>,
    pub resource_preflight: Option<super::ResourcePreflight>,
}

#[derive(Debug)]
pub struct ProviderResult {
    pub response: Value,
    pub metadata: Value,
    pub elapsed_ms: u128,
}

include!("provider/adapter.rs");
include!("provider/tests.rs");
