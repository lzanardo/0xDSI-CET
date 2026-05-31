"""Operational metrics helpers for CET production runtimes."""
from __future__ import annotations

from dataclasses import dataclass, field
from time import perf_counter
from typing import Any, Iterator
from contextlib import contextmanager
import json


@dataclass
class Metric:
    metric_name: str
    metric_value: float
    query_id: str = "_all"
    dimensions: dict[str, Any] = field(default_factory=dict)

    def as_row(self, batch_id: int) -> tuple[int, str, str, float, str]:
        return (int(batch_id), self.query_id, self.metric_name, float(self.metric_value), json.dumps(self.dimensions, sort_keys=True))


@dataclass
class MetricsBuffer:
    metrics: list[Metric] = field(default_factory=list)

    def add(self, name: str, value: float, *, query_id: str = "_all", **dimensions: Any) -> None:
        self.metrics.append(Metric(name, float(value), query_id=query_id, dimensions=dimensions))

    @contextmanager
    def timer(self, name: str, *, query_id: str = "_all", **dimensions: Any) -> Iterator[None]:
        t0 = perf_counter()
        try:
            yield
        finally:
            self.add(name, (perf_counter() - t0) * 1000.0, query_id=query_id, **dimensions)

    def rows(self, batch_id: int) -> list[tuple[int, str, str, float, str]]:
        return [m.as_row(batch_id) for m in self.metrics]


def metrics_from_trends(trends: list[dict[str, Any]]) -> list[Metric]:
    out: list[Metric] = [Metric("trends_emitted", len(trends))]
    by_query: dict[str, int] = {}
    for t in trends:
        by_query[t.get("query_id", "unknown")] = by_query.get(t.get("query_id", "unknown"), 0) + 1
    for q, n in by_query.items():
        out.append(Metric("trends_emitted", n, query_id=q))
    overflow = sum(1 for t in trends if (t.get("native_stats") or {}).get("overflow"))
    out.append(Metric("native_overflow_trends", overflow))
    return out
