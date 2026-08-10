use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use globset::{Glob, GlobMatcher, GlobSet, GlobSetBuilder};
use serde_json::{Value, json};

use crate::config::{pointer_strings, pointer_u64};
use crate::model::{InventoryFile, SkippedCounts};

const NULL_BYTE_WINDOW: usize = 4096;

fn decode_text(raw: Vec<u8>) -> Option<String> {
    if raw.starts_with(&[0xef, 0xbb, 0xbf]) {
        return String::from_utf8(raw[3..].to_vec()).ok();
    }
    if raw.starts_with(&[0xff, 0xfe]) || raw.starts_with(&[0xfe, 0xff]) {
        let little_endian = raw.starts_with(&[0xff, 0xfe]);
        let bytes = &raw[2..];
        if bytes.len() % 2 != 0 {
            return None;
        }
        let units = bytes.chunks_exact(2).map(|pair| {
            if little_endian {
                u16::from_le_bytes([pair[0], pair[1]])
            } else {
                u16::from_be_bytes([pair[0], pair[1]])
            }
        });
        return char::decode_utf16(units)
            .collect::<Result<String, _>>()
            .ok();
    }
    String::from_utf8(raw).ok()
}

fn looks_binary(raw: &[u8]) -> bool {
    if raw.is_empty() || raw.starts_with(&[0xff, 0xfe]) || raw.starts_with(&[0xfe, 0xff]) {
        return false;
    }
    let window = NULL_BYTE_WINDOW.min(raw.len());
    let starts = [
        0,
        raw.len().saturating_sub(window) / 2,
        raw.len().saturating_sub(window),
    ];
    let mut sampled = 0usize;
    let mut suspicious = 0usize;
    for start in starts {
        for byte in &raw[start..(start + window).min(raw.len())] {
            sampled += 1;
            if *byte == 0 || *byte < 0x09 || matches!(*byte, 0x0b | 0x0c | 0x0e..=0x1f) {
                suspicious += 1;
            }
        }
    }
    raw.iter().take(window).any(|byte| *byte == 0) || suspicious.saturating_mul(100) > sampled
}

fn ignore_set(patterns: &[String]) -> Result<GlobSet> {
    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        builder
            .add(Glob::new(pattern).with_context(|| format!("invalid ignore glob {pattern:?}"))?);
        if !pattern.contains('/') {
            builder.add(
                Glob::new(&format!("**/{pattern}"))
                    .with_context(|| format!("invalid ignore glob {pattern:?}"))?,
            );
        }
    }
    Ok(builder.build()?)
}

fn language_for_path(path: &str) -> &'static str {
    let lower = path.to_ascii_lowercase();
    let extension = lower.rsplit('.').next().unwrap_or_default();
    match extension {
        "rs" => "Rust",
        "py" | "pyi" => "Python",
        "js" | "mjs" | "cjs" => "JavaScript",
        "jsx" => "JSX",
        "ts" | "mts" | "cts" => "TypeScript",
        "tsx" => "TSX",
        "swift" => "Swift",
        "kt" | "kts" => "Kotlin",
        "java" => "Java",
        "go" => "Go",
        "rb" => "Ruby",
        "php" => "PHP",
        "c" | "h" => "C",
        "cc" | "cpp" | "cxx" | "hpp" => "C++",
        "cs" => "C#",
        "sh" | "bash" | "zsh" => "Shell",
        "md" | "mdx" => "Markdown",
        "json" | "jsonl" => "JSON",
        "yaml" | "yml" => "YAML",
        "toml" => "TOML",
        "xml" => "XML",
        "html" | "htm" => "HTML",
        "css" | "scss" | "sass" | "less" => "CSS",
        "sql" => "SQL",
        "graphql" | "gql" => "GraphQL",
        "csv" => "CSV",
        "tsv" => "TSV",
        "txt" | "text" => "Plain Text",
        "svg" => "SVG",
        _ => "Plain Text",
    }
}

