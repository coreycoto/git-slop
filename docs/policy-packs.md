# Policy packs

Policy-pack schema 1 is Git Slop's accepted data-only contract for sharing
organizational advice rules. Packs guide the optional `advise` command. They do
not run during `find`, change report evidence, alter scores or bands, or affect
`git slop check`.

## Directory contract

```text
git-slop-policy.yaml
policies/
  organization.md
tests/
  organization.yaml
README.md
LICENSE
```

Only the manifest, declared Markdown entrypoints, declared YAML golden files,
and the optional `README.md` and license file are accepted. Schema 1 rejects
undeclared files, symlinks, traversal, absolute paths, non-UTF-8 or non-NFC
text, NUL bytes, excessive depth, more than 64 files, any file over 256 KiB,
and packs over 1 MiB. It has no scripts, hooks, interpolation, native code,
WASM, archive extraction, or lifecycle commands.

The published [`policy-pack-1.json`](../schemas/policy-pack-1.json) schema is
authoritative after YAML-to-JSON conversion. The manifest defines:

- `schema_version`: currently `1`.
- `id`: a stable lowercase, repository-qualified namespace.
- `name`, `description`, `version`, `license`, and
  `min_git_slop_version`: inspectable identity and compatibility.
- `entrypoints`: ordered `policies/*.md` sources.
- `applicability`: `advise`, `plan`, or both.
- `tests`: optional ordered `tests/*.yaml` golden files.
- `rules`: stable IDs, normative text, applicability, severity, consequence,
  required evidence, insufficient-evidence verdict, optional remediation, and
  explicit conflicts.

Policy text is evidence for a constrained evaluator, not an instruction that
can escape the policy trust zone. Good rules state one observable obligation,
identify the evidence needed to judge it, distinguish missing evidence from a
violation, and give a bounded remediation. Do not ask the model to invent
repository architecture, commands, paths, or detector facts.

## Verdicts and composition

Each applicable rule receives an independent `approve`, `abstain`, `revise`,
or `reject` verdict. Git Slop recomputes the aggregate in this fixed order:

```text
reject > revise > abstain > approve
```

Selected packs compose additively in built-in-first, then pack-ID order. Rules
sort in their declared pack order. Duplicate pack IDs and duplicate rule IDs
fail. A conflict is preserved as a pair of rule IDs; no pack silently wins.
The built-in `org.git-slop.core` pack is always included, cannot be removed,
and its namespace cannot be used by third parties.

The pack content digest is SHA-256 over normalized manifest JSON plus ordered
entrypoint and golden text. The policy resolution digest is SHA-256 over the
ordered pack locks. This canonicalization is platform-independent because text
uses NFC and LF newlines and every collection order is explicit.

## Author, install, and lock

```sh
git slop policy init policies/my-team
git slop policy validate policies/my-team
git slop policy test policies/my-team
git slop policy install policies/my-team --select
git slop policy lock
git slop policy list --format json
git slop policy show com.example.repository-policy
```

`install` accepts a reviewed local directory in V1 and copies only declared
data into a content-addressed user cache. It never fetches a network source.
`--select` records the pack ID in `.slop/policies.yaml`; `policy lock` writes
`.slop/policy-lock.json`. Advice resolution is offline and read-only. Changed,
missing, incompatible, or unlocked selected content fails instead of falling
back. Set `GIT_SLOP_POLICY_HOME` when a reproducible isolated cache is needed.

Remove a selected pack explicitly:

```sh
git slop policy remove com.example.repository-policy --unselect
```

The core pack remains present. See
[`examples/policies/strict-verification`](../examples/policies/strict-verification)
and [`examples/policies/agent-change`](../examples/policies/agent-change) for
CC0 templates.

## Golden cases and compatibility

A golden file declares schema 1, unique case IDs, rule/verdict pairs, and the
expected deterministic aggregate. Include approve, abstain, revise, and reject
cases; missing or conflicting evidence; unsafe scope expansion; verification
weakening; and inventory evasion where relevant. `policy test` is model-free
and belongs in ordinary CI. The optional live Safeguard lane uses the separate
advisor benchmark and is never required to validate a pack statically.

Pack versions use strict `major.minor.patch`. Additive wording or golden-case
changes should increment the pack version because they change its content
digest. Breaking schema changes require a new schema file and explicit CLI
support; do not overwrite schema 1 semantics. A signature or popular source
does not make policy content trusted. Review its license, rules, tests, and
digest before selection.
