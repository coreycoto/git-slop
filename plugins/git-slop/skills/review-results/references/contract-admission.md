# Contract Admission And Surface Discipline

Use this reference when reviewed evidence could create or promote a durable
product or repository contract. Keep all resulting context in the monorepo;
logical boundaries do not require another repository.

## Optimize For Value, Not Closure

Maximize demonstrated user value while minimizing permanent surface area.

An agentic Goodhart loop occurs when agents optimize a proxy such as findings
closed, checks green, commands added, or plans completed. Once the proxy becomes
the target, agents can generate findings and surface that justify more work,
then treat completing that work as proof of value. Break the loop by allowing
`consolidate`, `defer`, `accept`, and `won't fix`, and by requiring user evidence
before permanent surface is admitted.

## Assign A Contract State

| State | Meaning | Promotion evidence |
| --- | --- | --- |
| `internal` | Implementation detail with no compatibility promise | None; keep it replaceable |
| `experimental` | Reversible hypothesis that may change or disappear | A named user, job, expiry, and learning goal |
| `candidate` | Contract used for a demonstrated workflow but not yet broadly promised | Repeated successful use and evidence that existing surface is insufficient |
| `stable` | Compatibility promise worth maintaining | Repeated dependence by multiple independent consumers and understood migration cost |

Do not promote a contract because its implementation is complete. In the
absence of evidence, keep it internal, run an expiring experiment, consolidate
it into existing surface, or defer it.

## Run The Admission Test

Answer every question before recommending a new or promoted contract:

1. Who is the concrete consumer, and what job are they trying to complete?
2. What direct evidence shows the need: observed use, request, failed workflow,
   or repeated workaround?
3. Why can the existing command, option, configuration, schema, report, or
   plugin guidance not do the job?
4. What is the smallest reversible experiment that can test the hypothesis?
5. What permanent surface and maintenance obligation would remain?
6. When does the experiment expire or require review?
7. What result would cause consolidation, rollback, or deletion?

Treat a missing consumer, weak evidence, or no deletion condition as a reason
to `defer` or `consolidate`, not as a prompt to invent more surface.

## Produce A Surface-Area Ledger

For a proposed change, record each affected dimension separately:

- CLI commands and options
- configuration keys and policy
- machine schemas and output formats
- Action inputs, outputs, summaries, comments, and artifacts
- Agent Plugin skills and references
- workflows and integrations
- compatibility promises and migration obligations

For each dimension, state what is added, removed, or consolidated; its contract
state; supporting user evidence; expiry; and deletion condition. Never collapse
the ledger into a composite score. Surface growth is evidence to review, not a
slop verdict or an automatic gate.