fn classification_for_path(path: &str) -> &'static str {
    let lower = path.to_ascii_lowercase();
    let name = lower.rsplit('/').next().unwrap_or(&lower);
    if lower.starts_with("vendor/")
        || lower.contains("/vendor/")
        || lower.starts_with("third_party/")
        || lower.contains("/third_party/")
        || lower.starts_with("node_modules/")
    {
        "vendored"
    } else if lower.starts_with("generated/")
        || lower.contains("/generated/")
        || lower.starts_with("dist/")
        || name.ends_with(".generated.rs")
        || name.ends_with(".generated.ts")
        || name.ends_with(".generated.js")
    {
        "generated"
    } else if lower.contains("/snapshots/")
        || lower.contains("/__snapshots__/")
        || lower.contains("/golden/")
        || lower.starts_with("snapshots/")
        || lower.starts_with("golden/")
        || name.ends_with(".snap")
    {
        "snapshot"
    } else if (lower.contains("/migrations/") || lower.starts_with("migrations/"))
        && (lower.contains("fixture") || lower.contains("test"))
    {
        "migration_fixture"
    } else if lower.starts_with("tests/")
        || lower.starts_with("test/")
        || lower.contains("/tests/")
        || lower.contains("/test/")
        || name.contains(".test.")
        || name.contains("_test.")
        || name.starts_with("test_")
        || lower.contains("__tests__")
    {
        "test"
    } else if lower.starts_with(".github/workflows/") {
        "workflow"
    } else if lower.starts_with(".github/issue_template/")
        || lower == ".github/funding.yml"
        || lower.starts_with("schemas/")
    {
        "config"
    } else if lower.starts_with("docs/") || lower.ends_with(".md") || lower.ends_with(".mdx") {
        "docs"
    } else if lower.starts_with("action/")
        || lower.starts_with("scripts/")
        || lower.starts_with("tools/")
        || lower.starts_with(".github/actions/")
    {
        "tool"
    } else if lower.starts_with("config/")
        || matches!(
            name,
            "cargo.toml" | "pyproject.toml" | "package.json" | "tsconfig.json" | "wrangler.toml"
        )
    {
        "config"
    } else if lower.starts_with("src/")
        || lower.starts_with("xtask/src/")
        || lower.starts_with("app/")
        || lower.starts_with("lib/")
        || lower.starts_with("crates/")
        || lower.starts_with("packages/")
    {
        "source"
    } else {
        "other"
    }
}

fn has_generated_marker(text: &str) -> bool {
    text.lines().take(3).any(|line| {
        let normalized = line.trim().to_ascii_lowercase();
        normalized.starts_with("# @generated")
            || normalized.starts_with("// @generated")
            || normalized.starts_with("/* @generated")
    })
}

fn line_counts(text: &str, language: &str) -> (usize, usize, usize, usize) {
    if text.is_empty() {
        return (0, 0, 0, 0);
    }
    let lines: Vec<&str> = text.lines().collect();
    let mut blank = 0;
    let mut comments = 0;
    let mut code = 0;
    let mut in_block_comment = false;
    for line in &lines {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            blank += 1;
            continue;
        }
        if in_block_comment {
            comments += 1;
            if trimmed.contains("*/") {
                in_block_comment = false;
            }
            continue;
        }
        let line_comment = match language {
            "Python" | "Ruby" | "Shell" | "YAML" | "TOML" => trimmed.starts_with('#'),
            "Markdown" => trimmed.starts_with("<!--"),
            _ => trimmed.starts_with("//"),
        };
        if line_comment {
            comments += 1;
        } else if trimmed.starts_with("/*") {
            comments += 1;
            in_block_comment = !trimmed.contains("*/");
        } else {
            code += 1;
        }
    }
    (lines.len(), code, comments, blank)
}

fn profile_for(path: &str, bytes: usize, config: &Value) -> &'static str {
    let lower = path.to_ascii_lowercase();
    let data_extension = matches!(
        lower.rsplit('.').next().unwrap_or_default(),
        "csv" | "tsv" | "parquet" | "ndjson" | "jsonl" | "sqlite" | "db" | "xml" | "json"
    );
    let data_path = lower.starts_with("data/")
        || lower.contains("/data/")
        || lower.contains("fixtures/")
        || lower.contains("reference_data/");
    let min_bytes = pointer_u64(config, "/health/data_context_min_bytes", 262_144) as usize;
    if data_extension && (data_path || bytes >= min_bytes) {
        "data_context"
    } else {
        "agent_context"
    }
}

struct CompiledPathOverride {
    matcher: GlobMatcher,
    classification: Option<String>,
    profile: Option<String>,
    language: Option<String>,
}

fn compile_path_overrides(config: &Value) -> Result<Vec<CompiledPathOverride>> {
    config
        .pointer("/inventory/path_overrides")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|mapping| {
            let pattern = mapping
                .get("glob")
                .and_then(Value::as_str)
                .context("inventory.path_overrides entry is missing glob")?;
            Ok(CompiledPathOverride {
                matcher: Glob::new(pattern)
                    .with_context(|| format!("invalid path override glob {pattern:?}"))?
                    .compile_matcher(),
                classification: mapping
                    .get("classification")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
                profile: mapping
                    .get("profile")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
                language: mapping
                    .get("language")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
            })
        })
        .collect()
}

