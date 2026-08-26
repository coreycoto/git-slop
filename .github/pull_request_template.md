## Summary

<!-- What changed, and why? -->

## Linked issue

<!-- Use "Closes #123" when this PR completes an issue, or "None". -->

## Validation

<!-- List the exact commands run and their results. -->

## Finding disposition

<!--
For audit-driven work, record each reviewed finding as implement, consolidate,
defer, accept, or won't fix. Use "Not applicable" when this PR did not originate
from an audit. Closing every finding is not the default.
-->

## Surface-area ledger

<!--
Report permanent product/repository surface, not line churn. Use "None" for an
empty row and delete irrelevant rows. The Action supplies path-based evidence;
this ledger records semantic contracts that path counts cannot prove. Do not
calculate a composite score.
-->

| Surface | Added | Removed or consolidated | State | User evidence, expiry, or deletion condition |
| --- | --- | --- | --- | --- |
| CLI commands/options |  |  |  |  |
| Config keys/policy |  |  |  |  |
| Schemas/outputs |  |  |  |  |
| Action/workflows |  |  |  |  |
| Agent Plugin |  |  |  |  |
| Other permanent surface |  |  |  |  |

Allowed states: `internal`, `experimental`, `candidate`, `stable`.

<!--
For every added or promoted contract: Who consumes it? What job does it do?
What direct evidence supports it? Why is existing surface insufficient? What is
the smallest reversible experiment? What will it cost permanently? When does it
expire, and what evidence would delete it?
-->

## Contract impact

- [ ] Report or JSON Schema
- [ ] CLI/help/man page
- [ ] GitHub Action
- [ ] Agent Plugin
- [ ] Release or distribution artifacts
- [ ] No public contract impact

<!-- Explain every checked impact, including compatibility or migration notes. -->
