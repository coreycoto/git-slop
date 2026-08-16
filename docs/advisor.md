# Policy-guided local advisor

`git slop advise` is an optional, non-mutating interpretation layer over an
existing current report. The deterministic detector remains authoritative:
`find`, default `health`, `check`, `explain`, and `plan` never start a model,
read advice, install policies, or change behavior based on model output.

## Flow and trust zones

```text
current schema-5 report
  -> deterministic explain and plan candidate slices
  -> bounded tracked guidance/source/test excerpts
  -> built-in plus selected locked policies
  -> explicit local provider endpoint
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

`--runtime-context-tokens` controls the provider's total input-plus-output
window. It defaults to the configured input and output token budgets combined
and is sent as native Ollama `num_ctx`, preventing Ollama's smaller default
window from silently truncating an 8K advice input.

## Local Safeguard provider

V1 accepts only the canonical reference identity
`openai/gpt-oss-safeguard-20b`. The public binary contains no model weights or
inference engine and never downloads, starts, updates, or discovers a model.
An explicitly configured OpenAI-compatible chat-completions server performs
non-streaming inference. The reference adapter keeps system, developer, and
user trust zones separate and uses strict JSON-schema output.

Ollama publishes a 20B Safeguard model and an OpenAI-compatible API. After
separately installing it and reviewing its approximately 14 GB model-storage
requirement, start the model using the official runtime instructions, then run:

```sh
git slop advise --top 1 \
  --endpoint http://127.0.0.1:11434/v1/chat/completions \
  --runtime-model gpt-oss-safeguard:20b \
  --runtime-label ollama \
  --model-digest <immutable-local-model-digest> \
  --reasoning medium
```

The runtime alias is separate from the canonical reference identity and is
recorded in provenance. The request uses system, developer, and user roles,
the strict published response schema, a bounded output token count, and no
streaming. See the [official OpenAI Safeguard guide](https://cookbook.openai.com/articles/gpt-oss-safeguard-guide)
and [official Ollama model page](https://ollama.com/library/gpt-oss-safeguard).

For reproducible local performance measurements, the optional native Ollama
adapter sends the same trust-zone messages and strict schema to `/api/chat` and
records Ollama's load, prompt-evaluation, and generation timings:

```sh
git slop advise --top 1 \
  --provider ollama \
  --endpoint http://127.0.0.1:11434/api/chat \
  --runtime-model gpt-oss-safeguard:20b \
  --runtime-label 'ollama <exact-version>' \
  --model-digest <immutable-local-model-digest>
```

Provider choice never changes detector behavior. The benchmark uses the native
adapter by default so prompt and generation rates are measured rather than
inferred from total wall time; use `--provider openai-compatible` to exercise
the portable compatibility boundary instead.

Loopback is the default privacy boundary. A non-loopback host fails unless
`--allow-remote` is explicit, and V1 accepts only plain `http://`; therefore a
remote opt-in is suitable only on a separately secured trusted network. The
endpoint cannot contain embedded credentials, a query, or a fragment. Git Slop
does not log full model input or chain-of-thought. Markdown contains concise
policy rationales only.

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
and size limits, token usage when returned, separate context/provider/
validation timing, per-rule evaluations, citations, uncertainty, validation
warnings, and the non-mutation boundary.

Validate and render an existing artifact only against a current matching
report:

```sh
git slop advise --validate-artifact .slop/advice/latest/advice.json
```

A stale report or digest mismatch is rejected visibly. Advice exits successfully
only after schema and reference validation, but it remains advisory: it cannot
edit files, Git, GitHub, configuration, policy selection, reports, or checks.

See the [Safeguard-only V1 benchmark](benchmarks/safeguard-v1.md) for the
privacy-safe gold corpus, fixed matrix, measurements, and ship gate.