fn path_override(
    path: &str,
    overrides: &[CompiledPathOverride],
) -> (Option<String>, Option<String>, Option<String>) {
    let mut classification = None;
    let mut profile = None;
    let mut language = None;
    for mapping in overrides {
        if mapping.matcher.is_match(path) {
            classification.clone_from(&mapping.classification);
            profile.clone_from(&mapping.profile);
            language.clone_from(&mapping.language);
        }
    }
    (classification, profile, language)
}

fn skipped_record(
    path: &str,
    bytes: usize,
    reason: &str,
    config: &Value,
    overrides: &[CompiledPathOverride],
) -> InventoryFile {
    let (classification_override, profile_override, language_override) =
        path_override(path, overrides);
    InventoryFile {
        path: path.replace('\\', "/"),
        bytes,
        lines: 0,
        blank_lines: 0,
        code_lines: 0,
        comment_lines: 0,
        language: language_override.unwrap_or_else(|| language_for_path(path).into()),
        profile: profile_override.unwrap_or_else(|| profile_for(path, bytes, config).to_string()),
        classification: classification_override
            .unwrap_or_else(|| classification_for_path(path).to_string()),
        text: String::new(),
        analysis_status: "skipped".to_string(),
        skipped_reason: Some(reason.to_string()),
        symlink_metadata: None,
    }
}

pub fn build(
    repo_root: &Path,
    tracked_paths: &[String],
    config: &Value,
) -> Result<(Vec<InventoryFile>, SkippedCounts)> {
    let patterns = pointer_strings(config, "/inventory/ignore_globs");
    let ignored = ignore_set(&patterns)?;
    let path_overrides = compile_path_overrides(config)?;
    let mut skipped = SkippedCounts::default();
    let mut records = Vec::new();
    let large_file_bytes = pointer_u64(config, "/resources/large_file_bytes", 2_097_152) as usize;
    for relative_path in tracked_paths {
        if ignored.is_match(relative_path) {
            skipped.ignored += 1;
            continue;
        }
        let absolute_path = repo_root.join(relative_path);
        let metadata = match fs::symlink_metadata(&absolute_path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                skipped.missing += 1;
                records.push(skipped_record(
                    relative_path,
                    0,
                    "missing",
                    config,
                    &path_overrides,
                ));
                continue;
            }
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("failed to inspect {}", absolute_path.display()));
            }
        };
        // Git represents a submodule as a tracked gitlink whose worktree path
        // is a directory. It is repository metadata, not a text file owned by
        // this analyzer.
        if metadata.is_dir() {
            skipped.ignored += 1;
            records.push(skipped_record(
                relative_path,
                0,
                "gitlink",
                config,
                &path_overrides,
            ));
            continue;
        }
        if !metadata.file_type().is_symlink() && metadata.len() > large_file_bytes as u64 {
            let bytes = usize::try_from(metadata.len()).unwrap_or(usize::MAX);
            records.push(skipped_record(
                relative_path,
                bytes,
                "large_file_limit",
                config,
                &path_overrides,
            ));
            continue;
        }
        // Analyze the link stored by Git, never the target it happens to resolve to
        // on the current machine. Following a tracked symlink could otherwise read
        // arbitrary content outside the repository.
        let raw = if metadata.file_type().is_symlink() {
            fs::read_link(&absolute_path)
                .with_context(|| format!("failed to read link {}", absolute_path.display()))?
                .to_string_lossy()
                .into_owned()
                .into_bytes()
        } else {
            fs::read(&absolute_path)
                .with_context(|| format!("failed to read {}", absolute_path.display()))?
        };
        if looks_binary(&raw) {
            skipped.binary += 1;
            records.push(skipped_record(
                relative_path,
                raw.len(),
                "binary",
                config,
                &path_overrides,
            ));
            continue;
        }
        let bytes = raw.len();
        let Some(mut text) = decode_text(raw) else {
            skipped.undecodable += 1;
            records.push(skipped_record(
                relative_path,
                bytes,
                "undecodable",
                config,
                &path_overrides,
            ));
            continue;
        };
        if !metadata.file_type().is_symlink() && text.contains("\r\n") {
            text = text.replace("\r\n", "\n");
        }
        let (classification_override, profile_override, language_override) =
            path_override(relative_path, &path_overrides);
        let language = language_override.unwrap_or_else(|| language_for_path(relative_path).into());
        let (lines, code_lines, comment_lines, blank_lines) = line_counts(&text, &language);
        records.push(InventoryFile {
            path: relative_path.replace('\\', "/"),
            bytes,
            lines,
            blank_lines,
            code_lines,
            comment_lines,
            language,
            profile: profile_override
                .unwrap_or_else(|| profile_for(relative_path, bytes, config).to_string()),
            classification: classification_override.unwrap_or_else(|| {
                if has_generated_marker(&text) {
                    "generated".to_string()
                } else {
                    classification_for_path(relative_path).to_string()
                }
            }),
            text,
            analysis_status: "analyzed".to_string(),
            skipped_reason: None,
            symlink_metadata: metadata.file_type().is_symlink().then(|| {
                json!({
                    "kind": "symbolic_link",
                    "target_status": if absolute_path.exists() { "resolves" } else { "broken" },
                    "target_content_read": false
                })
            }),
        });
    }
    records.sort_by(|left, right| left.path.cmp(&right.path));
    Ok((records, skipped))
}

