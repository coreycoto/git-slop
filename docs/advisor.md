# Policy-guided local advisor

`git slop advise` is an optional, non-mutating interpretation layer over an
existing current report. The deterministic detector remains authoritative:
`find`, default `health`, `check`, `explain`, and `plan` never start a model,
read advice, install policies, or change behavior based on model output.

> Public inference status: **disabled**. The checked-in benchmark recommendation
> is `defer` after the reference model exhausted a 16-GB M2 Air. The supported
> product path is provider-free context. Inference research is confined to the
> capacity-gated maintainer benchmark on a separately provisioned host.

## Flow and trust zones

```text
current schema-5 report
  -> deterministic explain and plan candidate slices
  -> bounded tracked guidance/source/test excerpts
  -> built-in plus selected locked policies
  -> provider-free context.json (supported public boundary)
  -> optional release-gated maintainer benchmark provider
  -> strict schema-1 provider response
  -> reference validation and deterministic verdict aggregation
  -> separate advice.json and advice.md artifacts
```

System/output instructions, the non-disableable core pack, third-party policy,
candidate facts, and untrusted repository excerpts remain distinct. Files must
be tracked regular UTF-8 files inside the worktree, may not traverse symlinks,
and are checked against report content digests when available. Every excerpt
records its selection reason, line range, source and excerpt digests, byte
counts, and truncation status. Context selection is deterministic and bounded
by files, bytes, and an `o200k_harmony` token estimate.

Candidate interpretations remain structurally separate from observed report
facts. Candidate and excerpt IDs are content-derived. The context digest is
computed over the complete provider-independent input except its own digest
and the derived token-count field; both are recomputable. Identical inputs
therefore produce identical compact JSON bytes in the content-addressed
context cache.

## Inspect input without inference

```sh
git slop advise --path src --context-only --format json
git slop advise --relationship <relationship-id> --context-only --format json
git slop advise --cluster <cluster-id> --context-only --format json
git slop advise --top 5 --context-only --format json
```

`--context-only --format json` remains the explicit spelling. Both options are
now the defaults when `--infer` is absent, so `git slop advise --top 5` produces
the same provider-independent JSON. No provider configuration is read and no
network connection is attempted. Provider-free construction does not evaluate
the inference release gate.

Stable `git slop advise --help` shows only provider-free context construction
and artifact validation. The disabled inference, provider, model, resource,
timeout, and mock-response controls are hidden because they are not supported
product workflows. `git slop doctor` independently reports that provider-free
context is available, no model is required for ordinary use, and public
inference is disabled.

`--top` preserves intervention ranking first and fills any remaining requested
slots from the report's ranked health refactor candidates. It never promotes
ordinary observations into plan candidates.

Use `--ephemeral` to suppress context-cache and advice-state writes for an
explicit disposable run. The release benchmark uses this mode; ordinary advice
retains its validated artifacts by default.

Use `--max-context-tokens`, `--max-context-bytes`, `--excerpt-bytes`, and
`--max-slices` to make the boundary smaller. Missing, stale, untracked,
ignored, binary, undecodable, escaping, or oversized required evidence fails
before inference. Optional unavailable guidance is skipped and budget
omissions are recorded as missing evidence.

At the minimum 2,048-token boundary, the builder first removes excerpts, then
may compact redundant detail for one candidate while retaining every policy,
candidate identity, trust zone, and complete reference index. The input marks
that compaction as truncation and adds `candidate-context` to missing evidence;
the model must abstain or revise when the compact input cannot support a
verdict.

Within the separately controlled benchmark, `--runtime-context-tokens` controls
the provider's total input-plus-output window. It defaults to the configured
input and output token budgets combined and is sent as native Ollama `num_ctx`,
preventing a smaller runtime default from silently truncating an 8K advice
input.

## Experimental inference gate

V1 accepts only the canonical reference identity
`openai/gpt-oss-safeguard-20b`. The public binary contains no model weights or
inference engine and never downloads, starts, updates, or discovers a model.
The embedded [release gate](../benchmarks/advisor/release-gate.json) records the
current `defer` decision and keeps public inference disabled. `--infer` fails
before report access, context-cache writes, or provider contact unless the
binary was explicitly built with the non-default
`advisor-inference-benchmark` feature and the command is running inside the
maintainer benchmark harness. Official release binaries never enable that
feature. The release tooling validates that only a complete `ship` decision
can enable the public boundary.

Even in that separately built benchmark lane, provider configuration and the
zero-data probe occur only after report currentness, artifact mode, selected
policies, and bounded context have passed local validation. A locally invalid
request therefore cannot contact the configured loopback service.

The benchmark has no provider, endpoint, model, or runtime-model defaults. It
requires an explicit canonical model, immutable digest, artifact size,
conservative peak-memory estimate, runtime state, and dedicated-host
confirmation. Before provider contact it measures physical and available
memory plus swap, enforces at least 24 GiB of physical memory and 24 GiB of
available capacity for the current model estimate, rejects more than 256 MiB
of swap already in use, and prints the complete resource contract. Those
minimums are enforced in both the runtime and the private harness even if the
checked-in gate is weakened. A continuous watchdog closes the request when the
8-GiB available-memory reserve or 256-MiB swap-growth boundary is crossed.

Maintainers must run the capacity-only preflight before preparing a report or
provisioning a provider:

```sh
cargo xtask advisor-capacity \
  --model openai/gpt-oss-safeguard-20b \
  --model-size-bytes 13793441254 \
  --estimated-peak-memory-bytes 17179869184 \
  --format json
```

