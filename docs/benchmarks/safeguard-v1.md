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
process ends. On Unix, that new workspace is mode 0700. Each matrix cell and
repetition gets a distinct non-reusable artifact path inside it.

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
Build the ordinary provider-free candidate binary and prepare deterministic
report fingerprints first:

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

Before building the inference lane, check only the proposed host's capacity:

```sh
cargo xtask advisor-capacity \
  --model openai/gpt-oss-safeguard-20b \
  --model-size-bytes 13793441254 \
  --estimated-peak-memory-bytes 17179869184 \
  --format json
```

This command does not read a report, build context, connect to a provider, or
manage a runtime. The machine-readable receipt explicitly records
`provider_contacted: false` and `report_accessed: false`; an ineligible host
receives every blocker and a nonzero exit. Each blocker carries a stable code,
actual bytes, comparison direction, limiting bytes, and a human message. The
receipt must match `git slop schema advisor-capacity`. This is the safe capacity
check for a low-memory development machine. Do not replace it there with
`advisor-benchmark`.

Build the inference lane only on the dedicated benchmark host with the
non-default feature:

```sh
cargo build --release --locked --features advisor-inference-benchmark
```

Official release binaries never enable that feature.

## Runtime and model identity

V1 evaluates only the canonical `openai/gpt-oss-safeguard-20b` model. The
benchmark has no provider, endpoint, canonical-model, or runtime-model
defaults. Record each explicitly along with the exact runtime version,
immutable model digest, model artifact size, conservative peak-memory
estimate, and runtime-reported quantization. Git Slop neither downloads nor
starts model weights. Follow the
[official Safeguard guide](https://cookbook.openai.com/articles/gpt-oss-safeguard-guide)
and the [official Ollama model page](https://ollama.com/library/gpt-oss-safeguard)
to provision a loopback native or OpenAI-compatible endpoint separately on a
dedicated host. Do not provision it on the recorded 16-GB M2 Air.

Before preparing repository context or contacting a provider, the inference
lane requires `--confirm-dedicated-host`, measures physical and available
memory and swap, validates the supplied model and peak-memory sizes against the
checked-in release gate, and prints the complete capacity contract. A
host is rejected when more than 256 MiB of swap is already in use. The runtime
and harness also enforce the 20B model, 16-GiB peak estimate, 24-GiB physical
memory, 8-GiB reserve, and 256-MiB swap boundaries as hard safety floors even
if a checked-in gate is weakened. A continuous 250-ms watchdog aborts the
client request if available memory drops below the reserve, swap grows past the
fixed limit, or either measurement becomes unavailable. A resource abort
terminates the matrix immediately and is not retried.

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
The harness never starts, stops, installs, unloads, or otherwise manages a
provider runtime. The operator must set `--initial-runtime-state cold` or
`warm` to record the separately verified state before the first sample; all
later samples are treated as warm. Child stdout and stderr are drained while
each sample runs so a full pipe cannot deadlock the watchdog or timeout path.
Each stream retains at most 8 MiB; crossing that boundary closes the child and
terminates the matrix with `benchmark_child_output_limit` instead of consuming
unbounded harness memory. Run only on the dedicated host:

```sh
cargo xtask advisor-benchmark \
  --repository git-slop=/absolute/path/to/clean/git-slop \
  --repository deeptravel=/absolute/path/to/clean/deeptravel \
  --confirm-dedicated-host \
  --initial-runtime-state cold \
  --provider ollama \
  --endpoint http://127.0.0.1:11434/api/chat \
  --model openai/gpt-oss-safeguard-20b \
  --runtime-model gpt-oss-safeguard:20b \
  --runtime-label 'ollama <exact-version>' \
  --model-digest '<immutable-model-digest>' \
  --model-quantization '<runtime-reported-quantization>' \
  --model-size-bytes '<verified-model-bytes>' \
  --estimated-peak-memory-bytes '<conservative-peak-bytes>' \
  --full-matrix \
  --repetitions 3 \
  --review-output-dir /absolute/private/path/to/review-artifacts
```

The explicitly requested review directory must be new or empty, absolute, and
outside the repository. Every valid 8,192-token repetition is transformed into
a mode-0600 blinded review artifact. Provider, runtime, reasoning-effort, and
repetition identity are removed from the artifact itself. The directory also
receives `blind-review-index.json`, which is safe to give reviewers, and a
private `review-manifest.json`, which maps opaque review IDs back to exact
sample and source-artifact digests. Keep that mapping from reviewers until
their independent ratings are complete. None of this private evidence may be
committed or shared as public benchmark output.

Terminal human output repeats the private path and sharing boundary after the
run finishes. The JSON operation receipt instead exposes a strict
`review_evidence` state and warning without leaking that path. A run without
`--review-output-dir` is marked `not_retained` and cannot be finalized; it must
not be mistaken for review-ready evidence.

Each returned response must name the exact requested served model and report a
normal stopped completion. The client accepts a complete `Content-Length` or
chunked HTTP response without waiting for the loopback server to close a
keep-alive socket. It rejects conflicting or oversized framing, compressed or
unsupported encodings, non-JSON successful responses, and trailing chunk data.
IPv6 loopback request Host headers retain the required brackets. Missing or
mismatched served-model identity terminates the matrix immediately; evidence
from that runtime is not accepted or retried.

The ratings file must match `git slop schema advisor-ratings-v2`. At least two
independent reviewers each rate every blinded artifact for the automatically
recommended effort, including at least two repetitions of every anonymous
case. Each rating covers usefulness, fact-versus-interpretation separation,
scope, verification, actionability, and unsupported-claim count. Ratings bind
the immutable source-result and review-manifest digests and contain no
rationale or source text.

Finalization is preview-only by default. It validates all evidence and prints
the proposed destinations without writing or rerunning inference:

```sh
cargo xtask advisor-benchmark-finalize \
  --results benchmark-results/advisor/results.json \
  --review-manifest /absolute/private/path/to/review-artifacts/review-manifest.json \
  --ratings /absolute/private/path/to/ratings.json
```

After reviewing that preview, repeat the command with `--apply`. The command
writes `finalized-results.json` and `finalized-decision.md`; it never mutates
`results.json` or `decision.md`. Custom new destinations are available through
`--output` and `--decision-output`. Existing destinations are refused.

The finalizer rejects results whose recorded corpus or threshold digest differs
from the reviewed inputs. It first validates the complete result against the
published schema, verifies every self-digested sample, recomputes the
recommended configuration and every automatic summary and gate, verifies the
exact ordered matrix and repository evidence, and rejects any drift or omitted
cell. It then verifies the blinded files and index against the private manifest,
requires exact per-reviewer coverage, recalculates aggregate and per-reviewer
manual scores, verifies the existing decision report, and binds source-result,
manifest, and ratings digests into new finalized outputs.

Aggregation and finalization call the same derivation engine for every metric,
gate, recommended configuration, typed result status, termination state, and
`ship`/`adjust`/`defer` recommendation. Finalization compares the stored source
against that recomputation before it considers ratings, so preview and apply
cannot drift from the live benchmark decision rules.

## Measurements and decision

`results.json` follows `advisor-benchmark-1`; `decision.md` is its human
summary. Each sample records both the source advice-artifact digest and a
self-digest over its complete machine evidence. The schema rejects unknown nested fields and distinguishes
prepare-only, completed, interrupted, unfinalized, finalized, and shippable
states. Every result is validated before persistence. The JSON result and
Markdown decision are written as a rollback-safe pair using new synchronized
files and restoration backups, so a failed second replacement does not leave
mixed evidence. Measurements distinguish model load, prompt processing, generation,
context construction, provider time, validation, time to a validated artifact,
total wall time, token rates/counts, peak process RSS, system-available memory
as a pressure proxy, swap growth, failures, retries, malformed outputs,
reference acceptance, detector-truth resistance, per-rule and aggregate
agreement, citation completeness, abstention, and repeated-verdict consistency.
Manual ratings cover the dimensions that cannot be inferred from schema
validity. `--format json` emits a validated `advisor-operation-receipt` with a
stable operation code for benchmark writes and finalization preview/apply.

The no-provider test suite exercises the child supervisor as an end-to-end
fault matrix: bounded success, ordinary nonzero exit, stalled child,
TERM-resistant child, oversized stdout, and oversized stderr. It verifies the
deadline/output termination codes, retained-byte ceilings, and prompt child
reaping without provisioning or contacting a model runtime.

Ollama's native `/api/chat` adapter can expose nanosecond load,
prompt-evaluation, and generation timings, but it is never selected
automatically. The OpenAI-compatible adapter remains a separately tested
portable path; select either provider and its exact loopback endpoint
explicitly. Remote endpoints are refused because V1 has no authenticated TLS
transport.

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
disables public inference but does not affect provider-free context or ordinary
Git Slop detector releases. `cargo xtask check-distribution`, and therefore
release preparation, rejects any attempt to enable public inference unless the
checked-in recommendation is `ship`.
