# Git Slop CLI Reference: `advise`

Generated from the live Clap command tree.

## `git-slop advise`

Build provider-free policy context; inference remains release-gated and experimental

**Usage**

```text
Usage: git-slop advise [OPTIONS]
```

**Machine contract:** `advice-input-1` for provider-free context; `advice-1` for release-gated validated advice.

| Argument | Value | Default | Constraints | Description |
| --- | --- | --- | --- | --- |
| `--report` | `REPORT` | `-` | - | Report path. Advice always requires this report to match the current worktree |
| `--path` | `PATH` | `-` | conflicts: --validate-artifact; exclusive group: path, cluster, relationship, top | Repo-relative file or folder path |
| `--relationship` | `RELATIONSHIP` | `-` | conflicts: --validate-artifact; exclusive group: path, cluster, relationship, top | Relationship identifier |
| `--cluster` | `CLUSTER` | `-` | conflicts: --validate-artifact; exclusive group: path, cluster, relationship, top | Cluster identifier |
| `--top` | `TOP` | `-` | conflicts: --validate-artifact; exclusive group: path, cluster, relationship, top | Evaluate the top N deterministic interventions, then health refactor candidates |
| `--policy` | `POLICIES` | `-` | conflicts: --validate-artifact | Evaluate only this already-locked pack or rule in addition to all core invariants |
| `--context-only` | `flag` | `-` | conflicts: --validate-artifact | Emit byte-stable provider-independent advice input without model inference |
| `--infer` | `flag` | `-` | conflicts: --context-only, --validate-artifact | Explicitly request experimental model inference after every safety gate passes |
| `--ephemeral` | `flag` | `-` | conflicts: --validate-artifact | Avoid context-cache and advice-state writes; useful for disposable benchmarks |
| `--validate-artifact` | `VALIDATE_ARTIFACT` | `-` | - | Validate and render an existing advice artifact against the current selected report |
| `--provider` | `PROVIDER` | `-` | values: openai-compatible, ollama, mock; conflicts: --validate-artifact | Explicit out-of-process reasoning provider. Required with --infer |
| `--endpoint` | `ENDPOINT` | `-` | conflicts: --validate-artifact | Explicit loopback provider endpoint. Remote endpoints are refused in V1 |
| `--model` | `MODEL` | `-` | conflicts: --validate-artifact | Explicit model identity. V1 accepts only openai/gpt-oss-safeguard-20b |
| `--runtime-model` | `RUNTIME_MODEL` | `-` | conflicts: --validate-artifact | Explicit provider-specific served-model name |
| `--runtime-label` | `RUNTIME_LABEL` | `-` | conflicts: --validate-artifact | Explicit human-readable runtime name and version recorded in provenance |
| `--model-digest` | `MODEL_DIGEST` | `-` | conflicts: --validate-artifact | Exact model artifact digest or immutable runtime model revision |
| `--model-size-bytes` | `MODEL_SIZE_BYTES` | `-` | conflicts: --validate-artifact | Exact model artifact size used by the fail-closed capacity preflight |
| `--estimated-peak-memory-bytes` | `ESTIMATED_PEAK_MEMORY_BYTES` | `-` | conflicts: --validate-artifact | Conservative peak host-memory estimate for the configured model and context |
| `--confirm-resources` | `flag` | `-` | conflicts: --validate-artifact | Confirm the displayed resource contract after independently reviewing it |
| `--reasoning` | `REASONING` | `medium` | values: low, medium, high; conflicts: --validate-artifact | Reasoning effort supplied to the explicitly selected provider |
| `--connect-timeout-seconds` | `CONNECT_TIMEOUT_SECONDS` | `5` | conflicts: --validate-artifact | Provider connection timeout in seconds |
| `--timeout-seconds` | `TIMEOUT_SECONDS` | `120` | conflicts: --validate-artifact | Total model-load and generation timeout in seconds |
| `--max-response-bytes` | `MAX_RESPONSE_BYTES` | `1048576` | conflicts: --validate-artifact | Maximum accepted provider response size in bytes |
| `--max-output-tokens` | `MAX_OUTPUT_TOKENS` | `2048` | conflicts: --validate-artifact | Maximum generated output tokens requested from the provider |
| `--runtime-context-tokens` | `RUNTIME_CONTEXT_TOKENS` | `-` | conflicts: --validate-artifact | Total provider context window. Defaults to the input and output token budgets combined |
| `--max-context-bytes` | `MAX_CONTEXT_BYTES` | `131072` | conflicts: --validate-artifact | Maximum provider-independent context size in bytes |
| `--max-context-tokens` | `MAX_CONTEXT_TOKENS` | `8192` | conflicts: --validate-artifact | Maximum estimated o200k_harmony input tokens |
| `--excerpt-bytes` | `EXCERPT_BYTES` | `4096` | conflicts: --validate-artifact | Maximum bytes included from each repository file |
| `--max-slices` | `MAX_SLICES` | `3` | conflicts: --validate-artifact | Maximum plan slices generated for one non-top selector |
| `--mock-response` | `MOCK_RESPONSE` | `-` | conflicts: --validate-artifact | Structured mock response used only with --provider mock |
| `--format` | `FORMAT` | `-` | values: markdown, json | Render context as JSON or validated advice as Markdown/JSON. Context defaults to JSON |
| `--output` | `OUTPUT` | `-` | - | Also write the selected rendering to this repo-relative or absolute path |

**Example**

```sh
git slop advise --top 1
```
