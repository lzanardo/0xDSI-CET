"""Counterfactual replay helpers for detection engineering."""
from __future__ import annotations

from dataclasses import dataclass
from typing import Any, Iterable, Protocol


class RuntimeLike(Protocol):
    def run(self, raw_events: Iterable[dict[str, Any]]) -> list[dict[str, Any]]: ...


@dataclass(frozen=True)
class TrendDiff:
    added: list[dict[str, Any]]
    removed: list[dict[str, Any]]
    common: list[dict[str, Any]]

    @property
    def summary(self) -> dict[str, int]:
        return {"added": len(self.added), "removed": len(self.removed), "common": len(self.common)}


def diff_trend_sets(base: Iterable[dict[str, Any]], candidate: Iterable[dict[str, Any]]) -> TrendDiff:
    b = {t["trend_id"]: t for t in base}
    c = {t["trend_id"]: t for t in candidate}
    added = [c[k] for k in sorted(c.keys() - b.keys())]
    removed = [b[k] for k in sorted(b.keys() - c.keys())]
    common = [c[k] for k in sorted(c.keys() & b.keys())]
    return TrendDiff(added=added, removed=removed, common=common)


def run_counterfactual(base_runtime: RuntimeLike, candidate_runtime: RuntimeLike, events: Iterable[dict[str, Any]]) -> dict[str, Any]:
    materialized = list(events)
    base = base_runtime.run(materialized)
    candidate = candidate_runtime.run(materialized)
    diff = diff_trend_sets(base, candidate)
    return {"base_count": len(base), "candidate_count": len(candidate), "diff": diff.summary, "added": diff.added, "removed": diff.removed}
