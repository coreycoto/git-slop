fn replace_quoted_strings(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    let mut previous = None;
    while let Some(character) = chars.next() {
        if !matches!(character, '\'' | '"' | '`') {
            result.push(character);
            previous = Some(character);
            continue;
        }
        if character == '\''
            && previous.is_some_and(char::is_alphanumeric)
            && chars.peek().is_some_and(|next| next.is_alphanumeric())
        {
            result.push(character);
            previous = Some(character);
            continue;
        }
        result.push_str(" str ");
        let quote = character;
        let mut escaped = false;
        for next in chars.by_ref() {
            if escaped {
                escaped = false;
                continue;
            }
            if next == '\\' {
                escaped = true;
            } else if next == quote {
                break;
            }
        }
        previous = Some(' ');
    }
    result
}

fn structural_mode(path: &str) -> &'static str {
    match Path::new(path).extension().and_then(|value| value.to_str()) {
        Some("md" | "mdx") => "markdown",
        Some("txt") => "prose",
        Some("sql") => "sql",
        Some("html" | "htm" | "xml" | "svg") => "markup",
        _ => "code",
    }
}

fn structural_categories(mode: &str, text: &str) -> Value {
    match mode {
        "markdown" => {
            let mut fenced = false;
            let mut prose_lines = 0usize;
            let mut fenced_code_lines = 0usize;
            for line in text.lines() {
                if line.trim_start().starts_with("```") {
                    fenced = !fenced;
                } else if fenced {
                    fenced_code_lines += 1;
                } else {
                    prose_lines += 1;
                }
            }
            json!({"mode": mode, "prose_lines": prose_lines, "fenced_code_lines": fenced_code_lines})
        }
        "sql" => {
            json!({"mode": mode, "query_lines": text.lines().count(), "string_literals_normalized": true})
        }
        "markup" => {
            json!({"mode": mode, "markup_lines": text.lines().count(), "tag_and_text_categories": true})
        }
        "prose" => json!({"mode": mode, "prose_lines": text.lines().count()}),
        _ => {
            json!({"mode": "code", "code_lines": text.lines().count(), "string_literals_normalized": true})
        }
    }
}

fn structural_content_tokens(mode: &str, text: &str) -> Vec<String> {
    let normalized: String = text.nfkc().collect();
    let normalized = normalized.replace(['\u{2018}', '\u{2019}'], "'");
    let normalized = ACRONYM_BOUNDARY_RE.replace_all(&normalized, "$1 $2");
    let normalized = CAMEL_CASE_RE.replace_all(&normalized, "$1 $2");
    let normalized = normalized.replace(['-', '/'], " ");
    let normalized = if matches!(mode, "prose" | "markdown") {
        normalized
    } else {
        replace_quoted_strings(&normalized)
    };
    let normalized = NUMBER_RE.replace_all(&normalized, " 0 ");
    let lower = normalized.to_lowercase();
    lower
        .unicode_words()
        .flat_map(|word| word.split('_'))
        .map(|word| {
            word.split_once('\'')
                .filter(|(prefix, suffix)| prefix.chars().count() == 1 && !suffix.is_empty())
                .map_or(word, |(_, suffix)| suffix)
        })
        .filter(|item| item.chars().count() > 1)
        .map(ToOwned::to_owned)
        .collect()
}

