# Safeguard-only V1 M2 Air decision

- Observed: 2026-08-15 in `America/Los_Angeles`
- Recommendation: **defer**
- Matrix completed: **false**
- Validated advice artifacts: **0**
- Automatic gates passed: **false**
- Manual gates run: **false**
- Recommended default configuration: **none**

This is the privacy-safe aggregate decision from the first real run of the V1
protocol on the target 16-GB M2 MacBook Air. It does not contain repository
paths, excerpts, prompts, responses, rationales, private skill content, serial
numbers, or hardware UUIDs.

## Pinned inputs

- Git Slop revision: `ad5cdc08768f68a870807aa52c8f32f6512353ad`
- `deeptravel` revision: `22e9544ce8603d7b4104e110f472f6229af90320`
- Corpus SHA-256: `0ede331f3340b0af6e6ea1ac6b85c04617e9cec5452011935c1e2a879f9bf959`
- Thresholds SHA-256: `282746d37e3ce7be26fbc11b6543e86c2e249c85e267aeffdacf7b96a3fa3b9d`
- Git Slop semantic report SHA-256: `6136abfc836d3997d40ac1f068767985fb01bccc3701ff1a0d3534176396b638`
- `deeptravel` semantic report SHA-256: `49a4ab7ad5be9d71a53946ea2e2435a847df5c2f44c0aa1e0c6e36c75cf09312`

Both disposable checkouts were clean and matched the revisions and semantic
report fingerprints preregistered in `corpus-v1.json`.

## Runtime identity

- Hardware: MacBook Air model `Mac14,2`, Apple M2, 16 GiB physical memory
- OS: macOS 26.5.2, Darwin 25.5.0
- Runtime: Ollama 0.32.13 with flash attention and q8_0 KV cache
- Provider: native loopback Ollama `/api/chat`
- Model: `openai/gpt-oss-safeguard-20b`
- Runtime model: `gpt-oss-safeguard:20b`
- Manifest digest: `sha256:f2e795d0099c05eb8231a96445e6d2440aa381e1c03a1e3b3cb1f2cec296adff`
- Quantization: MXFP4
- Published model size: 13,793,441,254 bytes
- Runtime context: 16,384 tokens
- Request timeout: 600 seconds

The model identity and size were checked against the local Ollama manifest and
the [official Ollama model page](https://ollama.com/library/gpt-oss-safeguard%3Alatest).

## Observed termination

The final pinned run attempted two consecutive samples and produced no advice
artifact:

1. The cold runner was killed by macOS with signal 9 during model loading. The
   loopback provider returned HTTP 500.
2. The next runner loaded successfully in 194.38 seconds at the required
   16,384-token context. During prompt evaluation it reached 512 of 4,169 input
   tokens in 59.32 seconds before macOS killed it with signal 9. Ollama returned
   HTTP 500 after approximately 4 minutes 29 seconds.

Ollama reported no free swap while launching the runner. Its fitted memory
projection included 11,005 MiB of Metal memory plus 1,288 MiB of host memory
and 1,344 MiB of CPU repack memory. Because no sample produced a validated
artifact, quality, citation, consistency, latency, and manual-review gates
could not be evaluated. The private review directory remained empty.

## Decision

Do not publish or enable Safeguard inference for this target. Keep the
deterministic policy and provider-free context workflows public; retain provider
adapters only behind the capacity-gated maintainer benchmark until a future
complete pinned matrix produces `ship`.

Do not retry inference on this 16-GB M2 Air. Any future model evaluation must
use a separately provisioned, adequately resourced Apple Silicon host with the
same pinned model and corpus. It must rerun the complete matrix and manual
review; this incomplete result cannot be finalized into `ship` or treated as
model-quality evidence.

This `defer` is now encoded in `benchmarks/advisor/release-gate.json`. It
disables public inference and cannot be changed to enabled unless the
checked-in recommendation is `ship`. The V1 protocol does not require model
weights for provider-free context, ordinary builds, tests, scans, or releases.
