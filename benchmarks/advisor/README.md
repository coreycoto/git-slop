# Safeguard V1 benchmark inputs

`corpus-v1.json` is the public, privacy-safe reviewed selector and verdict set.
It contains no source excerpts, prompts, private paths, or proprietary skill
content. `thresholds-v1.json` freezes the ship gates before the final corpus
run. The native harness validates both files before invoking the public CLI.

Private repository review can extend the corpus in an untracked file using the
published corpus schema. Do not commit raw `deeptravel` context, advice,
rationales, paths, or prompts. Generated results belong under ignored
`benchmark-results/`; the committed decision report may contain aggregate
measurements and anonymous case IDs only.

The complete protocol, clean-checkout requirement, preflight, model/runtime
matrix, private ratings contract, and decision semantics are documented in
[`docs/benchmarks/safeguard-v1.md`](../../docs/benchmarks/safeguard-v1.md).
The privacy-safe aggregate outcome from the first target-hardware run is the
[`defer` decision](../../docs/benchmarks/safeguard-v1-m2-air-decision.md).
