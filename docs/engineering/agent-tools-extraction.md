# Agent Tools Extraction

- Status: current
- Audience: maintainer
- Canonical: yes

This document defines the extraction boundary now that `git-slop` consumes the
standalone `agent-tools` repository for shared maintainer tooling.

## Goal

Create a sibling repo named `agent-tools` that owns reusable automation and
runtime code, while leaving product policy and workflow content inside each
consumer repository.

## What Moves

The future standalone repo should own:

- skill runtime and skill manifest machinery
- CLI framework and command registration
- repo and path resolution helpers
- metadata sync for generated `agents/openai.yaml`
- artifact path helpers and output writers
- schema and config validation helpers
- generic GitHub backlog and project helpers
- generic review-to-backlog, quarter-plan, and preview/apply workflow engines
- research intake normalization and artifact rendering

## What Stays Local

Consumer repos such as `git-slop` should continue to own:

- issue taxonomy choices
- milestone policy defaults
- label semantics
- seeded issue catalogs and roadmap tracks
- `.agents/skills/*`
- GitHub issue forms
- engineering governance docs
- any prompt or workflow content that encodes product-specific judgment

## Target Package Layout

The future `agent-tools` repo should expose:

- `agent-tools github ...`
- `agent-tools research ...`
- `agent-tools skills ...`

And organize the Python package into these domains:

- `agent_tools.core`
- `agent_tools.skills`
- `agent_tools.github`
- `agent_tools.research`
- `agent_tools.artifacts`

## Workspace-First Migration

Use this migration order:

1. Create `agent-tools` as a sibling repo with `uv` + `hatchling`.
2. Move the shared runtime, metadata sync, GitHub helpers, and research intake foundation first.
3. Keep `git-slop` as the first consumer migration and replace local runtime imports with the sibling dependency.
4. Migrate the overlapping `deeptravel` slices second.
5. Leave larger workflow families such as relationships, rebalance, review closeout, autoresearch, testing/evals, and performance tooling for later waves.

## Current Extraction Boundary

The current `git-slop` implementation should be treated like this:

- reusable code lives in the standalone `agent-tools` repo
- repo-local policy data lives under `config/github/`
- seeded issue definitions must not live inside the shared runtime package

This repo already uses `config/github/issue_seed_catalog.json` as the repo-local
source of truth for seeded issues, priorities, and queue order. That pattern
should be preserved across future consumers.

## Current CI Dependency Note

`git-slop` currently installs `agent-tools` from a tagged Git dependency.
While `coreycoto/agent-tools` remains private, CI must provide either
`AGENT_TOOLS_READ_TOKEN` or `GH_PROJECTS_TOKEN` with read access to that repo.
If the shared package becomes public later, the workflows can install it
without extra Git auth.
