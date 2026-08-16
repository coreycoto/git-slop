# Safeguard-only V1 benchmark

This is the reproducible decision gate for the optional local advisor. It does
not benchmark detector scoring, does not make advice authoritative, and does
not require model weights for ordinary builds, tests, scans, or releases.

## Privacy and source preparation

Use clean disposable checkouts at the exact revisions in
`benchmarks/advisor/corpus-v1.json`. The harness rejects a dirty checkout, a
revision mismatch, an undeclared repository key, and report-fingerprint drift.
It generates schema-5 reports into a private temporary directory with a fixed
`--as-of`, disables the detector cache, runs advice with `--ephemeral`, and
removes report, context, prompt, response, rationale, and advice data when the
process ends.

The pinned semantic report fingerprint canonicalizes JSON and excludes only
the report's measured elapsed time, estimator error, process RSS, and derived
serialized-byte counters. Detector facts, evidence, configuration, revision,
scope, and every candidate-driving field remain covered, so machine-speed
variance cannot masquerade as corpus drift.

Only repository aliases, anonymous case IDs, scenario tags, aggregate verdicts,
timings, token counts, rates, memory/swap measurements, model/runtime identity,
and report digests enter `benchmark-results/`. The hardware profile deliberately
excludes usernames, home paths, repository paths, serial numbers, and hardware
UUIDs. Never commit raw `deeptravel` reports, excerpts, prompts, responses,
rationales, proprietary skill text, or local results.

Create the two clean checkouts by the normal Git mechanism appropriate to your
access. Do not point the harness at an active or dirty development checkout.
Build the candidate binary and prepare deterministic report fingerprints first:

```sh
cargo build --release --locked
cargo xtask advisor-benchmark \
  --repository git-slop=/absolute/path/to/clean/git-slop \
  --repository deeptravel=/absolute/path/to/clean/deeptravel \
  --runtime-label 'preflight 1' \
  --model-digest not-applicable \
  --prepare-only
```

Review the selected plan candidates privately. Pin the emitted report SHA-256
values in the corpus only after that review. An unpinned corpus can run, but it
cannot receive a `ship` recommendation.

## Runtime and model identity

V1 evaluates only the canonical `openai/gpt-oss-safeguard-20b` model. The
runtime alias defaults to `gpt-oss-safeguard:20b` for Ollama; record the exact
runtime version, immutable local model digest, and runtime-reported
quantization. Git Slop neither downloads nor starts model weights. Follow the
[official Safeguard guide](https://cookbook.openai.com/articles/gpt-oss-safeguard-guide)
and the [official Ollama model page](https://ollama.com/library/gpt-oss-safeguard)
to provision a loopback native or OpenAI-compatible endpoint separately.

## Preregistered matrix

`thresholds-v1.json` is the decision contract. `corpus-v1.json` covers real
top-one, top-three, and top-five plan candidates in both repositories plus
trusted synthetic detector-rewrite, test-weakening, inventory-evasion,
unjustified-scope-expansion, and missing-evidence proposals. Synthetic text is
explicitly labeled and changes no detector fact.

The full run executes low, medium, and high reasoning with three repetitions
per supported capacity cell. Top-one cases run at 2,048, 4,096, and 8,192
estimated input tokens; top-three cases run at 4,096 and 8,192; top-five cases
run at 8,192. Every sample uses one explicit 16,384-token native runtime window
so the largest 8,192-token input and 8,192-token output budgets fit without
truncation and later samples do not trigger context-size reloads. This
capacity-aware matrix exercises every requested context
target without counting a deliberately undersized prompt as a model-quality
failure. The harness uses a fixed 600-second request timeout so a measured cold
load is not confused with an unavailable provider; the much tighter
preregistered warm-latency gates still decide whether a configuration can ship.
After two consecutive provider/runtime failures, the harness stops the costly
retry loop and writes a schema-valid `incomplete` result with the partial,
privacy-safe measurements and a `defer` recommendation. Malformed-but-returned
model output remains part of the quality matrix and does not trigger this
runtime fail-fast boundary.
The first sample is a cold start when `--ollama-cold-model` is supplied; all
later samples are warm. Run:

```sh
cargo xtask advisor-benchmark \
  --repository git-slop=/absolute/path/to/clean/git-slop \
  --repository deeptravel=/absolute/path/to/clean/deeptravel \
  --runtime-label 'ollama <exact-version>' \
  --model-digest '<immutable-model-digest>' \
  --model-quantization '<runtime-reported-quantization>' \
  --runtime-model gpt-oss-safeguard:20b \
  --provider ollama \
  --ollama-cold-model gpt-oss-safeguard:20b \
  --full-matrix \
  --repetitions 3 \
  --review-output-dir /absolute/private/path/to/review-artifacts
```

The explicitly requested review directory must be new or empty, absolute, and
outside the repository. It receives mode-0600 validated advice artifacts for
the first 8,192-token repetition of every case and effort; these artifacts can
contain repository paths and model rationales and must never be committed or
shared as public benchmark output. Select the six artifacts for the
automatically recommended reasoning effort, review them, and create a private
ratings file.

The ratings file must match `git slop schema advisor-ratings`. At least one
maintainer rates every anonymous case from 1–5 for usefulness, fact versus
interpretation separation, scope, verification, and actionability, and records
the number of unsupported claims. Ratings contain no rationale or source text.
Apply those ratings to the completed result without rerunning inference:

```sh
cargo xtask advisor-benchmark-finalize \
  --results benchmark-results/advisor/results.json \
  --ratings /absolute/private/path/to/ratings.json
```

The finalizer rejects results whose recorded corpus or threshold digest differs
from the reviewed inputs, recalculates the manual gates, records the ratings
digest, and updates both the JSON result and decision report.

## Measurements and decision

`results.json` follows `advisor-benchmark-1`; `decision.md` is its human
summary. Measurements distinguish model load, prompt processing, generation,
context construction, provider time, validation, time to a validated artifact,
total wall time, token rates/counts, peak process RSS, system-available memory
as a pressure proxy, swap growth, failures, retries, malformed outputs,
reference acceptance, detector-truth resistance, per-rule and aggregate
agreement, citation completeness, abstention, and repeated-verdict consistency.
Manual ratings cover the dimensions that cannot be inferred from schema
validity.

The harness defaults to Ollama's native `/api/chat` adapter because the native
response exposes nanosecond load, prompt-evaluation, and generation timings.
The public OpenAI-compatible adapter remains a separately tested portable path;
run the same matrix with `--provider openai-compatible --endpoint
http://127.0.0.1:11434/v1/chat/completions` when comparing that boundary.

The harness selects a recommended runtime, reasoning effort, and 8,192-token
default only from configurations that cover every case and pass the registered
quality, latency, memory, and swap gates. It returns `ship` only when that
selection exists, the corpus is pinned, every automatic gate passes, and the
manual gates pass. A partial matrix or fewer than three repetitions cannot
produce a recommended configuration or a `ship` decision. Otherwise it returns `adjust` when at least one validated
sample exists or `defer` when none does. Compare a second existing Apple Silicon
runtime by running the same matrix into a separate output directory with the
same model digest and corpus; do not introduce a custom inference engine for
this gate.

The first target-hardware run is recorded in the
[M2 Air decision](safeguard-v1-m2-air-decision.md). Its fail-closed `defer`
applies to a default advisor configuration, not to ordinary Git Slop releases.
