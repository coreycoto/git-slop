from __future__ import annotations

from git_slop.scoring import slop_band_for_score


def test_slop_band_for_score_thresholds() -> None:
    assert slop_band_for_score(0.0) == "low"
    assert slop_band_for_score(49.9) == "low"
    assert slop_band_for_score(50.0) == "moderate"
    assert slop_band_for_score(64.9) == "moderate"
    assert slop_band_for_score(65.0) == "high"
    assert slop_band_for_score(84.9) == "high"
    assert slop_band_for_score(85.0) == "critical"
