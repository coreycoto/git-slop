use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use globset::{Glob, GlobSet, GlobSetBuilder};
use serde_json::Value;

use crate::config::{pointer_strings, pointer_u64};
use crate::model::{InventoryFile, SkippedCounts};

const NULL_BYTE_WINDOW: usize = 4096;

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
    if lower.starts_with("tests/")
        || lower.starts_with("test/")
        || lower.contains("/tests/")
        || lower.contains("/test/")
        || name.contains(".test.")
        || name.contains("_test.")
        || name.starts_with("test_")
        || lower.contains("__tests__")
    {
        "test"
    } else if lower.starts_with("docs/") || lower.ends_with(".md") || lower.ends_with(".mdx") {
        "docs"
    } else if lower.starts_with("scripts/")
        || lower.starts_with("tools/")
        || lower.starts_with(".github/")
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

pub fn build(
    repo_root: &Path,
    tracked_paths: &[String],
    config: &Value,
) -> Result<(Vec<InventoryFile>, SkippedCounts)> {
    let patterns = pointer_strings(config, "/inventory/ignore_globs");
    let ignored = ignore_set(&patterns)?;
    let mut skipped = SkippedCounts::default();
    let mut records = Vec::new();
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
        if raw[..raw.len().min(NULL_BYTE_WINDOW)].contains(&0) {
            skipped.binary += 1;
            continue;
        }
        let bytes = raw.len();
        let Ok(text) = String::from_utf8(raw) else {
            skipped.undecodable += 1;
            continue;
        };
        let language = language_for_path(relative_path).to_string();
        let (lines, code_lines, comment_lines, blank_lines) = line_counts(&text, &language);
        records.push(InventoryFile {
            path: relative_path.replace('\\', "/"),
            absolute_path,
            bytes,
            lines,
            blank_lines,
            code_lines,
            comment_lines,
            language,
            profile: profile_for(relative_path, bytes, config).to_string(),
            classification: classification_for_path(relative_path).to_string(),
            text,
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

        assert!(files.is_empty());
        assert_eq!(skipped.ignored, 1);
    }
}