#[cfg(test)]
mod tests {
    use std::fs;
    #[cfg(unix)]
    use std::os::unix::fs::symlink;

    use tempfile::tempdir;

    use super::build;
    use crate::config;

    #[cfg(unix)]
    #[test]
    fn tracked_symlinks_are_analyzed_without_following_their_targets() {
        let repository = tempdir().expect("repository");
        let outside = tempdir().expect("outside");
        let secret = outside.path().join("secret.txt");
        fs::write(&secret, "do not read this target").expect("secret");
        symlink(&secret, repository.path().join("linked.txt")).expect("symlink");

        let (files, skipped) = build(
            repository.path(),
            &["linked.txt".to_string()],
            &config::default_config(),
        )
        .expect("inventory");

        assert_eq!(files.len(), 1);
        assert_eq!(files[0].text, secret.to_string_lossy());
        assert!(!files[0].text.contains("do not read this target"));
        assert_eq!(skipped.missing, 0);
    }

    #[test]
    fn tracked_gitlink_directories_are_skipped_instead_of_read_as_files() {
        let repository = tempdir().expect("repository");
        fs::create_dir_all(repository.path().join("vendor/submodule")).expect("gitlink directory");

        let (files, skipped) = build(
            repository.path(),
            &["vendor/submodule".to_string()],
            &config::default_config(),
        )
        .expect("inventory");

        assert_eq!(files.len(), 1);
        assert_eq!(files[0].analysis_status, "skipped");
        assert_eq!(files[0].skipped_reason.as_deref(), Some("gitlink"));
        assert_eq!(skipped.ignored, 1);
    }

    #[test]
    fn utf8_bom_and_utf16_bom_text_are_decoded_instead_of_marked_binary() {
        let repository = tempdir().expect("repository");
        fs::write(repository.path().join("utf8.txt"), b"\xef\xbb\xbfhello\n").expect("utf8 bom");
        fs::write(
            repository.path().join("utf16.txt"),
            [0xff, 0xfe, b'h', 0, b'i', 0, b'\n', 0],
        )
        .expect("utf16 bom");
        let (files, skipped) = build(
            repository.path(),
            &["utf8.txt".to_string(), "utf16.txt".to_string()],
            &config::default_config(),
        )
        .expect("inventory");
        assert_eq!(files.len(), 2);
        let decoded = files
            .iter()
            .map(|file| (file.path.as_str(), file.text.as_str()))
            .collect::<std::collections::BTreeMap<_, _>>();
        assert_eq!(decoded["utf8.txt"], "hello\n");
        assert_eq!(decoded["utf16.txt"], "hi\n");
        assert_eq!(skipped.binary, 0);
        assert_eq!(skipped.undecodable, 0);
    }

    #[test]
    fn tracked_text_normalizes_crlf_for_cross_platform_analysis() {
        let repository = tempdir().expect("repository");
        fs::write(
            repository.path().join("source.rs"),
            b"fn one() {}\r\nfn two() {}\r\n",
        )
        .expect("crlf source");
        let (files, skipped) = build(
            repository.path(),
            &["source.rs".to_string()],
            &config::default_config(),
        )
        .expect("inventory");
        assert_eq!(skipped.binary, 0);
        assert_eq!(files[0].text, "fn one() {}\nfn two() {}\n");
        assert_eq!(files[0].bytes, 26);
    }

    #[test]
    fn explicit_generated_markers_override_ordinary_source_paths() {
        let repository = tempdir().expect("repository");
        fs::write(
            repository.path().join("release.yml"),
            "# @generated from reviewed stage fragments\nname: Release\n",
        )
        .expect("generated workflow");
        let (files, _) = build(
            repository.path(),
            &["release.yml".to_string()],
            &config::default_config(),
        )
        .expect("inventory");
        assert_eq!(files[0].classification, "generated");
    }
}