fn structural_path_tokens(path: &str) -> Vec<String> {
    path.replace(['-', '_', '.'], "/")
        .to_ascii_lowercase()
        .split('/')
        .filter(|item| !item.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

#[cfg(test)]
fn structural_tokens(path: &str, text: &str) -> Vec<String> {
    let mut tokens = structural_content_tokens(structural_mode(path), text);
    tokens.extend(structural_path_tokens(path));
    tokens
}

fn content_fingerprint(text: &str) -> String {
    hex::encode(Sha256::digest(text.as_bytes()))
}

fn structural_symbols(language: &str, text: &str) -> Vec<String> {
    let expression = match language {
        "Rust" => Some(&*RUST_SYMBOL_RE),
        "Python" => Some(&*PYTHON_SYMBOL_RE),
        "Go" => Some(&*GO_SYMBOL_RE),
        "JavaScript" | "JSX" | "TypeScript" | "TSX" => Some(&*JS_SYMBOL_RE),
        "Markdown" => Some(&*MARKDOWN_HEADING_RE),
        _ => None,
    };
    let Some(expression) = expression else { return Vec::new() };
    let mut symbols = Vec::new();
    for captures in expression.captures_iter(text) {
        let symbol = captures.get(1).map(|value| value.as_str().trim()).unwrap_or_default();
        if symbol.len() >= 3 && !symbols.iter().any(|existing| existing == symbol) {
            symbols.push(symbol.to_string());
        }
    }
    symbols
}

fn stem_structural_term(term: &str) -> String {
    if let Some(base) = term.strip_suffix("ies").filter(|base| base.len() >= 3) {
        return format!("{base}y");
    }
    if let Some(mut base) = term.strip_suffix("ing").filter(|base| base.len() >= 4) {
        let bytes = base.as_bytes();
        if bytes.len() >= 2 && bytes[bytes.len() - 1] == bytes[bytes.len() - 2] {
            base = &base[..base.len() - 1];
        }
        return base.to_string();
    }
    if let Some(base) = term.strip_suffix("ed").filter(|base| base.len() >= 4) {
        let bytes = base.as_bytes();
        return if bytes.len() >= 2 && bytes[bytes.len() - 1] == bytes[bytes.len() - 2] {
            base[..base.len() - 1].to_string()
        } else {
            base.to_string()
        };
    }
    if !["ss", "us", "is"].iter().any(|suffix| term.ends_with(suffix)) {
        if let Some(base) = term.strip_suffix('s').filter(|base| base.len() >= 4) {
            return stem_structural_term(base);
        }
    }
    term.to_string()
}

fn unhelpful_structural_term(term: &str) -> bool {
    matches!(
        term,
        "all" | "and" | "any" | "base" | "buf" | "else" | "false" | "for" | "from"
            | "git" | "into" | "let" | "mod" | "none" | "pub" | "repo" | "root" | "self"
            | "some" | "std" | "str" | "then" | "the" | "this" | "true" | "unwrap" | "use"
            | "value" | "with"
    )
}

fn top_terms(tokens: &[String], language: &str, text: &str, limit: usize) -> Vec<String> {
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for token in tokens {
        let stemmed = stem_structural_term(token);
        if !unhelpful_structural_term(token) && !unhelpful_structural_term(&stemmed) {
            *counts.entry(stemmed).or_default() += 1;
        }
    }
    let mut ranked: Vec<(String, usize)> = counts.into_iter().collect();
    ranked.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(&right.0)));
    let mut result = structural_symbols(language, text);
    result.truncate(limit);
    for (term, count) in ranked {
        if result.len() >= limit {
            break;
        }
        if count >= 2 && !result.iter().any(|existing| existing == &term) {
            result.push(term);
        }
    }
    result
}

fn has_inline_tests(language: &str, text: &str) -> bool {
    match language {
        "Rust" => text.contains("#[cfg(test)]") || text.contains("#[test]"),
        "Go" => text.contains("func Test") || text.contains("func Benchmark"),
        "Python" => text.contains("def test_") || text.contains("class Test"),
        "JavaScript" | "JSX" | "TypeScript" | "TSX" => {
            text.contains("describe(") || text.contains("test(") || text.contains("it(")
        }
        "Swift" => text.contains("XCTestCase") || text.contains("@Test"),
        _ => false,
    }
}

