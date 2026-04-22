from __future__ import annotations

from collections import Counter, defaultdict
from pathlib import PurePosixPath
from typing import Any

from git_slop.core.models import RepositoryFacts
from git_slop.costs.protocols import OverlayAnalyzer


def _top_level_root(path: str) -> str:
    parts = PurePosixPath(path).parts
    return parts[0] if parts else "."


def _context_neighbors(tokens: list[str], term: str, *, radius: int = 3) -> Counter[str]:
    neighbors: Counter[str] = Counter()
    for index, token in enumerate(tokens):
        if token != term:
            continue
        start = max(0, index - radius)
        end = min(len(tokens), index + radius + 1)
        for neighbor in tokens[start:end]:
            if neighbor != term:
                neighbors[neighbor] += 1
    return neighbors


def _counter_jaccard(left: Counter[str], right: Counter[str]) -> float:
    shared = 0
    total = 0
    for key in set(left) | set(right):
        shared += min(left.get(key, 0), right.get(key, 0))
        total += max(left.get(key, 0), right.get(key, 0))
    return shared / total if total else 0.0


class SemanticDriftOverlayAnalyzer(OverlayAnalyzer):
    id = "semantic_drift"
    version = "1"
    experimental = True

    def analyze(self, facts: RepositoryFacts) -> dict[str, Any]:
        token_facts = facts.token_facts_by_path()
        root_terms: dict[str, Counter[str]] = defaultdict(Counter)
        file_terms: dict[str, Counter[str]] = {}
        for path, token_fact in token_facts.items():
            counter = Counter(token_fact.structural_tokens)
            file_terms[path] = counter
            root_terms[_top_level_root(path)].update(counter)
        candidate_terms = [
            term
            for term, count in Counter(
                term
                for counter in file_terms.values()
                for term, term_count in counter.items()
                if term_count >= 2
            ).most_common(int(facts.config["semantic_drift"]["top_term_limit"]))
        ]
        drift_entries: list[dict[str, Any]] = []
        term_scores: dict[str, float] = {}
        for term in candidate_terms:
            root_neighbors = {
                root: _context_neighbors(list(counter.elements()), term)
                for root, counter in root_terms.items()
                if counter.get(term, 0) > 0
            }
            if len(root_neighbors) < 2:
                continue
            similarities: list[float] = []
            roots = sorted(root_neighbors)
            for index, left_root in enumerate(roots):
                for right_root in roots[index + 1 :]:
                    similarities.append(
                        _counter_jaccard(root_neighbors[left_root], root_neighbors[right_root])
                    )
            drift_score = 1.0 - (sum(similarities) / max(1, len(similarities)))
            term_scores[term] = drift_score
            drift_entries.append(
                {
                    "term": term,
                    "roots": roots,
                    "drift_score": round(drift_score, 6),
                }
            )
        file_overlays: list[dict[str, Any]] = []
        for path, token_fact in token_facts.items():
            drift_terms = sorted(
                [term for term in token_fact.top_structural_terms if term in term_scores],
                key=lambda term: (-term_scores[term], term),
            )[:5]
            pressure = max((term_scores[term] for term in drift_terms), default=0.0)
            file_overlays.append(
                {
                    "path": path,
                    "drift_terms": drift_terms,
                    "drift_scores": {term: round(term_scores[term], 6) for term in drift_terms},
                    "semantic_drift_pressure": round(pressure, 6),
                }
            )
        return {
            "analysis_status": "experimental",
            "analysis_version": 1,
            "files": sorted(file_overlays, key=lambda item: item["path"]),
            "findings": sorted(
                drift_entries,
                key=lambda item: (-item["drift_score"], item["term"]),
            )[:25],
        }
