from __future__ import annotations

from agent_tools.skills.manifest import ActionSpec, SkillSpec

ACTION_SPECS: dict[str, ActionSpec] = {
    "repo": ActionSpec(
        "repo",
        "Resolve the current repository context.",
        ("github", "current-repo"),
    ),
    "snapshot": ActionSpec(
        "snapshot",
        "Build the current backlog project snapshot.",
        ("github", "project-snapshot"),
    ),
    "graph": ActionSpec(
        "graph",
        "Build the canonical issue graph.",
        ("github", "issue-graph"),
    ),
    "digest": ActionSpec(
        "digest",
        "Normalize local markdown and DOCX research into intake artifacts.",
        ("research", "digest"),
    ),
    "label-palette": ActionSpec(
        "label-palette",
        "Validate or preview the checked-in label palette.",
        ("github", "sync-label-palette"),
    ),
    "milestone-check": ActionSpec(
        "milestone-check",
        "Compute the current and next quarter milestone policy.",
        ("github", "milestone-check"),
    ),
    "validate-backlog-mutations": ActionSpec(
        "validate-backlog-mutations",
        "Validate a deterministic backlog mutation plan.",
        ("github", "validate-backlog-mutations"),
    ),
    "apply-backlog-mutations": ActionSpec(
        "apply-backlog-mutations",
        "Materialize an apply report for a backlog mutation plan.",
        ("github", "apply-backlog-mutations"),
    ),
    "review-to-backlog": ActionSpec(
        "review-to-backlog",
        "Turn review findings into a deterministic backlog delta.",
        ("github", "review-to-backlog"),
    ),
    "apply-review-delta": ActionSpec(
        "apply-review-delta",
        "Materialize a review backlog apply report.",
        ("github", "apply-review-backlog-delta"),
    ),
    "validate-quarter-plan": ActionSpec(
        "validate-quarter-plan",
        "Validate a quarter plan payload.",
        ("github", "validate-quarter-plan"),
    ),
    "build-quarter-delta": ActionSpec(
        "build-quarter-delta",
        "Turn a validated quarter plan into a milestone delta.",
        ("github", "build-quarter-plan-delta"),
    ),
    "apply-quarter-delta": ActionSpec(
        "apply-quarter-delta",
        "Materialize a quarter-plan apply report.",
        ("github", "apply-quarter-plan-delta"),
    ),
}

SKILL_SPECS: dict[str, SkillSpec] = {
    "intake-preview": SkillSpec(
        name="intake-preview",
        description=(
            "Preview how repo-local research would change the backlog without live GitHub mutation."
        ),
        actions=("repo", "digest", "snapshot"),
    ),
    "intake": SkillSpec(
        name="intake",
        description=(
            "Normalize repo-local research and prepare the minimum backlog "
            "reconciliation artifacts."
        ),
        actions=("repo", "digest", "snapshot"),
    ),
    "review-to-backlog-preview": SkillSpec(
        name="review-to-backlog-preview",
        description="Preview backlog-ready issues from deterministic review findings.",
        actions=("repo", "snapshot", "graph", "review-to-backlog"),
    ),
    "review-to-backlog-apply": SkillSpec(
        name="review-to-backlog-apply",
        description="Prepare review-backed backlog deltas and apply reports.",
        actions=("repo", "snapshot", "graph", "review-to-backlog", "apply-review-delta"),
    ),
    "ensure-quarter-milestones": SkillSpec(
        name="ensure-quarter-milestones",
        description="Check the current and next quarter milestone contract.",
        actions=("repo", "milestone-check"),
    ),
    "plan-quarter-preview": SkillSpec(
        name="plan-quarter-preview",
        description="Validate a quarter plan and preview the resulting milestone delta.",
        actions=("repo", "snapshot", "validate-quarter-plan", "build-quarter-delta"),
    ),
    "plan-quarter-apply": SkillSpec(
        name="plan-quarter-apply",
        description="Validate a quarter plan, build the delta, and materialize the apply report.",
        actions=(
            "repo",
            "snapshot",
            "validate-quarter-plan",
            "build-quarter-delta",
            "apply-quarter-delta",
        ),
    ),
    "github-backlog-mutate": SkillSpec(
        name="github-backlog-mutate",
        description="Validate a mixed backlog mutation plan and materialize an apply report.",
        actions=("repo", "validate-backlog-mutations", "apply-backlog-mutations"),
    ),
    "label-palette-design": SkillSpec(
        name="label-palette-design",
        description="Preview or refresh the deterministic backlog label palette.",
        actions=("repo", "label-palette"),
    ),
}