fn configured_context_encoder(config: &Value) -> Result<CoreBPE> {
    let tokenizer_name = match config.pointer("/tokenization/context_tokenizer_name") {
        Some(Value::String(name)) if !name.trim().is_empty() => name.as_str(),
        Some(Value::String(_)) => {
            bail!("tokenization.context_tokenizer_name must not be empty")
        }
        Some(_) => bail!("tokenization.context_tokenizer_name must be a string"),
        None => "cl100k_base",
    };
    let encoder = match tokenizer_name {
        "cl100k_base" => cl100k_base(),
        "o200k_base" => o200k_base(),
        "o200k_harmony" => o200k_harmony(),
        "p50k_base" => p50k_base(),
        "p50k_edit" => p50k_edit(),
        "r50k_base" => r50k_base(),
        unsupported => {
            bail!(
                "unsupported tokenization.context_tokenizer_name {unsupported:?}; \
                 supported encodings: cl100k_base, o200k_base, o200k_harmony, \
                 p50k_base, p50k_edit, r50k_base"
            )
        }
    };
    encoder.with_context(|| format!("failed to initialize {tokenizer_name} tokenizer"))
}

fn action_queue(
    files: &[FileAnalysis],
    history_evidence_reliable: bool,
    config: &Value,
) -> Vec<Value> {
    let mut files: Vec<&FileAnalysis> = files.iter().collect();
    files.sort_by(|left, right| {
        right
            .slop_score
            .total_cmp(&left.slop_score)
            .then_with(|| right.tokens.cmp(&left.tokens))
            .then_with(|| left.path.cmp(&right.path))
    });
    files
        .into_iter()
        .filter(|file| {
            let profile_minimum_score = if config.pointer("/health/profile_threshold_policy").and_then(Value::as_str) == Some("per_profile") {
                config.pointer(&format!("/health/profile_queue_minimum_score/{}", file.profile)).and_then(Value::as_f64).unwrap_or_default()
            } else { 0.0 };
            file.slop_score >= profile_minimum_score && (!file.reason_codes.is_empty()
                || matches!(file.context_band.as_str(), "warning" | "critical")
                || matches!(file.slop_band.as_str(), "high" | "critical"))
        })
        .map(|file| {
            let non_context_reasons = file.reason_codes.iter().any(|reason| {
                !matches!(reason.as_str(), "critical_token_cost" | "high_token_cost")
            });
            let synchronization_group = (file.path.starts_with("schemas/")
                || file.path == "Cargo.toml"
                || file.path == "Cargo.lock"
                || file.path == "action.yml"
                || file.path == "CHANGELOG.md"
                || file.path.starts_with("docs/")
                || file.path == "README.md")
                .then_some("release-version-sync");
            let remediation_kind = match file.classification.as_str() {
                "generated" => "generator_source_investigation",
                "snapshot" | "fixture" | "migration_fixture" => "fixture_strategy_investigation",
                "vendored" => "upstream_dependency_investigation",
                _ => "source_intervention",
            };
            let remediation_target_paths = match file.classification.as_str() {
                "generated" if !file.generated_from.is_empty() => file.generated_from.clone(),
                _ => vec![file.path.clone()],
            };
            let next_action_path = remediation_target_paths
                .first()
                .map(String::as_str)
                .unwrap_or(file.path.as_str());
            json!({
                "path": file.path,
                "profile": file.profile,
                "classification": file.classification,
                "generated_from": file.generated_from,
                "synchronization_group": synchronization_group,
                "remediation_kind": remediation_kind,
                "remediation_target_paths": remediation_target_paths,
                "slop_score": file.slop_score,
                "slop_band": file.slop_band,
                "context_band": file.context_band,
                "tokens": file.tokens,
                "age_days": file.age_days,
                "revisions_window": file.revisions_window,
                "churn_pressure": file.churn_pressure,
                "reason_codes": file.reason_codes,
                "is_pure_context_hotspot": !file.reason_codes.is_empty() && !non_context_reasons,
                "severity": if matches!(file.context_band.as_str(), "critical") || matches!(file.slop_band.as_str(), "critical") { "error" } else if matches!(file.context_band.as_str(), "warning") || matches!(file.slop_band.as_str(), "high") { "warning" } else { "notice" },
                "evidence_status": if history_evidence_reliable && file.revisions_window >= 5 { "supported" } else { "low_support" },
                "next_action": format!("git slop explain --path {}", next_action_path)
            })
        })
        .collect()
}
