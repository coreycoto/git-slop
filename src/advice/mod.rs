mod artifact;
mod context;
mod provider;
mod validate;

pub use artifact::{
    AdviceRun, AdviceTimings, load_and_validate_artifact, render_advice_markdown, write_artifacts,
};
pub use context::{
    AdviceSelector, BuildInputOptions, EvaluationScenario, build_input, cache_input,
};
pub use provider::{ProviderConfig, ProviderKind, ProviderResult, ReasoningEffort, invoke};
pub use validate::{ValidatedResponse, validate_response};

pub const ADVICE_INPUT_SCHEMA_VERSION: u64 = 1;
pub const ADVICE_RESPONSE_SCHEMA_VERSION: u64 = 1;
pub const ADVICE_SCHEMA_VERSION: u64 = 1;
pub const CONTEXT_BUILDER_VERSION: u64 = 1;
