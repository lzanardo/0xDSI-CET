"""Multi-query CET runtime for production use.

This layer runs a registry of CET v4 queries over a normalized security graph,
uses the native C bridge for fast sequence matching, then validates DSL features
that are intentionally kept outside the C hot path.
"""
from __future__ import annotations

from dataclasses import dataclass
from typing import Any, Iterable
import hashlib

from .bridge import CETBridge, CETMatch
from .dsl import QuerySpec, filter_events_for_query, validate_path
from .features import extract_trend_features
from .query_registry import QueryRegistry
from .risk import score_trend
from .security_graph import SecurityGraphBuilder
from .event_contracts import validate_events


@dataclass(frozen=True)
class RuntimeOptions:
    switch_depth: int = 2
    native_threads: int = 1
    enable_mmap_arena: bool = False
    mmap_workspace_bytes: int = 0
    raise_on_overflow: bool = False


def make_trend_id(query: QuerySpec, partition_key: str, path: list[int]) -> str:
    raw = f"{query.query_id}:{query.version}:{partition_key}:{','.join(map(str, path))}"
    return hashlib.sha256(raw.encode("utf-8")).hexdigest()


class CETRuntimeV4:
    def __init__(
        self,
        registry: QueryRegistry,
        bridge: CETBridge | Any | None = None,
        graph_builder: SecurityGraphBuilder | None = None,
        options: RuntimeOptions | None = None,
    ):
        self.registry = registry
        self.bridge = bridge or CETBridge()
        self.graph_builder = graph_builder or SecurityGraphBuilder()
        self.options = options or RuntimeOptions()
        self.registry.validate_unique_versions()

    def run(self, raw_events: Iterable[dict[str, Any]]) -> list[dict[str, Any]]:
        events = list(raw_events)
        validate_events(events)
        trends: list[dict[str, Any]] = []
        for query in self.registry.enabled():
            trends.extend(self._run_query(query, events))
        return sorted(trends, key=lambda t: (t["query_id"], t["partition_key"], t["trend_id"]))

    def _run_query(self, query: QuerySpec, all_events: list[dict[str, Any]]) -> list[dict[str, Any]]:
        candidate_events = filter_events_for_query(query, all_events)
        if not candidate_events:
            return []
        graph = self.graph_builder.build(candidate_events)
        native_query = self.bridge.parse_query(query.native_name(), query.native_pattern(), query.within_ms, query.slide_ms)
        match: CETMatch = self.bridge.run_hcet(
            native_query,
            graph.native_events,
            graph.native_edges,
            switch_depth=self.options.switch_depth,
            native_threads=self.options.native_threads,
            enable_mmap_arena=self.options.enable_mmap_arena,
            mmap_workspace_bytes=self.options.mmap_workspace_bytes,
            raise_on_overflow=self.options.raise_on_overflow,
        )
        by_id = graph.events_by_id()
        out: list[dict[str, Any]] = []
        for path in match.paths:
            path_events = [by_id[eid] for eid in path if eid in by_id]
            if len(path_events) != len(path):
                continue
            if not validate_path(query, path_events, candidate_events):
                continue
            partition_key = str(path_events[0].get("partition_key"))
            features = extract_trend_features(path_events)
            risk = score_trend(query, path_events, match.stats)
            out.append({
                "trend_id": make_trend_id(query, partition_key, path),
                "query_id": query.query_id,
                "query_version": query.version,
                "partition_key": partition_key,
                "path": path,
                "trend_start_time_ms": features["trend_start_time_ms"],
                "trend_end_time_ms": features["trend_end_time_ms"],
                "event_count": features["event_count"],
                "risk_score": risk["risk_score"],
                "severity": risk["severity"],
                "features": features,
                "risk_explanation": risk["explanation"],
                "native_stats": match.stats,
                "dsl_native_fallback_features": sorted(query.unsupported_native_features()),
            })
        return out