It reads only host memory and swap state. Its receipt records
`provider_contacted: false` and `report_accessed: false`, and it exits nonzero
when the host is ineligible. Every blocker is returned with a stable code,
actual byte count, comparison direction, limit, and message instead of stopping
at the first failure. `git slop schema advisor-capacity` prints the strict
`advisor-capacity-1` receipt contract. This is therefore the only advisor
capacity command that should be run on the recorded 16-GB M2 Air; do not use
that machine for the full benchmark.

Git Slop never manages the provider runtime. In particular, neither the public
CLI nor the benchmark runs `ollama serve`, `ollama pull`, `ollama stop`, package
installation, or model deletion. Runtime provisioning and recovery remain an
operator responsibility outside Git Slop. The benchmark's explicit
`--initial-runtime-state` only records whether that separately controlled
runtime was already warm; it does not change the state.

Only loopback `http://` endpoints are accepted. Remote endpoints are refused
because V1 has no authenticated TLS transport; there is no `--allow-remote`
escape hatch. Endpoint credentials, queries, fragments, whitespace, and header
injection are rejected. A short zero-data TCP probe has its own connection
timeout before any repository context is sent. That timeout is one deadline
across every resolved loopback address, not a fresh allowance per address.
Model loading and generation share a separately displayed total timeout, emit
phase progress, and can be cancelled with Ctrl-C.

Provider responses must identify the exact requested served model and report a
normal stopped completion (`finish_reason: stop` for OpenAI-compatible
responses or `done: true` with `done_reason: stop` for native Ollama). The HTTP
reader honors complete `Content-Length` and chunked framing without waiting for
the provider to close a keep-alive connection, and rejects conflicting or
ambiguous framing before response validation. It formats IPv6 loopback Host
headers correctly, rejects invalid ports, oversized declared bodies,
unsupported transfer/content encodings, non-JSON successful responses, and
trailing chunk bytes before the provider envelope is accepted.

Provider HTTP error bodies are intentionally excluded from diagnostics because
they can echo prompts or repository content. Stored provenance does not retain
endpoint paths or arbitrary response metadata: token usage is reduced to
numeric prompt, completion, and total counts, while a system fingerprint is
kept only when it is a bounded safe identifier. Runtime model and label values
must also satisfy bounded privacy-safe identifier contracts before contact.

See the [official OpenAI Safeguard guide](https://cookbook.openai.com/articles/gpt-oss-safeguard-guide),
the [benchmark protocol](benchmarks/safeguard-v1.md), and the [resource-safety
and recovery guide](troubleshooting/advisor-resource-safety.md). Do not run the
reference model on a 16-GB M2 Air.

## Validated outputs

Every candidate must evaluate every applicable rule. Every material rationale
must cite identifiers from the supplied reference index, and each rule must
cite itself. Unknown or duplicate candidates, rules, paths, findings,
relationships, clusters, excerpts, policies, and verification references fail.
The response schema bounds rationales, revisions, assumptions, missing
evidence, recommendations, citations, and candidate count. Git Slop ignores a
model's aggregate as authority and recomputes `reject > revise > abstain >
approve`.

Successful runs write outside the canonical detector bundle:

```text
.slop/advice/latest/advice.json
.slop/advice/latest/advice.md
.slop/advice/runs/<timestamp-content-id>/advice.json
.slop/advice/runs/<timestamp-content-id>/advice.md
```

Before adoption, the same paths live in Git-private active state. Artifacts
record report/revision/dirty-state digests, selector and candidates, context
builder and digest, policy locks, provider/runtime/model identity, reasoning
and size limits, allowlisted token usage when returned, endpoint classification,
separate context/provider/validation timing, per-rule evaluations, citations,
uncertainty, validation warnings, and the non-mutation boundary. They do not
retain the provider endpoint itself.

The Markdown view starts with a decision summary: the aggregate disposition,
candidate counts by verdict, required-revision and missing-evidence totals, and
the number of low-confidence candidates. Each candidate then shows confidence,
its disposition, cited evidence, rule results, requested revisions, next step,
assumptions, and missing evidence. It also warns that repository-derived
evidence and provider rationale are private retained state; use
`git slop prune --dry-run` to review retention before removing anything.

Advice state is owner-private (`0700` directories and `0600` files on Unix),
written through fsynced temporary directories, and replaced under an exclusive
write lock. An interrupted `latest` replacement is recovered before the next
write; `git slop doctor` reports retained-run counts, permissions, stale
artifacts, and any recovery entry. `git slop prune --dry-run` previews both
detector and advice retention with the configured run and byte limits, while
`--yes` removes only immutable historical runs and preserves `advice/latest`.

Validate and render an existing artifact only against a current matching
report:

```sh
git slop advise --validate-artifact .slop/advice/latest/advice.json
```

A stale report or digest mismatch is rejected visibly. Loading an artifact also
rechecks candidate and policy identity, citations, policy completeness, warning
parity, and every candidate and overall aggregate instead of trusting stored
validation flags. Advice exits successfully only after schema and reference
validation, but it remains advisory: it cannot
edit files, Git, GitHub, configuration, policy selection, reports, or checks.

Private benchmark results self-digest every sample and bind the source advice
artifact. An explicitly requested review directory receives blinded
multi-repetition artifacts, a reviewer-facing opaque index, and a withheld
mapping manifest. Finalization requires two independent reviewers, verifies
the complete private evidence chain, previews by default, and writes new
finalized outputs only with `--apply`; the completed source result remains
immutable.

Every benchmark terminal receipt reports review evidence as `not_applicable`,
`not_retained`, or `retained` with an actionable warning. A retained result
names the blinded-review protocol without exposing its private directory in
machine-readable output; human output repeats the operator-supplied path and
reminds maintainers which manifest must remain withheld.

See the [Safeguard-only V1 benchmark](benchmarks/safeguard-v1.md) for the
privacy-safe gold corpus, fixed matrix, measurements, and ship gate.
