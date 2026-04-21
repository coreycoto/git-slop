from __future__ import annotations

from typing import Any

import tiktoken


def encoder_for_config(config: dict[str, Any]) -> tiktoken.Encoding:
    return tiktoken.get_encoding(config["tokenizer"]["name"])


def context_band_for_tokens(tokens: int, config: dict[str, Any]) -> str:
    thresholds = config["context_bands"]
    if tokens <= thresholds["compact_max_tokens"]:
        return "compact"
    if tokens <= thresholds["healthy_max_tokens"]:
        return "healthy"
    if tokens <= thresholds["warning_max_tokens"]:
        return "warning"
    return "critical"


def context_pressure_for_tokens(tokens: int, config: dict[str, Any]) -> float:
    critical_threshold = float(config["context_bands"]["warning_max_tokens"])
    return min(1.0, tokens / critical_threshold)


def apply_token_metrics(
    records: list[dict[str, Any]], config: dict[str, Any]
) -> list[dict[str, Any]]:
    encoding = encoder_for_config(config)
    enriched: list[dict[str, Any]] = []
    for record in records:
        tokens = len(encoding.encode(record["text"], disallowed_special=()))
        enriched_record = dict(record)
        enriched_record["tokens"] = tokens
        enriched_record["context_band"] = context_band_for_tokens(tokens, config)
        enriched_record["context_pressure"] = round(context_pressure_for_tokens(tokens, config), 6)
        enriched.append(enriched_record)
    return enriched
