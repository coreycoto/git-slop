from __future__ import annotations

import math
from typing import Any

from git_slop.tokenization import context_band_for_tokens, context_pressure_for_tokens

PRIORITY_BAND_ORDER = {
    "watchlist": 0,
    "needs_refactor": 1,
    "should_refactor": 2,
    "must_refactor": 3,
}

CONTEXT_BAND_ORDER = {
    "compact": 0,
    "healthy": 1,
    "warning": 2,
    "critical": 3,
}


def _p95(values: list[float]) -> float:
    if not values:
        return 1.0
    sorted_values = sorted(values)
    index = max(0, math.ceil(len(sorted_values) * 0.95) - 1)
    return sorted_values[index]


def priority_band_for_score(score: float) -> str:
    if score >= 85:
        return "must_refactor"
    if score >= 65:
        return "should_refactor"
    if score >= 50:
        return "needs_refactor"
    return "watchlist"


def age_pressure(age_days: int, config: dict[str, Any]) -> float:
    half_life = float(config["history"]["age_half_life_days"])
    if age_days <= 0:
        return 0.0
    return 1 - math.pow(2, -(age_days / half_life))


def reason_codes_for_record(record: dict[str, Any]) -> list[str]:
    reason_codes: list[str] = []
    if record["context_band"] == "critical":
        reason_codes.append("critical_token_cost")
    elif record["context_band"] == "warning":
        reason_codes.append("high_token_cost")
    if record["age_days"] >= 180:
        reason_codes.append("old_file")
    if record["revision_norm"] >= 0.8:
        reason_codes.append("high_revision_frequency")
    if record["relative_churn_norm"] >= 0.8:
        reason_codes.append("high_relative_churn")
    if record["age_days"] >= 180 and record["churn_pressure"] >= 0.6:
        reason_codes.append("old_and_volatile")
    return reason_codes


def apply_scoring(records: list[dict[str, Any]], config: dict[str, Any]) -> list[dict[str, Any]]:
    revision_p95 = max(1.0, _p95([float(record["revisions_window"]) for record in records]))
    relative_churn_p95 = _p95([float(record["relative_churn_window"]) for record in records])
    relative_churn_denom = relative_churn_p95 if relative_churn_p95 > 0 else 1.0

    scoring_config = config["scoring"]
    enriched: list[dict[str, Any]] = []
    for record in records:
        age_component = age_pressure(int(record["age_days"]), config)
        revision_norm = min(1.0, float(record["revisions_window"]) / revision_p95)
        relative_churn_norm = min(
            1.0, float(record["relative_churn_window"]) / relative_churn_denom
        )
        churn_pressure = (0.6 * revision_norm) + (0.4 * relative_churn_norm)
        priority_score = 100 * (
            (float(scoring_config["context_weight"]) * float(record["context_pressure"]))
            + (float(scoring_config["age_weight"]) * age_component)
            + (float(scoring_config["churn_weight"]) * churn_pressure)
        )
        enriched_record = dict(record)
        enriched_record["age_pressure"] = round(age_component, 6)
        enriched_record["revision_norm"] = round(revision_norm, 6)
        enriched_record["relative_churn_norm"] = round(relative_churn_norm, 6)
        enriched_record["churn_pressure"] = round(churn_pressure, 6)
        enriched_record["priority_score"] = round(priority_score, 1)
        enriched_record["priority_band"] = priority_band_for_score(
            enriched_record["priority_score"]
        )
        enriched_record["reason_codes"] = reason_codes_for_record(enriched_record)
        enriched.append(enriched_record)
    return enriched


def build_folder_record(
    *,
    path: str,
    descendants: list[dict[str, Any]],
    config: dict[str, Any],
) -> dict[str, Any]:
    sorted_descendants = sorted(
        descendants,
        key=lambda item: (-item["priority_score"], -item["tokens"], item["path"]),
    )
    top_record = sorted_descendants[0]
    reason_codes: list[str] = []
    for record in sorted_descendants:
        for reason_code in record["reason_codes"]:
            if reason_code not in reason_codes:
                reason_codes.append(reason_code)
    total_tokens = sum(int(record["tokens"]) for record in descendants)
    return {
        "path": path,
        "descendant_file_count": len(descendants),
        "bytes": sum(int(record["bytes"]) for record in descendants),
        "lines": sum(int(record["lines"]) for record in descendants),
        "tokens": total_tokens,
        "context_band": context_band_for_tokens(total_tokens, config),
        "context_pressure": round(context_pressure_for_tokens(total_tokens, config), 6),
        "priority_score": top_record["priority_score"],
        "priority_band": top_record["priority_band"],
        "reason_codes": reason_codes,
        "top_file_path": top_record["path"],
    }
