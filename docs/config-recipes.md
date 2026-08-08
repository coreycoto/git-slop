# Configuration Recipes

All examples are partial overrides. Inspect defaults with `git slop config show --effective`.

## Small application

```yaml
schema_version: 2
organization:
  candidate_file_limit: 250
```

## Mature monorepo

```yaml
schema_version: 2
organization:
  max_commit_files: 150
  max_temporal_edges: 8000
resources:
  memory_budget_mb: 2048
output:
  retention_runs: 30
```

Use `git slop find --scope packages/example`; Git evidence remains repository-wide.

## Generated or vendor-heavy repository

```yaml
schema_version: 2
inventory:
  ignore_globs: [dist/**, vendor/**, "**/*.min.js"]
resources:
  large_file_bytes: 1048576
```

## Documentation repository

```yaml
schema_version: 2
tokenization:
  context_bands:
    compact_max_tokens: 5000
    healthy_max_tokens: 12000
    warning_max_tokens: 18000
```

## Strict CI ratchet

Generate a compatible base report, pass it as the Action's `baseline-report`, and use `compare --fail-on-regression` locally. This gates new or worsened evidence without requiring the existing backlog to disappear first.

## Fast local versus full-history

Use `--scope` and a smaller candidate limit for local feedback. Use a full clone, the normal candidate limit, and the same tokenizer/config digest for authoritative comparisons. Changing the tokenizer invalidates baselines because token counts and context bands are no longer comparable.

## Supported tokenizers and baseline compatibility

`tokenization.context_tokenizer_name` accepts `cl100k_base` (the default),
`o200k_base`, `o200k_harmony`, `p50k_base`, `p50k_edit`, or `r50k_base`.
The tokenizer name participates in analyzer provenance and the content-addressed
token cache. A tokenizer change deliberately invalidates cached counts and makes
reports comparison-incompatible unless `compare --force` is explicitly used.
