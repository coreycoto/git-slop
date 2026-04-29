---
name: plan-maintenance
description: Turn reviewed git-slop findings into bounded maintenance slices without duplicating shared project-management workflows.
---

# Plan Maintenance From Git Slop

Use this skill when a reviewed hotspot should become a bounded maintenance
proposal.

## Workflow

1. Start from an existing report under `.slop/latest/`.
2. Run `git-slop explain` for the selected file, folder, cluster, or relationship.
3. Run `git-slop plan` for the same selector.
4. Keep the proposal narrow and evidence-backed.
5. If backlog conversion is requested, hand the reviewed plan to the
   `project-management-workflows` plugin rather than reimplementing backlog
   mutation guidance here.
