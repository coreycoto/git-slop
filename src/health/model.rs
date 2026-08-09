use serde_json::Value;

pub(super) const DEFAULT_COMPACT_MAX: u64 = 3_072;
pub(super) const DEFAULT_HEALTHY_MAX: u64 = 8_000;
pub(super) const DEFAULT_WARNING_MAX: u64 = 10_000;
pub(super) const DEFAULT_FOLDER_COMPACT_MAX: u64 = 31_999;
pub(super) const DEFAULT_FOLDER_HEALTHY_MAX: u64 = 128_000;
pub(super) const DEFAULT_FOLDER_WARNING_MAX: u64 = 256_000;
pub(super) const DEFAULT_FOLDER_WARNING_FILES: u64 = 17;
pub(super) const DEFAULT_FOLDER_REFACTOR_FILES: u64 = 37;
pub(super) const DEFAULT_TOP_FILES: usize = 10;
pub(super) const DEFAULT_TOP_FOLDERS: usize = 10;

#[derive(Debug, Clone, Default)]
pub(super) struct Totals {
    pub(super) files: usize,
    pub(super) lines: usize,
    pub(super) code: usize,
    pub(super) comments: usize,
    pub(super) blanks: usize,
    pub(super) tokens: usize,
}

impl Totals {
    pub(super) fn add_file(&mut self, file: &Value) {
        self.files += 1;
        self.lines += usize_field(file, "lines");
        self.code += usize_field(file, "code_lines");
        self.comments += usize_field(file, "comment_lines");
        self.blanks += usize_field(file, "blank_lines");
        self.tokens += usize_field(file, "tokens");
    }
}

pub(super) fn config_u64(config: &Value, pointer: &str, default: u64) -> u64 {
    config
        .pointer(pointer)
        .and_then(Value::as_u64)
        .unwrap_or(default)
}

pub(super) fn config_usize(config: &Value, pointer: &str, default: usize) -> usize {
    config_u64(config, pointer, default as u64) as usize
}

pub(super) fn string_field<'a>(value: &'a Value, field: &str) -> &'a str {
    value.get(field).and_then(Value::as_str).unwrap_or_default()
}

pub(super) fn usize_field(value: &Value, field: &str) -> usize {
    value
        .get(field)
        .and_then(Value::as_u64)
        .and_then(|number| usize::try_from(number).ok())
        .unwrap_or_default()
}

pub(super) fn float_field(value: &Value, field: &str) -> f64 {
    value.get(field).and_then(Value::as_f64).unwrap_or_default()
}

pub(super) fn string_array(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(ToOwned::to_owned)
        .collect()
}

pub(super) fn file_profile(file: &Value) -> &str {
    file.get("profile")
        .and_then(Value::as_str)
        .filter(|profile| !profile.is_empty())
        .unwrap_or("agent_context")
}

pub(super) fn classification_for_path(path: &str) -> &'static str {
    if path.starts_with("tests/")
        || path.starts_with("test/")
        || path.contains("/tests/")
        || path.contains("/test/")
        || path.contains("__tests__")
    {
        "test"
    } else if path.starts_with(".github/workflows/") {
        "workflow"
    } else if path.starts_with(".github/ISSUE_TEMPLATE/")
        || path == ".github/FUNDING.yml"
        || path.starts_with("schemas/")
    {
        "config"
    } else if path.starts_with("docs/") || path.ends_with(".md") || path.ends_with(".mdx") {
        "docs"
    } else if path.starts_with("scripts/")
        || path.starts_with("tools/")
        || path.starts_with(".github/")
    {
        "tool"
    } else if path.starts_with("data/")
        || path.ends_with(".csv")
        || path.ends_with(".parquet")
        || path.ends_with(".geojson")
    {
        "data"
    } else {
        "source"
    }
}

pub(super) fn classification(value: &Value) -> String {
    value
        .get("classification")
        .and_then(Value::as_str)
        .filter(|item| !item.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| classification_for_path(string_field(value, "path")).to_string())
}

pub(super) fn health_file_band(file: &Value) -> String {
    match string_field(file, "context_band") {
        "critical" | "refactor_required" | "budget_exceeded" => "budget_exceeded".to_string(),
        "warning" => "warning".to_string(),
        "healthy" => "healthy".to_string(),
        _ => "compact".to_string(),
    }
}

pub(super) fn folder_health_band_for(
    direct_tokens: u64,
    direct_files: u64,
    config: &Value,
) -> String {
    let compact_max = config_u64(
        config,
        "/health/folder_bands/compact_max_direct_tokens",
        DEFAULT_FOLDER_COMPACT_MAX,
    );
    let healthy_max = config_u64(
        config,
        "/health/folder_bands/healthy_max_direct_tokens",
        DEFAULT_FOLDER_HEALTHY_MAX,
    );
    let warning_max = config_u64(
        config,
        "/health/folder_bands/warning_max_direct_tokens",
        DEFAULT_FOLDER_WARNING_MAX,
    );
    let warning_files = config_u64(
        config,
        "/health/folder_bands/warning_max_direct_files",
        DEFAULT_FOLDER_WARNING_FILES,
    );
    let refactor_files = config_u64(
        config,
        "/health/folder_bands/refactor_required_max_direct_files",
        DEFAULT_FOLDER_REFACTOR_FILES,
    );
    if direct_tokens > warning_max || direct_files > refactor_files {
        "budget_exceeded".to_string()
    } else if direct_tokens > healthy_max || direct_files > warning_files {
        "warning".to_string()
    } else if direct_tokens > compact_max {
        "healthy".to_string()
    } else {
        "compact".to_string()
    }
}

pub(super) fn direct_parent(path: &str) -> String {
    path.trim_matches('/')
        .rsplit_once('/')
        .map(|(parent, _)| parent.to_string())
        .unwrap_or_else(|| ".".to_string())
}

pub(super) fn band_rank(band: &str) -> usize {
    match band {
        "budget_exceeded" | "refactor_required" | "critical" => 3,
        "warning" | "high" => 2,
        "healthy" | "moderate" => 1,
        _ => 0,
    }
}
