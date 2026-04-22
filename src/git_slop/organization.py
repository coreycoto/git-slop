from __future__ import annotations

import hashlib
import json
import math
import re
from collections import defaultdict
from dataclasses import dataclass
from itertools import combinations
from pathlib import Path, PurePosixPath
from typing import Any, Iterable

from .config import cache_dir
from .git import run_git
from .tokenization import encoder_for_config

ANALYSIS_STATUS = "experimental"
ANALYSIS_VERSION = 1
WINDOW_SIZE = 128
WINDOW_STEP = 32
MIN_WINDOW_TOKENS = 32
NEAR_DUPLICATE_SIMILARITY = 0.85
MIN_COCHANGE_SUPPORT = 3
MIN_COUPLING_LIFT = 2.0
MINHASH_SEEDS = [11_111 + (index * 7_919) for index in range(32)]
MINHASH_BAND_SIZE = 4
MINHASH_MASK = (1 << 64) - 1
MAX_NEAR_DUPLICATE_BAND_BUCKET = 32
STOPWORDS = {
    "about",
    "after",
    "again",
    "also",
    "because",
    "being",
    "between",
    "could",
    "every",
    "first",
    "from",
    "have",
    "into",
    "just",
    "like",
    "many",
    "more",
    "other",
    "over",
    "same",
    "some",
    "such",
    "than",
    "that",
    "their",
    "there",
    "these",
    "this",
    "those",
    "through",
    "under",
    "very",
    "what",
    "when",
    "where",
    "which",
    "while",
    "with",
}
STRING_RE = re.compile(r"""(?s)(['"`])(?:\\.|(?!\1).)*\1""")
NUMBER_RE = re.compile(r"\b\d+(?:\.\d+)?\b")
WHITESPACE_RE = re.compile(r"\s+")
WORD_RE = re.compile(r"[a-z][a-z0-9_]{3,}")


@dataclass(frozen=True)
class _WindowRecord:
    id: str
    path: str
    top_level_root: str
    start_token: int
    token_length: int
    exact_hash: str
    shingle_hashes: tuple[int, ...]
    minhash_signature: tuple[int, ...]


def _top_level_root(path: str) -> str:
    parts = PurePosixPath(path).parts
    return parts[0] if parts else "."


def _percentile(values: list[float], quantile: float) -> float:
    if not values:
        return 0.0
    sorted_values = sorted(values)
    index = max(0, math.ceil(len(sorted_values) * quantile) - 1)
    return float(sorted_values[index])


def _round6(value: float) -> float:
    return round(float(value), 6)


def _json_fingerprint(payload: Any) -> str:
    encoded = json.dumps(payload, sort_keys=True, separators=(",", ":")).encode("utf-8")
    return hashlib.sha256(encoded).hexdigest()


def _content_fingerprint(records: list[dict[str, Any]]) -> str:
    digest = hashlib.sha256()
    for record in sorted(records, key=lambda item: item["path"]):
        digest.update(record["path"].encode("utf-8"))
        digest.update(b"\0")
        digest.update(record["text"].encode("utf-8"))
        digest.update(b"\0")
    return digest.hexdigest()


def _analysis_cache_key(
    repo_root: Path,
    records: list[dict[str, Any]],
    config: dict[str, Any],
) -> str:
    head_completed = run_git(repo_root, ["rev-parse", "--verify", "HEAD"])
    head_value = head_completed.stdout.strip() if head_completed.returncode == 0 else ""
    payload = {
        "analysis_version": ANALYSIS_VERSION,
        "head": head_value,
        "config": config,
        "inventory": _content_fingerprint(records),
    }
    return _json_fingerprint(payload)


def _cache_root(repo_root: Path, cache_key: str) -> Path:
    return cache_dir(repo_root) / "organization-health" / cache_key


def _load_cached_json(path: Path) -> Any | None:
    if not path.exists():
        return None
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except json.JSONDecodeError:
        return None


