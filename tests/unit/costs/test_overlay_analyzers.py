from __future__ import annotations

from pathlib import Path

from git_slop.core.models import (
    BaselineFacts,
    ChangeSetFacts,
    FileFacts,
    FileTokenFacts,
    HistoryFacts,
    InventoryFacts,
    RepositoryFacts,
    TokenFacts,
)
from git_slop.costs.blast_radius import BlastRadiusOverlayAnalyzer
from git_slop.costs.navigation import NavigationOverlayAnalyzer
from git_slop.costs.semantic_drift import SemanticDriftOverlayAnalyzer
from git_slop.costs.stewardship import StewardshipOverlayAnalyzer
from git_slop.costs.verification import VerificationOverlayAnalyzer


def _build_repository_facts() -> RepositoryFacts:
    config = {
        "verification": {"test_path_markers": ["tests/", ".test."]},
        "navigation": {"top_distinctive_terms": 5},
        "stewardship": {"bot_name_markers": ["bot"]},
        "semantic_drift": {"top_term_limit": 25},
    }
    inventory = InventoryFacts(
        tracked_paths=["src/app.py", "tests/test_app.py"],
        skipped={},
        files=[
            FileFacts("src/app.py", 20, 2, "def parseTrip(v): return v", "a"),
            FileFacts("tests/test_app.py", 20, 2, "def test_parse_trip(): pass", "b"),
        ],
    )
    tokens = TokenFacts(
        context_tokenizer_name="cl100k_base",
        structural_tokenizer_version="1",
        files=[
            FileTokenFacts(
                path="src/app.py",
                context_token_count=20,
                context_band="compact",
                context_pressure=0.1,
                structural_tokens=["parse", "trip", "parse", "trip", "app", "src"],
                structural_token_count=6,
                top_structural_terms=["parse", "trip", "app"],
            ),
            FileTokenFacts(
                path="tests/test_app.py",
                context_token_count=18,
                context_band="compact",
                context_pressure=0.1,
                structural_tokens=["test", "parse", "trip", "app", "tests"],
                structural_token_count=5,
                top_structural_terms=["test", "parse", "trip"],
            ),
        ],
    )
    commit_records = [
        {
            "commit": "one",
            "timestamp": 1710000000,
            "author_name": "Alice",
            "author_email": "alice@example.com",
            "author_key": "Alice <alice@example.com>",
            "file_count": 2,
            "top_level_root_count": 2,
            "change_entropy": 0.8,
            "files": [
                {"path": "src/app.py", "line_delta": 10, "token_delta": 12},
                {"path": "tests/test_app.py", "line_delta": 4, "token_delta": 4},
            ],
        },
        {
            "commit": "two",
            "timestamp": 1710500000,
            "author_name": "Alice",
            "author_email": "alice@example.com",
            "author_key": "Alice <alice@example.com>",
            "file_count": 1,
            "top_level_root_count": 1,
            "change_entropy": 0.2,
            "files": [{"path": "src/app.py", "line_delta": 8, "token_delta": 10}],
        },
    ]
    return RepositoryFacts(
        repo_root=Path("."),
        config=config,
        repo={"repo_name": "sample"},
        inventory=inventory,
        tokens=tokens,
        history=HistoryFacts(
            file_metrics={
                "src/app.py": {},
                "tests/test_app.py": {},
            },
            commit_records=commit_records,
            repo_baselines={},
        ),
        changesets=ChangeSetFacts(commit_records=commit_records),
        baselines=BaselineFacts(repo_baselines={}),
        file_records=[
            {
                "path": "src/app.py",
                "tokens": 20,
                "priority_score": 70.0,
                "churn_pressure": 0.7,
            },
            {
                "path": "tests/test_app.py",
                "tokens": 18,
                "priority_score": 30.0,
                "churn_pressure": 0.2,
            },
        ],
    )


def test_overlay_analyzers_emit_expected_sections() -> None:
    facts = _build_repository_facts()

    verification = VerificationOverlayAnalyzer().analyze(facts)
    navigation = NavigationOverlayAnalyzer().analyze(facts)
    blast_radius = BlastRadiusOverlayAnalyzer().analyze(facts)
    stewardship = StewardshipOverlayAnalyzer().analyze(facts)
    semantic_drift = SemanticDriftOverlayAnalyzer().analyze(facts)

    assert verification["files"][0]["path"] == "src/app.py"
    assert "verification_gap" in verification["files"][0]
    assert "navigation_pressure" in navigation["files"][0]
    assert "blast_radius_pressure" in blast_radius["files"][0]
    assert "stewardship_pressure" in stewardship["files"][0]
    assert "semantic_drift_pressure" in semantic_drift["files"][0]
