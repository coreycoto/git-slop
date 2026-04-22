from __future__ import annotations

from collections import Counter, defaultdict
from typing import Iterable


def weighted_jaccard(left: Counter[str], right: Counter[str]) -> float:
    shared = 0
    total = 0
    for token in set(left) | set(right):
        left_count = left.get(token, 0)
        right_count = right.get(token, 0)
        shared += min(left_count, right_count)
        total += max(left_count, right_count)
    return shared / total if total else 0.0


def document_frequency(token_sets: dict[str, set[str]]) -> dict[str, int]:
    counts: dict[str, int] = defaultdict(int)
    for tokens in token_sets.values():
        for token in tokens:
            counts[token] += 1
    return dict(counts)


def term_dispersion_by_root(
    path_tokens: dict[str, Iterable[str]],
    path_roots: dict[str, str],
) -> dict[str, int]:
    roots_by_token: dict[str, set[str]] = defaultdict(set)
    for path, tokens in path_tokens.items():
        root = path_roots[path]
        for token in set(tokens):
            roots_by_token[token].add(root)
    return {token: len(roots) for token, roots in roots_by_token.items()}