def _write_cached_json(path: Path, payload: Any) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(
        json.dumps(payload, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )


def _load_or_compute_cached(path: Path, builder) -> Any:
    cached = _load_cached_json(path)
    if cached is not None:
        return cached
    payload = builder()
    _write_cached_json(path, payload)
    return payload


def _normalize_text(text: str) -> str:
    normalized = text.lower()
    normalized = STRING_RE.sub('"str"', normalized)
    normalized = NUMBER_RE.sub("0", normalized)
    normalized = WHITESPACE_RE.sub(" ", normalized).strip()
    return normalized


def _stable_hash(parts: Iterable[Any]) -> str:
    digest = hashlib.blake2b(digest_size=16)
    for part in parts:
        digest.update(str(part).encode("utf-8"))
        digest.update(b"\0")
    return digest.hexdigest()


def _shingle_hashes(window_tokens: list[int]) -> tuple[int, ...]:
    if len(window_tokens) < 3:
        hashes = {_stable_hash(("token", token)) for token in window_tokens}
        return tuple(sorted(int(hash_value, 16) for hash_value in hashes))
    hashes: set[int] = set()
    for index in range(len(window_tokens) - 2):
        shingle = window_tokens[index : index + 3]
        hashed = _stable_hash(("shingle", *shingle))
        hashes.add(int(hashed, 16))
    return tuple(sorted(hashes))


def _minhash_signature(shingle_hashes: tuple[int, ...]) -> tuple[int, ...]:
    if not shingle_hashes:
        return tuple(0 for _ in MINHASH_SEEDS)
    signature: list[int] = []
    for seed in MINHASH_SEEDS:
        transformed_hashes = (
            ((shingle_hash ^ seed) * 1_099_511_628_211) & MINHASH_MASK
            for shingle_hash in shingle_hashes
        )
        signature.append(
            min(transformed_hashes)
        )
    return tuple(signature)


def _window_starts(token_count: int) -> list[int]:
    if token_count < MIN_WINDOW_TOKENS:
        return []
    if token_count <= WINDOW_SIZE:
        return [0]
    starts = set(range(0, max(token_count - WINDOW_SIZE, 0) + 1, WINDOW_STEP))
    starts.add(token_count - WINDOW_SIZE)
    return sorted(starts)


def _serialize_window_record(window: _WindowRecord) -> dict[str, Any]:
    return {
        "id": window.id,
        "path": window.path,
        "top_level_root": window.top_level_root,
        "start_token": window.start_token,
        "token_length": window.token_length,
        "exact_hash": window.exact_hash,
        "shingle_hashes": list(window.shingle_hashes),
        "minhash_signature": list(window.minhash_signature),
    }


def _deserialize_window_records(payload: list[dict[str, Any]]) -> dict[str, _WindowRecord]:
    return {
        item["id"]: _WindowRecord(
            id=item["id"],
            path=item["path"],
            top_level_root=item["top_level_root"],
            start_token=int(item["start_token"]),
            token_length=int(item["token_length"]),
            exact_hash=item["exact_hash"],
            shingle_hashes=tuple(int(value) for value in item["shingle_hashes"]),
            minhash_signature=tuple(int(value) for value in item["minhash_signature"]),
        )
        for item in payload
    }


def _build_window_records(
    records: list[dict[str, Any]],
    config: dict[str, Any],
) -> list[dict[str, Any]]:
    encoding = encoder_for_config(config)
    payload: list[dict[str, Any]] = []
    for record in sorted(records, key=lambda item: item["path"]):
        normalized_text = _normalize_text(record["text"])
        token_ids = encoding.encode(normalized_text, disallowed_special=())
        for start in _window_starts(len(token_ids)):
            window_tokens = token_ids[start : start + WINDOW_SIZE]
            exact_hash = _stable_hash(("exact", *window_tokens))
            shingle_hashes = _shingle_hashes(window_tokens)
            window = _WindowRecord(
                id=_stable_hash(("window", record["path"], start, len(window_tokens), exact_hash)),
                path=record["path"],
                top_level_root=_top_level_root(record["path"]),
                start_token=start,
                token_length=len(window_tokens),
                exact_hash=exact_hash,
                shingle_hashes=shingle_hashes,
                minhash_signature=_minhash_signature(shingle_hashes),
            )
            payload.append(_serialize_window_record(window))
    return payload


def _build_exact_index(windows: dict[str, _WindowRecord]) -> dict[str, list[str]]:
    grouped: dict[str, list[str]] = defaultdict(list)
    for window in windows.values():
        grouped[window.exact_hash].append(window.id)
    return {key: sorted(values) for key, values in grouped.items()}


def _pair_key(path_a: str, path_b: str) -> tuple[str, str]:
    return (path_a, path_b) if path_a <= path_b else (path_b, path_a)


def _relationship_id(kind: str, path_a: str, path_b: str) -> str:
    return f"{kind}-{_stable_hash((kind, *_pair_key(path_a, path_b)))[:12]}"


def _cluster_id(kind: str, member_paths: list[str]) -> str:
    return f"{kind}-{_stable_hash((kind, *member_paths))[:12]}"


def _crosses_top_level_boundary(path_a: str, path_b: str) -> bool:
    return _top_level_root(path_a) != _top_level_root(path_b)


def _build_duplicate_relationships(
    windows: dict[str, _WindowRecord],
    exact_index: dict[str, list[str]],
) -> tuple[list[dict[str, Any]], dict[str, set[str]]]:
    aggregated: dict[tuple[str, str], dict[str, Any]] = {}
    file_window_ids: dict[str, set[str]] = defaultdict(set)
    for window_ids in exact_index.values():
        if len(window_ids) < 2:
            continue
        exact_windows = [windows[window_id] for window_id in window_ids]
        if len({window.path for window in exact_windows}) < 2:
            continue
        for left, right in combinations(exact_windows, 2):
            if left.path == right.path:
                continue
            pair = _pair_key(left.path, right.path)
            relationship = aggregated.setdefault(
                pair,
                {
                    "id": _relationship_id("duplicate_neighborhood", *pair),
                    "kind": "duplicate_neighborhood",
                    "source_path": pair[0],
                    "target_path": pair[1],
                    "duplicate_token_mass": 0,
                    "shared_window_count": 0,
                    "similarity_ratio": 1.0,
                    "crosses_top_level_boundary": _crosses_top_level_boundary(*pair),
                    "window_ids": set(),
                },
            )
            relationship["duplicate_token_mass"] += min(left.token_length, right.token_length)
            relationship["shared_window_count"] += 1
            relationship["window_ids"].update((left.id, right.id))
            file_window_ids[left.path].add(left.id)
            file_window_ids[right.path].add(right.id)
    results: list[dict[str, Any]] = []
    for relationship in aggregated.values():
        relationship["evidence_score"] = round(
            float(relationship["duplicate_token_mass"]),
            3,
        )
        relationship["window_ids"] = sorted(relationship["window_ids"])
        results.append(relationship)
    return sorted(
        results,
        key=lambda item: (-item["evidence_score"], item["id"]),
    ), file_window_ids


def _build_near_signature_index(windows: dict[str, _WindowRecord]) -> dict[str, list[str]]:
    grouped: dict[str, set[str]] = defaultdict(set)
    for window in windows.values():
        for band_start in range(0, len(window.minhash_signature), MINHASH_BAND_SIZE):
            band = window.minhash_signature[band_start : band_start + MINHASH_BAND_SIZE]
            band_key = f"{band_start}:{','.join(str(value) for value in band)}"
            grouped[band_key].add(window.id)
    return {
        key: sorted(values)
        for key, values in grouped.items()
        if 1 < len(values) <= MAX_NEAR_DUPLICATE_BAND_BUCKET
    }


def _jaccard_similarity(left: tuple[int, ...], right: tuple[int, ...]) -> float:
    if not left and not right:
        return 1.0
    left_set = set(left)
    right_set = set(right)
    union = left_set | right_set
    if not union:
        return 0.0
    return len(left_set & right_set) / len(union)


def _build_near_duplicate_relationships(
    windows: dict[str, _WindowRecord],
    signature_index: dict[str, list[str]],
) -> tuple[list[dict[str, Any]], dict[str, set[str]]]:
    candidate_pairs: set[tuple[str, str]] = set()
    for window_ids in signature_index.values():
        if len(window_ids) < 2:
            continue
        for left_id, right_id in combinations(window_ids, 2):
            pair = (left_id, right_id) if left_id <= right_id else (right_id, left_id)
            candidate_pairs.add(pair)

    aggregated: dict[tuple[str, str], dict[str, Any]] = {}
    file_window_ids: dict[str, set[str]] = defaultdict(set)
    for left_id, right_id in sorted(candidate_pairs):
        left = windows[left_id]
        right = windows[right_id]
        if left.path == right.path or left.exact_hash == right.exact_hash:
            continue
        similarity = _jaccard_similarity(left.shingle_hashes, right.shingle_hashes)
        if similarity < NEAR_DUPLICATE_SIMILARITY:
            continue
        pair = _pair_key(left.path, right.path)
        relationship = aggregated.setdefault(
            pair,
            {
                "id": _relationship_id("near_duplicate_neighborhood", *pair),
                "kind": "near_duplicate_neighborhood",
                "source_path": pair[0],
                "target_path": pair[1],
                "duplicate_token_mass": 0,
                "shared_window_count": 0,
                "similarity_ratio": 0.0,
                "crosses_top_level_boundary": _crosses_top_level_boundary(*pair),
                "window_ids": set(),
            },
        )
        relationship["duplicate_token_mass"] += min(left.token_length, right.token_length)
        relationship["shared_window_count"] += 1
        relationship["similarity_ratio"] = max(relationship["similarity_ratio"], similarity)
        relationship["window_ids"].update((left.id, right.id))
        file_window_ids[left.path].add(left.id)
        file_window_ids[right.path].add(right.id)

    results: list[dict[str, Any]] = []
    for relationship in aggregated.values():
        relationship["similarity_ratio"] = round(float(relationship["similarity_ratio"]), 6)
        relationship["evidence_score"] = round(
            float(relationship["duplicate_token_mass"]) * float(relationship["similarity_ratio"]),
            3,
        )
        relationship["window_ids"] = sorted(relationship["window_ids"])
        results.append(relationship)
    return sorted(
        results,
        key=lambda item: (-item["evidence_score"], item["id"]),
    ), file_window_ids


def _build_commit_diffusion_records(
    commit_records: list[dict[str, Any]],
    repo_baselines: dict[str, float],
) -> list[dict[str, Any]]:
    files_denom = max(1.0, repo_baselines["p95_files_touched"])
    token_denom = max(1.0, repo_baselines["p95_token_delta_mass"])
    root_denom = max(1.0, repo_baselines["p95_top_level_root_spread"])
    entropy_denom = max(1.0, repo_baselines["p95_change_entropy"])
    diffusion_records: list[dict[str, Any]] = []
    for record in commit_records:
        normalized_files = min(1.0, float(record["file_count"]) / files_denom)
        normalized_tokens = min(1.0, float(record["total_token_delta"]) / token_denom)
        normalized_roots = min(1.0, float(record["top_level_root_count"]) / root_denom)
        normalized_entropy = min(1.0, float(record["change_entropy"]) / entropy_denom)
        normalized_metrics = [
            normalized_files,
            normalized_tokens,
            normalized_roots,
            normalized_entropy,
        ]
        score = sum(normalized_metrics) / len(normalized_metrics)
        is_high_diffusion = score >= 0.75 or sum(value >= 0.8 for value in normalized_metrics) >= 2
        diffusion_records.append(
            {
                **record,
                "normalized_files_touched": _round6(normalized_files),
                "normalized_token_delta_mass": _round6(normalized_tokens),
                "normalized_root_spread": _round6(normalized_roots),
                "normalized_change_entropy": _round6(normalized_entropy),
                "diffusion_score": _round6(score),
                "is_high_diffusion": is_high_diffusion,
            }
        )
    return diffusion_records


def _build_cochange_graph(
    commit_records: list[dict[str, Any]],
    repo_baselines: dict[str, float],
) -> dict[str, Any]:
    file_commit_counts: dict[str, int] = defaultdict(int)
    pair_support: dict[tuple[str, str], int] = defaultdict(int)
    pair_high_diffusion_support: dict[tuple[str, str], int] = defaultdict(int)
    degenerate_threshold = max(1.0, repo_baselines["p99_files_touched"])
    eligible_commit_count = 0

    for record in commit_records:
        paths = sorted({file_record["path"] for file_record in record["files"]})
        for path in paths:
            file_commit_counts[path] += 1
        if float(record["file_count"]) > degenerate_threshold:
            continue
        eligible_commit_count += 1
        is_high_diffusion = bool(record.get("is_high_diffusion"))
        for pair in combinations(paths, 2):
            pair_support[pair] += 1
            if is_high_diffusion:
                pair_high_diffusion_support[pair] += 1

    payload_pairs: list[dict[str, Any]] = []
    for pair, support_count in pair_support.items():
        left_count = file_commit_counts[pair[0]]
        right_count = file_commit_counts[pair[1]]
        expected_support = (
            (left_count * right_count) / eligible_commit_count if eligible_commit_count else 0.0
        )
        lift = (support_count / expected_support) if expected_support > 0 else 0.0
        payload_pairs.append(
            {
                "source_path": pair[0],
                "target_path": pair[1],
                "support_count": support_count,
                "high_diffusion_support_count": pair_high_diffusion_support.get(pair, 0),
                "source_commit_count": left_count,
                "target_commit_count": right_count,
                "expected_support": _round6(expected_support),
                "lift_score": _round6(lift),
                "crosses_top_level_boundary": _crosses_top_level_boundary(*pair),
            }
        )

    return {
        "eligible_commit_count": eligible_commit_count,
        "file_commit_counts": dict(sorted(file_commit_counts.items())),
        "pair_metrics": sorted(
            payload_pairs,
            key=lambda item: (
                -item["support_count"],
                -item["lift_score"],
                item["source_path"],
                item["target_path"],
            ),
        ),
    }


def _build_temporal_coupling_edges(cochange_graph: dict[str, Any]) -> list[dict[str, Any]]:
    edges: list[dict[str, Any]] = []
    for pair_metric in cochange_graph["pair_metrics"]:
        if pair_metric["support_count"] < MIN_COCHANGE_SUPPORT:
            continue
        if float(pair_metric["lift_score"]) < MIN_COUPLING_LIFT:
            continue
        evidence_score = float(pair_metric["support_count"]) * float(pair_metric["lift_score"])
        edges.append(
            {
                "id": _relationship_id(
                    "temporal_coupling_edge",
                    pair_metric["source_path"],
                    pair_metric["target_path"],
                ),
                "kind": "temporal_coupling_edge",
                "source_path": pair_metric["source_path"],
                "target_path": pair_metric["target_path"],
                "support_count": int(pair_metric["support_count"]),
                "high_diffusion_support_count": int(
                    pair_metric["high_diffusion_support_count"]
                ),
                "lift_score": _round6(pair_metric["lift_score"]),
                "evidence_score": round(evidence_score, 3),
                "crosses_top_level_boundary": bool(
                    pair_metric["crosses_top_level_boundary"]
                ),
            }
        )
    return sorted(edges, key=lambda item: (-item["evidence_score"], item["id"]))


def _file_vocab(record: dict[str, Any]) -> set[str]:
    normalized_text = _normalize_text(record["text"])
    vocab = {word for word in WORD_RE.findall(normalized_text) if word not in STOPWORDS}
    path_parts = re.split(r"[/_.-]+", record["path"].lower())
    vocab.update(part for part in path_parts if len(part) >= 4 and part not in STOPWORDS)
    return vocab


def _build_lexical_affinity_edges(records: list[dict[str, Any]]) -> list[dict[str, Any]]:
    vocab_by_path = {record["path"]: _file_vocab(record) for record in records}
    inverted: dict[str, list[str]] = defaultdict(list)
    for path, vocab in vocab_by_path.items():
        for token in sorted(vocab):
            inverted[token].append(path)

    pair_counts: dict[tuple[str, str], float] = defaultdict(float)
    pair_shared_tokens: dict[tuple[str, str], set[str]] = defaultdict(set)
    for token, paths in inverted.items():
        if len(paths) < 2 or len(paths) > 20:
            continue
        weight = 1.0 / math.log2(2 + len(paths))
        for pair in combinations(sorted(paths), 2):
            pair_counts[pair] += weight
            pair_shared_tokens[pair].add(token)

    edges: list[dict[str, Any]] = []
    for pair, weighted_count in pair_counts.items():
        shared_tokens = pair_shared_tokens[pair]
        if len(shared_tokens) < 3:
            continue
        union = vocab_by_path[pair[0]] | vocab_by_path[pair[1]]
        if not union:
            continue
        jaccard = len(shared_tokens) / len(union)
        if jaccard < 0.15:
            continue
        edges.append(
            {
                "id": _relationship_id("lexical_affinity_edge", *pair),
                "kind": "lexical_affinity_edge",
                "source_path": pair[0],
                "target_path": pair[1],
                "shared_token_count": len(shared_tokens),
                "shared_tokens": sorted(shared_tokens)[:12],
                "similarity_ratio": _round6(jaccard),
                "evidence_score": round(weighted_count + jaccard, 3),
                "crosses_top_level_boundary": _crosses_top_level_boundary(*pair),
            }
        )
    return sorted(edges, key=lambda item: (-item["evidence_score"], item["id"]))


def _build_boundary_leakage_edges(
    duplicate_edges: list[dict[str, Any]],
    near_duplicate_edges: list[dict[str, Any]],
    coupling_edges: list[dict[str, Any]],
    lexical_edges: list[dict[str, Any]],
) -> list[dict[str, Any]]:
    aggregated: dict[tuple[str, str], dict[str, Any]] = {}
    source_groups = {
        "duplicate_neighborhood": duplicate_edges,
        "near_duplicate_neighborhood": near_duplicate_edges,
        "temporal_coupling_edge": coupling_edges,
        "lexical_affinity_edge": lexical_edges,
    }
    for edge_kind, edges in source_groups.items():
        for edge in edges:
            if not bool(edge["crosses_top_level_boundary"]):
                continue
            pair = _pair_key(edge["source_path"], edge["target_path"])
            aggregate = aggregated.setdefault(
                pair,
                {
                    "id": _relationship_id("boundary_leakage_edge", *pair),
                    "kind": "boundary_leakage_edge",
                    "source_path": pair[0],
                    "target_path": pair[1],
                    "crosses_top_level_boundary": True,
                    "source_relationship_ids": [],
                    "source_kinds": set(),
                    "evidence_score": 0.0,
                },
            )
            aggregate["source_relationship_ids"].append(edge["id"])
            aggregate["source_kinds"].add(edge_kind)
            aggregate["evidence_score"] += float(edge["evidence_score"])

    edges: list[dict[str, Any]] = []
    for aggregate in aggregated.values():
        if not aggregate["source_kinds"]:
            continue
        edges.append(
            {
                "id": aggregate["id"],
                "kind": aggregate["kind"],
                "source_path": aggregate["source_path"],
                "target_path": aggregate["target_path"],
                "crosses_top_level_boundary": True,
                "source_relationship_ids": sorted(aggregate["source_relationship_ids"]),
                "source_kind_count": len(aggregate["source_kinds"]),
                "evidence_score": round(float(aggregate["evidence_score"]), 3),
            }
        )
    return sorted(edges, key=lambda item: (-item["evidence_score"], item["id"]))


def _component_clusters(
    *,
    kind: str,
    edges: list[dict[str, Any]],
    candidate_type: str,
) -> list[dict[str, Any]]:
    adjacency: dict[str, set[str]] = defaultdict(set)
    edge_ids_by_pair: dict[tuple[str, str], list[str]] = defaultdict(list)
    evidence_scores: dict[tuple[str, str], float] = defaultdict(float)
    for edge in edges:
        pair = _pair_key(edge["source_path"], edge["target_path"])
        adjacency[pair[0]].add(pair[1])
        adjacency[pair[1]].add(pair[0])
        edge_ids_by_pair[pair].append(edge["id"])
        evidence_scores[pair] += float(edge["evidence_score"])

    clusters: list[dict[str, Any]] = []
    visited: set[str] = set()
    for start in sorted(adjacency):
        if start in visited:
            continue
        stack = [start]
        members: set[str] = set()
        while stack:
            current = stack.pop()
            if current in visited:
                continue
            visited.add(current)
            members.add(current)
            stack.extend(sorted(adjacency[current] - visited, reverse=True))
        if len(members) < 2:
            continue
        member_paths = sorted(members)
        member_pairs = [
            pair for pair in edge_ids_by_pair if pair[0] in members and pair[1] in members
        ]
        source_relationship_ids = sorted(
            {
                edge_id
                for pair in member_pairs
                for edge_id in edge_ids_by_pair[pair]
            }
        )
        top_level_roots = sorted({_top_level_root(path) for path in member_paths})
        clusters.append(
            {
                "id": _cluster_id(kind, member_paths),
                "kind": kind,
                "member_paths": member_paths,
                "member_count": len(member_paths),
                "top_level_roots": top_level_roots,
                "evidence_score": round(
                    sum(evidence_scores[pair] for pair in member_pairs),
                    3,
                ),
                "source_relationship_ids": source_relationship_ids,
                "candidate_type": candidate_type,
            }
        )
    return sorted(clusters, key=lambda item: (-item["evidence_score"], item["id"]))


def _build_consolidation_candidates(
    duplicate_sets: list[dict[str, Any]],
    scattered_concepts: list[dict[str, Any]],
    boundary_clusters: list[dict[str, Any]],
) -> list[dict[str, Any]]:
    candidates: dict[tuple[str, ...], dict[str, Any]] = {}

    def add_candidate(cluster: dict[str, Any]) -> None:
        key = tuple(cluster["member_paths"])
        existing = candidates.get(key)
        if existing is None or float(cluster["evidence_score"]) > float(existing["evidence_score"]):
            candidates[key] = dict(cluster)

    for cluster in duplicate_sets:
        candidate = dict(cluster)
        candidate["kind"] = "consolidation_candidate"
        candidate["candidate_type"] = "consolidate_duplicate_knowledge"
        add_candidate(candidate)
    for cluster in scattered_concepts:
        candidate = dict(cluster)
        candidate["kind"] = "consolidation_candidate"
        candidate["candidate_type"] = "reduce_scattered_concept"
        add_candidate(candidate)
    for cluster in boundary_clusters:
        candidate = dict(cluster)
        candidate["kind"] = "consolidation_candidate"
        candidate["candidate_type"] = "extract_boundary"
        add_candidate(candidate)

    return sorted(candidates.values(), key=lambda item: (-item["evidence_score"], item["id"]))


def _build_clusters(
    duplicate_edges: list[dict[str, Any]],
    near_duplicate_edges: list[dict[str, Any]],
    coupling_edges: list[dict[str, Any]],
    lexical_edges: list[dict[str, Any]],
    boundary_edges: list[dict[str, Any]],
) -> dict[str, list[dict[str, Any]]]:
    duplicate_sets = _component_clusters(
        kind="duplicate_set",
        edges=duplicate_edges + near_duplicate_edges,
        candidate_type="consolidate_duplicate_knowledge",
    )
    scattered_concepts = _component_clusters(
        kind="scattered_concept",
        edges=duplicate_edges + near_duplicate_edges + coupling_edges + lexical_edges,
        candidate_type="reduce_scattered_concept",
    )
    boundary_clusters = [
        cluster
        for cluster in _component_clusters(
            kind="boundary_leakage_cluster",
            edges=boundary_edges,
            candidate_type="extract_boundary",
        )
        if len(cluster["top_level_roots"]) > 1
    ]
    consolidation_candidates = _build_consolidation_candidates(
        duplicate_sets,
        scattered_concepts,
        boundary_clusters,
    )
    return {
        "duplicate_sets": duplicate_sets,
        "scattered_concepts": scattered_concepts,
        "boundary_leakage_clusters": boundary_clusters,
        "consolidation_candidates": consolidation_candidates,
    }


def _path_relationship_index(
    relationship_groups: dict[str, list[dict[str, Any]]],
) -> dict[str, dict[str, list[dict[str, Any]]]]:
    index: dict[str, dict[str, list[dict[str, Any]]]] = defaultdict(
        lambda: defaultdict(list)
    )
    for kind, edges in relationship_groups.items():
        for edge in edges:
            index[edge["source_path"]][kind].append(edge)
            index[edge["target_path"]][kind].append(edge)
    for path_groups in index.values():
        for kind, edges in path_groups.items():
            path_groups[kind] = sorted(
                edges,
                key=lambda item: (-item["evidence_score"], item["id"]),
            )
    return index


def _path_cluster_index(cluster_groups: dict[str, list[dict[str, Any]]]) -> dict[str, list[str]]:
    index: dict[str, set[str]] = defaultdict(set)
    for clusters in cluster_groups.values():
        for cluster in clusters:
            for member_path in cluster["member_paths"]:
                index[member_path].add(cluster["id"])
    return {path: sorted(cluster_ids) for path, cluster_ids in index.items()}


def _build_file_overlay_baselines(raw_metrics: list[dict[str, Any]]) -> dict[str, float]:
    return {
        "p95_duplicate_token_ratio": _percentile(
            [float(metric["duplicate_token_ratio"]) for metric in raw_metrics],
            0.95,
        ),
        "p95_high_diffusion_commit_count": _percentile(
            [float(metric["high_diffusion_commit_count"]) for metric in raw_metrics],
            0.95,
        ),
        "p95_coupling_signal": _percentile(
            [float(metric["coupling_signal"]) for metric in raw_metrics],
            0.95,
        ),
        "p95_cross_boundary_edge_count": _percentile(
            [float(metric["cross_boundary_edge_count"]) for metric in raw_metrics],
            0.95,
        ),
    }


def _build_file_overlays(
    records: list[dict[str, Any]],
    duplicate_window_ids: dict[str, set[str]],
    near_duplicate_window_ids: dict[str, set[str]],
    diffusion_records: list[dict[str, Any]],
    temporal_coupling_edges: list[dict[str, Any]],
    boundary_edges: list[dict[str, Any]],
    relationship_index: dict[str, dict[str, list[dict[str, Any]]]],
    cluster_index: dict[str, list[str]],
    history_baselines: dict[str, float],
) -> tuple[list[dict[str, Any]], dict[str, Any]]:
    high_diffusion_counts: dict[str, int] = defaultdict(int)
    for commit in diffusion_records:
        if not bool(commit["is_high_diffusion"]):
            continue
        for file_record in commit["files"]:
            high_diffusion_counts[file_record["path"]] += 1

    coupling_signal_by_path: dict[str, float] = defaultdict(float)
    for edge in temporal_coupling_edges:
        signal = float(edge["support_count"]) * float(edge["lift_score"])
        coupling_signal_by_path[edge["source_path"]] += signal
        coupling_signal_by_path[edge["target_path"]] += signal

    cross_boundary_counts: dict[str, int] = defaultdict(int)
    for edge in boundary_edges:
        cross_boundary_counts[edge["source_path"]] += 1
        cross_boundary_counts[edge["target_path"]] += 1

    raw_metrics: list[dict[str, Any]] = []
    for record in records:
        duplicate_window_count = len(
            duplicate_window_ids.get(record["path"], set()) | near_duplicate_window_ids.get(
                record["path"],
                set(),
            )
        )
        duplicate_token_ratio = (
            (duplicate_window_count * WINDOW_SIZE) / max(int(record["tokens"]), 1)
            if int(record["tokens"]) > 0
            else 0.0
        )
        raw_metrics.append(
            {
                "path": record["path"],
                "duplicate_token_ratio": min(1.0, duplicate_token_ratio),
                "high_diffusion_commit_count": high_diffusion_counts.get(record["path"], 0),
                "coupling_signal": coupling_signal_by_path.get(record["path"], 0.0),
                "cross_boundary_edge_count": cross_boundary_counts.get(record["path"], 0),
            }
        )

    overlay_baselines = _build_file_overlay_baselines(raw_metrics)
    duplication_denom = max(overlay_baselines["p95_duplicate_token_ratio"], 1e-6)
    diffusion_denom = max(overlay_baselines["p95_high_diffusion_commit_count"], 1.0)
    coupling_denom = max(overlay_baselines["p95_coupling_signal"], 1.0)
    boundary_denom = max(overlay_baselines["p95_cross_boundary_edge_count"], 1.0)

    file_overlays: list[dict[str, Any]] = []
    for raw_metric in sorted(raw_metrics, key=lambda item: item["path"]):
        path = raw_metric["path"]
        duplicate_relationship_ids = [
            edge["id"]
            for edge in relationship_index.get(path, {}).get("duplicate_neighborhoods", [])
        ] + [
            edge["id"]
            for edge in relationship_index.get(path, {}).get("near_duplicate_neighborhoods", [])
        ]
        coupling_relationship_ids = [
            edge["id"]
            for edge in relationship_index.get(path, {}).get("temporal_coupling_edges", [])
        ]
        file_overlays.append(
            {
                "path": path,
                "duplication_pressure": _round6(
                    min(1.0, float(raw_metric["duplicate_token_ratio"]) / duplication_denom)
                ),
                "diffusion_pressure": _round6(
                    min(
                        1.0,
                        float(raw_metric["high_diffusion_commit_count"]) / diffusion_denom,
                    )
                ),
                "coupling_pressure": _round6(
                    min(1.0, float(raw_metric["coupling_signal"]) / coupling_denom)
                ),
                "boundary_pressure": _round6(
                    min(
                        1.0,
                        float(raw_metric["cross_boundary_edge_count"]) / boundary_denom,
                    )
                ),
                "top_duplicate_relationship_ids": duplicate_relationship_ids[:5],
                "top_coupling_relationship_ids": coupling_relationship_ids[:5],
                "cluster_ids": cluster_index.get(path, []),
                "duplicate_token_ratio": _round6(raw_metric["duplicate_token_ratio"]),
                "high_diffusion_commit_count": int(raw_metric["high_diffusion_commit_count"]),
                "cross_boundary_edge_count": int(raw_metric["cross_boundary_edge_count"]),
            }
        )

    return file_overlays, {
        "history": {
            key: _round6(value) for key, value in history_baselines.items()
        },
        "organization": {
            key: _round6(value) for key, value in overlay_baselines.items()
        },
    }


def _folder_paths_for_file(path: str) -> list[str]:
    pure_path = PurePosixPath(path)
    parents = ["."]
    current = pure_path.parent
    while str(current) not in ("", "."):
        parents.append(current.as_posix())
        current = current.parent
    return parents


def _build_folder_overlays(file_overlays: list[dict[str, Any]]) -> list[dict[str, Any]]:
    grouped: dict[str, list[dict[str, Any]]] = defaultdict(list)
    for overlay in file_overlays:
        for folder_path in _folder_paths_for_file(overlay["path"]):
            grouped[folder_path].append(overlay)

    folder_overlays: list[dict[str, Any]] = []
    for folder_path, overlays in grouped.items():
        top_overlay = max(
            overlays,
            key=lambda item: (
                max(
                    item["duplication_pressure"],
                    item["diffusion_pressure"],
                    item["coupling_pressure"],
                    item["boundary_pressure"],
                ),
                item["path"],
            ),
        )
        duplicate_relationship_ids = []
        coupling_relationship_ids = []
        cluster_ids = []
        for overlay in sorted(overlays, key=lambda item: item["path"]):
            for relationship_id in overlay["top_duplicate_relationship_ids"]:
                if relationship_id not in duplicate_relationship_ids:
                    duplicate_relationship_ids.append(relationship_id)
            for relationship_id in overlay["top_coupling_relationship_ids"]:
                if relationship_id not in coupling_relationship_ids:
                    coupling_relationship_ids.append(relationship_id)
            for cluster_id in overlay["cluster_ids"]:
                if cluster_id not in cluster_ids:
                    cluster_ids.append(cluster_id)
        folder_overlays.append(
            {
                "path": folder_path,
                "descendant_file_count": len(overlays),
                "duplication_pressure": max(
                    float(overlay["duplication_pressure"]) for overlay in overlays
                ),
                "diffusion_pressure": max(
                    float(overlay["diffusion_pressure"]) for overlay in overlays
                ),
                "coupling_pressure": max(
                    float(overlay["coupling_pressure"]) for overlay in overlays
                ),
                "boundary_pressure": max(
                    float(overlay["boundary_pressure"]) for overlay in overlays
                ),
                "top_duplicate_relationship_ids": duplicate_relationship_ids[:5],
                "top_coupling_relationship_ids": coupling_relationship_ids[:5],
                "cluster_ids": cluster_ids,
                "duplicate_token_ratio": max(
                    float(overlay["duplicate_token_ratio"]) for overlay in overlays
                ),
                "high_diffusion_commit_count": sum(
                    int(overlay["high_diffusion_commit_count"]) for overlay in overlays
                ),
                "cross_boundary_edge_count": sum(
                    int(overlay["cross_boundary_edge_count"]) for overlay in overlays
                ),
                "top_file_path": top_overlay["path"],
            }
        )
    return sorted(folder_overlays, key=lambda item: (item["path"] != ".", item["path"]))


def _relation_sections(relationship_groups: dict[str, list[dict[str, Any]]]) -> dict[str, Any]:
    return {
        "analysis_status": ANALYSIS_STATUS,
        "analysis_version": ANALYSIS_VERSION,
        "duplicate_neighborhoods": relationship_groups["duplicate_neighborhoods"],
        "near_duplicate_neighborhoods": relationship_groups["near_duplicate_neighborhoods"],
        "temporal_coupling_edges": relationship_groups["temporal_coupling_edges"],
        "lexical_affinity_edges": relationship_groups["lexical_affinity_edges"],
        "boundary_leakage_edges": relationship_groups["boundary_leakage_edges"],
    }


def _cluster_sections(cluster_groups: dict[str, list[dict[str, Any]]]) -> dict[str, Any]:
    return {
        "analysis_status": ANALYSIS_STATUS,
        "analysis_version": ANALYSIS_VERSION,
        "duplicate_sets": cluster_groups["duplicate_sets"],
        "scattered_concepts": cluster_groups["scattered_concepts"],
        "boundary_leakage_clusters": cluster_groups["boundary_leakage_clusters"],
        "consolidation_candidates": cluster_groups["consolidation_candidates"],
    }


def build_organization_health(
    repo_root: Path,
    records: list[dict[str, Any]],
    history_snapshot: dict[str, Any],
    config: dict[str, Any],
) -> dict[str, Any]:
    cache_key = _analysis_cache_key(repo_root, records, config)
    organization_cache_root = _cache_root(repo_root, cache_key)
    return _load_or_compute_cached(
        organization_cache_root / "organization_sections.json",
        lambda: _build_organization_sections(
            organization_cache_root,
            records,
            history_snapshot,
            config,
        ),
    )


def _build_organization_sections(
    organization_cache_root: Path,
    records: list[dict[str, Any]],
    history_snapshot: dict[str, Any],
    config: dict[str, Any],
) -> dict[str, Any]:
    serialized_windows = _load_or_compute_cached(
        organization_cache_root / "normalized_token_windows.json",
        lambda: _build_window_records(records, config),
    )
    windows = _deserialize_window_records(serialized_windows)
    exact_index = _load_or_compute_cached(
        organization_cache_root / "exact_duplicate_index.json",
        lambda: _build_exact_index(windows),
    )
    near_signature_index = _load_or_compute_cached(
        organization_cache_root / "near_duplicate_signature_index.json",
        lambda: _build_near_signature_index(windows),
    )
    diffusion_records = _load_or_compute_cached(
        organization_cache_root / "commit_diffusion_records.json",
        lambda: _build_commit_diffusion_records(
            history_snapshot["commit_records"],
            history_snapshot["repo_baselines"],
        ),
    )
    cochange_graph = _load_or_compute_cached(
        organization_cache_root / "cochange_graph.json",
        lambda: _build_cochange_graph(diffusion_records, history_snapshot["repo_baselines"]),
    )

    duplicate_edges, duplicate_window_ids = _build_duplicate_relationships(windows, exact_index)
    near_duplicate_edges, near_duplicate_window_ids = _build_near_duplicate_relationships(
        windows,
        near_signature_index,
    )
    temporal_coupling_edges = _build_temporal_coupling_edges(cochange_graph)
    lexical_affinity_edges = _build_lexical_affinity_edges(records)
    boundary_leakage_edges = _build_boundary_leakage_edges(
        duplicate_edges,
        near_duplicate_edges,
        temporal_coupling_edges,
        lexical_affinity_edges,
    )

    relationship_groups = {
        "duplicate_neighborhoods": duplicate_edges,
        "near_duplicate_neighborhoods": near_duplicate_edges,
        "temporal_coupling_edges": temporal_coupling_edges,
        "lexical_affinity_edges": lexical_affinity_edges,
        "boundary_leakage_edges": boundary_leakage_edges,
    }
    cluster_groups = _build_clusters(
        duplicate_edges,
        near_duplicate_edges,
        temporal_coupling_edges,
        lexical_affinity_edges,
        boundary_leakage_edges,
    )
    relationship_index = _path_relationship_index(relationship_groups)
    cluster_index = _path_cluster_index(cluster_groups)
    file_overlays, repo_baselines = _build_file_overlays(
        records,
        duplicate_window_ids,
        near_duplicate_window_ids,
        diffusion_records,
        temporal_coupling_edges,
        boundary_leakage_edges,
        relationship_index,
        cluster_index,
        history_snapshot["repo_baselines"],
    )
    folder_overlays = _build_folder_overlays(file_overlays)

    return {
        "organization_metrics": {
            "analysis_status": ANALYSIS_STATUS,
            "analysis_version": ANALYSIS_VERSION,
            "repo_baselines": repo_baselines,
            "files": file_overlays,
            "folders": folder_overlays,
        },
        "relationships": _relation_sections(relationship_groups),
        "clusters": _cluster_sections(cluster_groups),
    }


def _overlay_sort_key(overlay: dict[str, Any]) -> tuple[int, float, str]:
    pressures = [
        float(overlay["duplication_pressure"]),
        float(overlay["diffusion_pressure"]),
        float(overlay["coupling_pressure"]),
        float(overlay["boundary_pressure"]),
    ]
    return (sum(value >= 1.0 for value in pressures), max(pressures), overlay["path"])


def top_organization_file_overlays(
    report: dict[str, Any],
    *,
    limit: int = 5,
) -> list[dict[str, Any]]:
    overlays = list(report["organization_metrics"]["files"])
    return sorted(overlays, key=lambda item: _overlay_sort_key(item), reverse=True)[:limit]


def relationships_for_path(report: dict[str, Any], target_path: str) -> list[dict[str, Any]]:
    matched: list[dict[str, Any]] = []
    for key in (
        "duplicate_neighborhoods",
        "near_duplicate_neighborhoods",
        "temporal_coupling_edges",
        "lexical_affinity_edges",
        "boundary_leakage_edges",
    ):
        for relationship in report["relationships"][key]:
            if (
                relationship["source_path"] == target_path
                or relationship["target_path"] == target_path
            ):
                matched.append(relationship)
    return sorted(matched, key=lambda item: (-item["evidence_score"], item["id"]))


def clusters_for_path(report: dict[str, Any], target_path: str) -> list[dict[str, Any]]:
    matched: list[dict[str, Any]] = []
    for key in (
        "duplicate_sets",
        "scattered_concepts",
        "boundary_leakage_clusters",
        "consolidation_candidates",
    ):
        for cluster in report["clusters"][key]:
            if target_path in cluster["member_paths"]:
                matched.append(cluster)
    return sorted(matched, key=lambda item: (-item["evidence_score"], item["id"]))


def folder_relationships_for_prefix(
    report: dict[str, Any],
    folder_path: str,
) -> list[dict[str, Any]]:
    prefix = "" if folder_path == "." else f"{folder_path.rstrip('/')}/"
    matched: list[dict[str, Any]] = []
    for key in (
        "duplicate_neighborhoods",
        "near_duplicate_neighborhoods",
        "temporal_coupling_edges",
        "lexical_affinity_edges",
        "boundary_leakage_edges",
    ):
        for relationship in report["relationships"][key]:
            if (
                relationship["source_path"].startswith(prefix)
                or relationship["target_path"].startswith(prefix)
            ):
                matched.append(relationship)
    return sorted(matched, key=lambda item: (-item["evidence_score"], item["id"]))


def folder_clusters_for_prefix(report: dict[str, Any], folder_path: str) -> list[dict[str, Any]]:
    prefix = "" if folder_path == "." else f"{folder_path.rstrip('/')}/"
    matched: list[dict[str, Any]] = []
    for key in (
        "duplicate_sets",
        "scattered_concepts",
        "boundary_leakage_clusters",
        "consolidation_candidates",
    ):
        for cluster in report["clusters"][key]:
            if any(path.startswith(prefix) for path in cluster["member_paths"]):
                matched.append(cluster)
    return sorted(matched, key=lambda item: (-item["evidence_score"], item["id"]))
