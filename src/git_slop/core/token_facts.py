from __future__ import annotations

import hashlib
import re
from collections import Counter
from pathlib import Path
from typing import Any

import tiktoken

from .cache import json_fingerprint, load_or_compute_json
from .config import cache_dir
from .models import FileFacts, FileTokenFacts, InventoryFacts, TokenFacts

STRUCTURAL_TOKENIZER_VERSION = 1
CAMEL_CASE_RE = re.compile(r"([a-z0-9])([A-Z])")
STRING_RE = re.compile(r"""(?s)(['"`])(?:\\.|(?!\1).)*\1""")
NUMBER_RE = re.compile(r"\b\d+(?:\.\d+)?\b")
WORD_RE = re.compile(r"[a-z][a-z0-9_]{1,}")


def encoder_for_config(config: dict[str, Any]) -> tiktoken.Encoding:
    return tiktoken.get_encoding(config["tokenization"]["context_tokenizer_name"])


def context_band_for_tokens(tokens: int, config: dict[str, Any]) -> str:
    thresholds = config["tokenization"]["context_bands"]
    if tokens <= thresholds["compact_max_tokens"]:
        return "compact"
    if tokens <= thresholds["healthy_max_tokens"]:
        return "healthy"
    if tokens <= thresholds["warning_max_tokens"]:
        return "warning"
    return "critical"


def context_pressure_for_tokens(tokens: int, config: dict[str, Any]) -> float:
    critical_threshold = float(config["tokenization"]["context_bands"]["warning_max_tokens"])
    return min(1.0, tokens / critical_threshold)


def content_fingerprint_for_text(text: str) -> str:
    return hashlib.sha256(text.encode("utf-8")).hexdigest()


def _normalize_structural_text(text: str) -> str:
    normalized = CAMEL_CASE_RE.sub(r"\1 \2", text)
    normalized = normalized.replace("-", " ").replace("/", " ")
    normalized = STRING_RE.sub(" str ", normalized)
    normalized = NUMBER_RE.sub(" 0 ", normalized)
    return normalized.lower()


def _path_tokens(path: str) -> list[str]:
    path_text = path.replace("-", "/").replace("_", "/").replace(".", "/")
    return [token for token in path_text.lower().split("/") if token]


def structural_tokens_for_file(file_facts: FileFacts) -> list[str]:
    normalized = _normalize_structural_text(file_facts.text)
    tokens = WORD_RE.findall(normalized)
    return tokens + _path_tokens(file_facts.path)


def _token_cache_root(repo_root: Path, namespace: str, cache_key: str) -> Path:
    return cache_dir(repo_root) / "tokens" / namespace / cache_key


def _context_cache_key(file_facts: FileFacts, config: dict[str, Any]) -> str:
    return json_fingerprint(
        {
            "fingerprint": file_facts.content_fingerprint,
            "tokenizer_name": config["tokenization"]["context_tokenizer_name"],
        }
    )


def _structural_cache_key(file_facts: FileFacts) -> str:
    return json_fingerprint(
        {
            "fingerprint": file_facts.content_fingerprint,
            "structural_tokenizer_version": STRUCTURAL_TOKENIZER_VERSION,
        }
    )


def _build_context_payload(
    *,
    file_facts: FileFacts,
    config: dict[str, Any],
    encoding: tiktoken.Encoding,
) -> dict[str, Any]:
    context_token_count = len(encoding.encode(file_facts.text, disallowed_special=()))
    return {
        "path": file_facts.path,
        "context_token_count": context_token_count,
        "context_band": context_band_for_tokens(context_token_count, config),
        "context_pressure": round(context_pressure_for_tokens(context_token_count, config), 6),
    }


def _build_structural_payload(file_facts: FileFacts) -> dict[str, Any]:
    tokens = structural_tokens_for_file(file_facts)
    frequencies = Counter(tokens)
    top_terms = [
        token
        for token, _count in sorted(
            frequencies.items(),
            key=lambda item: (-item[1], item[0]),
        )[:12]
    ]
    return {
        "path": file_facts.path,
        "structural_tokens": tokens,
        "structural_token_count": len(tokens),
        "top_structural_terms": top_terms,
    }


def build_token_facts(
    repo_root: Path,
    inventory: InventoryFacts,
    config: dict[str, Any],
) -> TokenFacts:
    encoding = encoder_for_config(config)
    token_records: list[FileTokenFacts] = []
    for file_facts in inventory.files:
        context_payload = load_or_compute_json(
            _token_cache_root(repo_root, "context", _context_cache_key(file_facts, config))
            / "token_facts.json",
            lambda file_facts=file_facts: _build_context_payload(
                file_facts=file_facts,
                config=config,
                encoding=encoding,
            ),
        )
        structural_payload = load_or_compute_json(
            _token_cache_root(repo_root, "structural", _structural_cache_key(file_facts))
            / "token_facts.json",
            lambda file_facts=file_facts: _build_structural_payload(file_facts),
        )
        token_records.append(
            FileTokenFacts(
                path=file_facts.path,
                context_token_count=int(context_payload["context_token_count"]),
                context_band=str(context_payload["context_band"]),
                context_pressure=float(context_payload["context_pressure"]),
                structural_tokens=list(structural_payload["structural_tokens"]),
                structural_token_count=int(structural_payload["structural_token_count"]),
                top_structural_terms=list(structural_payload["top_structural_terms"]),
            )
        )
    return TokenFacts(
        files=token_records,
        context_tokenizer_name=config["tokenization"]["context_tokenizer_name"],
        structural_tokenizer_version=str(STRUCTURAL_TOKENIZER_VERSION),
    )


def serialize_token_facts(token_facts: TokenFacts) -> dict[str, Any]:
    return {
        "context_tokenizer_name": token_facts.context_tokenizer_name,
        "structural_tokenizer_version": token_facts.structural_tokenizer_version,
        "files": [record.to_dict() for record in token_facts.files],
    }


def apply_token_metrics(
    records: list[dict[str, Any]], config: dict[str, Any]
) -> list[dict[str, Any]]:
    encoding = encoder_for_config(config)
    enriched: list[dict[str, Any]] = []
    for record in records:
        enriched_record = dict(record)
        text = str(record["text"])
        tokens = len(encoding.encode(text, disallowed_special=()))
        file_facts = FileFacts(
            path=record["path"],
            bytes=int(record["bytes"]),
            lines=int(record["lines"]),
            text=text,
            content_fingerprint=content_fingerprint_for_text(text),
        )
        structural_payload = _build_structural_payload(file_facts)
        enriched_record["tokens"] = tokens
        enriched_record["context_band"] = context_band_for_tokens(tokens, config)
        enriched_record["context_pressure"] = round(context_pressure_for_tokens(tokens, config), 6)
        enriched_record["structural_tokens"] = list(structural_payload["structural_tokens"])
        enriched_record["structural_token_count"] = int(
            structural_payload["structural_token_count"]
        )
        enriched_record["top_structural_terms"] = list(structural_payload["top_structural_terms"])
        enriched.append(enriched_record)
    return enriched
