from __future__ import annotations

from dataclasses import dataclass

try:
    from agent_plugins.contracts.skills import ActionSpec, SkillSpec
except ModuleNotFoundError:

    @dataclass(frozen=True)
    class ActionSpec:  # type: ignore[no-redef]
        name: str
        description: str
        command: tuple[str, ...]

    @dataclass(frozen=True)
    class SkillSpec:  # type: ignore[no-redef]
        name: str
        description: str
        actions: tuple[str, ...]

ACTION_SPECS: dict[str, ActionSpec] = {
    "repo": ActionSpec(
        "repo",
        "Resolve the current repository context.",
        ("agent_plugins.github.current_repo",),
    ),
    "snapshot": ActionSpec(
        "snapshot",
        "Build the current backlog project snapshot.",
        ("agent_plugins.github.shared.project_snapshot",),
    ),
    "graph": ActionSpec(
        "graph",
        "Build the canonical issue graph.",
        ("agent_plugins.github.shared.issue_graph",),
    ),
    "digest": ActionSpec(
        "digest",
        "Normalize local markdown and DOCX research into intake artifacts.",
        ("agent_plugins.research.digest",),
    ),
    "label-palette": ActionSpec(
        "label-palette",
        "Validate or preview the checked-in label palette.",
        ("agent_plugins.github.governance.sync_label_palette",),
    ),
    "milestone-check": ActionSpec(
        "milestone-check",
        "Compute the current and next quarter milestone policy.",
        ("agent_plugins.github.governance.milestone_check",),
    ),
    "validate-backlog-mutations": ActionSpec(
        "validate-backlog-mutations",
        "Validate a deterministic backlog mutation plan.",
        ("agent_plugins.github.governance.validate_backlog_mutations",),
    ),
    "apply-backlog-mutations": ActionSpec(
        "apply-backlog-mutations",
        "Materialize an apply report for a backlog mutation plan.",
        ("agent_plugins.github.governance.apply_backlog_mutations",),
    ),
    "review-to-backlog": ActionSpec(
        "review-to-backlog",
        "Turn review findings into a deterministic backlog delta.",
        ("agent_plugins.github.reviews.review_to_backlog",),
    ),
    "plan-to-backlog": ActionSpec(
        "plan-to-backlog",
        "Turn git-slop plan output into a deterministic backlog preview delta.",
        ("agent_plugins.github.planning.plan_to_backlog",),
    ),
    "apply-review-delta": ActionSpec(
        "apply-review-delta",
        "Materialize a review backlog apply report.",
        ("agent_plugins.github.reviews.apply_review_backlog_delta",),
    ),
    "validate-quarter-plan": ActionSpec(
        "validate-quarter-plan",
        "Validate a quarter plan payload.",
        ("agent_plugins.github.planning.validate_quarter_plan",),
    ),
    "build-quarter-delta": ActionSpec(
        "build-quarter-delta",
        "Turn a validated quarter plan into a milestone delta.",
        ("agent_plugins.github.planning.build_quarter_plan_delta",),
    ),
    "apply-quarter-delta": ActionSpec(
        "apply-quarter-delta",
        "Materialize a quarter-plan apply report.",
        ("agent_plugins.github.planning.apply_quarter_plan_delta",),
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
    "plan-to-backlog-preview": SkillSpec(
        name="plan-to-backlog-preview",
        description="Preview backlog-ready maintenance issues from git-slop plan output.",
        actions=("repo", "snapshot", "plan-to-backlog"),
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
