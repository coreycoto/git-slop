from __future__ import annotations

from collections import Counter, defaultdict
from pathlib import PurePosixPath
from typing import Any

from git_slop.core.models import RepositoryFacts
from git_slop.costs.protocols import OverlayAnalyzer
from git_slop.graphs.token_similarity import document_frequency, term_dispersion_by_root


class NavigationOverlayAnalyzer(OverlayAnalyzer):
    id = "navigation"
    version = "1"
    experimental = True

    def analyze(self, facts: RepositoryFacts) -> dict[str, Any]:
        token_facts = facts.token_facts_by_path()
        file_paths = [record["path"] for record in facts.file_records]
        token_sets = {
            path: set(token_facts[path].structural_tokens)
            for path in file_paths
        }
        doc_freq = document_frequency(token_sets)
        path_roots = {
            path: (PurePosixPath(path).parts[0] if PurePosixPath(path).parts else ".")
            for path in file_paths
        }
        dispersion = term_dispersion_by_root(
            {path: token_facts[path].structural_tokens for path in file_paths},
            path_roots,
        )
        sibling_counts: dict[str, int] = defaultdict(int)
        basename_counts = Counter(PurePosixPath(path).name for path in file_paths)
        for path in file_paths:
            parent = PurePosixPath(path).parent.as_posix() or "."
            sibling_counts[parent] += 1
        file_overlays: list[dict[str, Any]] = []
        total_files = max(1, len(file_paths))
        total_roots = max(1, len(set(path_roots.values())))
        for path in file_paths:
            token_fact = token_facts[path]
            top_terms = token_fact.top_structural_terms[
                : int(facts.config["navigation"]["top_distinctive_terms"])
            ]
            search_ambiguity = (
                sum(doc_freq.get(term, 0) for term in top_terms)
                / max(1, len(top_terms) * total_files)
            )
            term_dispersion = (
                sum(dispersion.get(term, 1) for term in top_terms)
                / max(1, len(top_terms) * total_roots)
            )
            parent = PurePosixPath(path).parent.as_posix() or "."
            path_depth = len(PurePosixPath(path).parts) - 1
            sibling_count = sibling_counts[parent]
            duplicate_name_count = basename_counts[PurePosixPath(path).name]
            navigation_pressure = min(
                1.0,
                (0.30 * min(1.0, path_depth / 8.0))
                + (0.30 * min(1.0, sibling_count / 20.0))
                + (0.25 * search_ambiguity)
                + (0.15 * term_dispersion),
            )
            file_overlays.append(
                {
                    "path": path,
                    "path_depth": path_depth,
                    "sibling_count": sibling_count,
                    "folder_width": sibling_count,
                    "search_ambiguity": round(search_ambiguity, 6),
                    "term_dispersion": round(term_dispersion, 6),
                    "duplicate_name_count": duplicate_name_count,
                    "navigation_pressure": round(navigation_pressure, 6),
                    "top_terms": top_terms,
                }
            )
        return {
            "analysis_status": "experimental",
            "analysis_version": 1,
            "files": sorted(file_overlays, key=lambda item: item["path"]),
        }
