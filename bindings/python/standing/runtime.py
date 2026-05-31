"""Standing CET graph runtime.

The native C engine remains the low-level batch/path kernel. This module adds
an online graph-state runtime inspired by standing-query systems: events mutate a
long-lived temporal graph, registered CET queries are incrementally re-evaluated
around affected partitions, and the runtime emits both positive matches and
cancellations/retractions when a previously active trend disappears.

The implementation deliberately favors deterministic, auditable behavior over
clever concurrency. It is safe to run inside Spark partitions, in replay jobs, or
as a local ZeroBus consumer. Heavy deployments can shard instances by tenant or
partition_key.
"""
from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any, Iterable, Protocol
from collections import defaultdict
import hashlib
import json
import time

from bindings.python.dsl import QuerySpec, validate_path
from bindings.python.query_registry import QueryRegistry
from bindings.python.security_graph import ENTITY_KEYS, RELATION_BY_KEY
from bindings.python.features import extract_trend_features
from bindings.python.risk import score_trend


def _event_id(event: dict[str, Any]) -> int:
    return int(event["event_id"])


def _event_time(event: dict[str, Any]) -> int:
    return int(event.get("event_time_ms", 0))


def _partition(event: dict[str, Any]) -> str:
    return str(event.get("partition_key", "default"))


def _field(event: dict[str, Any], name: str) -> Any:
    if name in event:
        return event[name]
    attrs = event.get("attributes") or event.get("payload") or {}
    if isinstance(attrs, dict) and name in attrs:
        return attrs[name]
    cur: Any = attrs
    for part in name.split("."):
        if isinstance(cur, dict) and part in cur:
            cur = cur[part]
        else:
            return None
    return cur


def _fingerprint(query: QuerySpec, partition_key: str, path: Iterable[int]) -> str:
    raw = f"{query.query_id}:{query.version}:{partition_key}:{','.join(map(str, path))}"
    return hashlib.sha256(raw.encode("utf-8")).hexdigest()


@dataclass(frozen=True)
class GraphEdge:
    src: int
    dst: int
    relation: str
    event_time_ms: int
    evidence: dict[str, Any] = field(default_factory=dict)

    def as_dict(self) -> dict[str, Any]:
        return {
            "src": self.src,
            "dst": self.dst,
            "relation": self.relation,
            "event_time_ms": self.event_time_ms,
            "evidence": dict(self.evidence),
        }


@dataclass(frozen=True)
class GraphMutation:
    op: str
    event_id: int | None = None
    edge: GraphEdge | None = None
    partition_key: str | None = None
    revision: int = 0
    happened_at_ms: int = field(default_factory=lambda: int(time.time() * 1000))


@dataclass(frozen=True)
class StandingTrendOptions:
    max_window_events_per_partition: int = 20000
    max_matches_per_query_partition: int = 10000
    emit_cancellations: bool = True
    relation_aware: bool = True
    include_evidence_graph: bool = True
    default_max_gap_ms: int | None = 24 * 60 * 60 * 1000


@dataclass(frozen=True)
class StandingTrendEvent:
    event_type: str  # positive_match | cancel_match | state_update
    trend_id: str
    query_id: str
    query_version: str
    partition_key: str
    path: tuple[int, ...]
    revision: int
    trend: dict[str, Any]
    reason: str = ""

    def as_dict(self) -> dict[str, Any]:
        return {
            "event_type": self.event_type,
            "trend_id": self.trend_id,
            "query_id": self.query_id,
            "query_version": self.query_version,
            "partition_key": self.partition_key,
            "path": list(self.path),
            "revision": self.revision,
            "trend": self.trend,
            "reason": self.reason,
        }


class TrendSink(Protocol):
    def emit(self, event: StandingTrendEvent) -> None: ...


class TemporalGraphState:
    """Mutable temporal graph state for standing trend queries."""

    def __init__(self, entity_keys: Iterable[str] = ENTITY_KEYS, max_gap_ms: int | None = None):
        self.entity_keys = tuple(entity_keys)
        self.max_gap_ms = max_gap_ms
        self.events: dict[int, dict[str, Any]] = {}
        self.by_partition: dict[str, set[int]] = defaultdict(set)
        self.edges_by_src: dict[int, list[GraphEdge]] = defaultdict(list)
        self.edges_by_dst: dict[int, list[GraphEdge]] = defaultdict(list)
        self.revision = 0
        self.mutation_log: list[GraphMutation] = []

    def snapshot(self) -> dict[str, Any]:
        return {
            "revision": self.revision,
            "events": list(self.events.values()),
            "edges": [e.as_dict() for edges in self.edges_by_src.values() for e in edges],
        }

    def load_snapshot(self, snapshot: dict[str, Any]) -> None:
        self.events.clear(); self.by_partition.clear(); self.edges_by_src.clear(); self.edges_by_dst.clear(); self.mutation_log.clear()
        self.revision = int(snapshot.get("revision", 0))
        for event in snapshot.get("events", []):
            self._add_event_only(dict(event))
        for raw in snapshot.get("edges", []):
            edge = GraphEdge(
                src=int(raw["src"]),
                dst=int(raw["dst"]),
                relation=str(raw["relation"]),
                event_time_ms=int(raw.get("event_time_ms", 0)),
                evidence=dict(raw.get("evidence", {})),
            )
            self._add_edge_only(edge)

    def events_for_partition(self, partition_key: str) -> list[dict[str, Any]]:
        ids = self.by_partition.get(partition_key, set())
        events = [self.events[i] for i in ids if i in self.events]
        return sorted(events, key=lambda e: (_event_time(e), _event_id(e)))

    def edges_for_path(self, path: Iterable[int]) -> list[dict[str, Any]]:
        ids = list(path)
        wanted = set(zip(ids, ids[1:]))
        out: list[dict[str, Any]] = []
        for src, dst in wanted:
            for edge in self.edges_by_src.get(src, []):
                if edge.dst == dst:
                    out.append(edge.as_dict())
        return out

    def upsert_event(self, event: dict[str, Any]) -> GraphMutation:
        normalized = dict(event)
        normalized.setdefault("attributes", {})
        eid = _event_id(normalized)
        old_partition = _partition(self.events[eid]) if eid in self.events else None
        if old_partition is not None:
            self.by_partition[old_partition].discard(eid)
            self._remove_edges_touching(eid)
        self._add_event_only(normalized)
        self._rebuild_edges_for_event(eid)
        self.revision += 1
        mutation = GraphMutation("upsert_event", event_id=eid, partition_key=_partition(normalized), revision=self.revision)
        self.mutation_log.append(mutation)
        return mutation

    def remove_event(self, event_id: int) -> GraphMutation:
        partition_key = _partition(self.events[event_id]) if event_id in self.events else None
        if event_id in self.events:
            self.by_partition[partition_key or "default"].discard(event_id)
            del self.events[event_id]
            self._remove_edges_touching(event_id)
        self.revision += 1
        mutation = GraphMutation("remove_event", event_id=event_id, partition_key=partition_key, revision=self.revision)
        self.mutation_log.append(mutation)
        return mutation

    def _add_event_only(self, event: dict[str, Any]) -> None:
        eid = _event_id(event)
        self.events[eid] = event
        self.by_partition[_partition(event)].add(eid)

    def _add_edge_only(self, edge: GraphEdge) -> None:
        if edge.src == edge.dst:
            return
        if any(e.dst == edge.dst and e.relation == edge.relation for e in self.edges_by_src.get(edge.src, [])):
            return
        self.edges_by_src[edge.src].append(edge)
        self.edges_by_dst[edge.dst].append(edge)

    def _remove_edges_touching(self, event_id: int) -> None:
        for edge in list(self.edges_by_src.get(event_id, [])):
            self.edges_by_dst[edge.dst] = [e for e in self.edges_by_dst.get(edge.dst, []) if not (e.src == edge.src and e.relation == edge.relation)]
        for edge in list(self.edges_by_dst.get(event_id, [])):
            self.edges_by_src[edge.src] = [e for e in self.edges_by_src.get(edge.src, []) if not (e.dst == edge.dst and e.relation == edge.relation)]
        self.edges_by_src.pop(event_id, None)
        self.edges_by_dst.pop(event_id, None)

    def _rebuild_edges_for_event(self, event_id: int) -> None:
        event = self.events[event_id]
        partition_key = _partition(event)
        partition_events = self.events_for_partition(partition_key)
        idx = [i for i, e in enumerate(partition_events) if _event_id(e) == event_id]
        if idx:
            i = idx[0]
            for neighbor in [partition_events[j] for j in (i - 1, i + 1) if 0 <= j < len(partition_events)]:
                self._maybe_add_temporal_edge(event, neighbor)
        for key in self.entity_keys:
            value = _field(event, key)
            if value in (None, ""):
                continue
            relation = RELATION_BY_KEY.get(key, f"same_{key}")
            for other in self.events.values():
                if _event_id(other) == event_id:
                    continue
                if _field(other, key) == value:
                    self._maybe_add_relation_edge(event, other, relation, {"field": key, "value": value})
        parent_pid = _field(event, "parent_process_id")
        if parent_pid not in (None, ""):
            candidates = [e for e in self.events.values() if _field(e, "process_id") == parent_pid and _event_time(e) <= _event_time(event)]
            if candidates:
                parent = max(candidates, key=lambda e: (_event_time(e), _event_id(e)))
                self._maybe_add_relation_edge(parent, event, "parent_process", {"parent_process_id": parent_pid})

    def _within_gap(self, a: dict[str, Any], b: dict[str, Any]) -> bool:
        return self.max_gap_ms is None or abs(_event_time(a) - _event_time(b)) <= self.max_gap_ms

    def _ordered_pair(self, a: dict[str, Any], b: dict[str, Any]) -> tuple[dict[str, Any], dict[str, Any]]:
        return (a, b) if (_event_time(a), _event_id(a)) <= (_event_time(b), _event_id(b)) else (b, a)

    def _maybe_add_temporal_edge(self, a: dict[str, Any], b: dict[str, Any]) -> None:
        if _partition(a) != _partition(b) or not self._within_gap(a, b):
            return
        src, dst = self._ordered_pair(a, b)
        edge = GraphEdge(_event_id(src), _event_id(dst), "temporal_next", _event_time(dst), {"partition_key": _partition(src)})
        self._add_edge_only(edge)

    def _maybe_add_relation_edge(self, a: dict[str, Any], b: dict[str, Any], relation: str, evidence: dict[str, Any]) -> None:
        if not self._within_gap(a, b):
            return
        src, dst = self._ordered_pair(a, b)
        edge = GraphEdge(_event_id(src), _event_id(dst), relation, _event_time(dst), evidence)
        self._add_edge_only(edge)


class StandingTrendRuntime:
    """Incremental standing-query runtime over TemporalGraphState."""

    def __init__(self, registry: QueryRegistry, state: TemporalGraphState | None = None, options: StandingTrendOptions | None = None, sinks: Iterable[TrendSink] = ()): 
        self.registry = registry
        self.registry.validate_unique_versions()
        self.options = options or StandingTrendOptions()
        self.state = state or TemporalGraphState(max_gap_ms=self.options.default_max_gap_ms)
        self.sinks = list(sinks)
        self.active: dict[str, StandingTrendEvent] = {}

    def process_event(self, event: dict[str, Any]) -> list[StandingTrendEvent]:
        mutation = self.state.upsert_event(event)
        return self._reevaluate_partition(mutation.partition_key or _partition(event), mutation)

    def remove_event(self, event_id: int) -> list[StandingTrendEvent]:
        mutation = self.state.remove_event(event_id)
        if not mutation.partition_key:
            return []
        return self._reevaluate_partition(mutation.partition_key, mutation)

    def replay(self, events: Iterable[dict[str, Any]]) -> list[StandingTrendEvent]:
        emitted: list[StandingTrendEvent] = []
        for event in sorted(events, key=lambda e: (_event_time(e), _event_id(e))):
            emitted.extend(self.process_event(event))
        return emitted

    def _reevaluate_partition(self, partition_key: str, mutation: GraphMutation) -> list[StandingTrendEvent]:
        current: dict[str, StandingTrendEvent] = {}
        partition_events = self.state.events_for_partition(partition_key)
        if len(partition_events) > self.options.max_window_events_per_partition:
            partition_events = partition_events[-self.options.max_window_events_per_partition:]
        for query in self.registry.enabled():
            for trend in self._evaluate_query_partition(query, partition_key, partition_events, mutation.revision):
                current[trend.trend_id] = trend
        previous_ids = {tid for tid, ev in self.active.items() if ev.partition_key == partition_key}
        current_ids = set(current)
        output: list[StandingTrendEvent] = []
        for tid in sorted(current_ids - previous_ids):
            output.append(current[tid])
        if self.options.emit_cancellations:
            for tid in sorted(previous_ids - current_ids):
                prev = self.active[tid]
                output.append(StandingTrendEvent(
                    event_type="cancel_match",
                    trend_id=prev.trend_id,
                    query_id=prev.query_id,
                    query_version=prev.query_version,
                    partition_key=prev.partition_key,
                    path=prev.path,
                    revision=mutation.revision,
                    trend=prev.trend,
                    reason=f"match no longer valid after {mutation.op}",
                ))
        for tid in previous_ids:
            self.active.pop(tid, None)
        self.active.update(current)
        for event in output:
            for sink in self.sinks:
                sink.emit(event)
        return output

    def _evaluate_query_partition(self, query: QuerySpec, partition_key: str, events: list[dict[str, Any]], revision: int) -> list[StandingTrendEvent]:
        refs = query.referenced_event_types()
        candidates = [e for e in events if not refs or str(e.get("event_type")) in refs]
        paths = _match_query_paths(query, candidates, self.options.max_matches_per_query_partition)
        out: list[StandingTrendEvent] = []
        for path_events in paths:
            if not validate_path(query, path_events, candidates):
                continue
            path = tuple(_event_id(e) for e in path_events)
            features = extract_trend_features(path_events)
            native_stats = {"standing_runtime": True, "revision": revision, "candidate_events": len(candidates)}
            risk = score_trend(query, path_events, native_stats)
            edge_evidence = self.state.edges_for_path(path) if self.options.include_evidence_graph else []
            trend = {
                "trend_id": _fingerprint(query, partition_key, path),
                "query_id": query.query_id,
                "query_version": query.version,
                "partition_key": partition_key,
                "path": list(path),
                "trend_start_time_ms": features["trend_start_time_ms"],
                "trend_end_time_ms": features["trend_end_time_ms"],
                "event_count": features["event_count"],
                "risk_score": risk["risk_score"],
                "severity": risk["severity"],
                "features": features,
                "risk_explanation": risk["explanation"],
                "edge_evidence": edge_evidence,
                "native_stats": native_stats,
            }
            out.append(StandingTrendEvent(
                event_type="positive_match",
                trend_id=trend["trend_id"],
                query_id=query.query_id,
                query_version=query.version,
                partition_key=partition_key,
                path=path,
                revision=revision,
                trend=trend,
            ))
        return out


def _step_matches(step: Any, event: dict[str, Any]) -> bool:
    return str(event.get("event_type")) in step.aliases


def _match_query_paths(query: QuerySpec, events: list[dict[str, Any]], max_matches: int) -> list[list[dict[str, Any]]]:
    steps = query.steps
    events = sorted(events, key=lambda e: (_event_time(e), _event_id(e)))
    out: list[list[dict[str, Any]]] = []

    def rec(step_idx: int, start_pos: int, path: list[dict[str, Any]], start_time: int | None) -> None:
        if len(out) >= max_matches:
            return
        if step_idx >= len(steps):
            if path:
                out.append(list(path))
            return
        step = steps[step_idx]
        # Optional branch: skip the step.
        if step.optional:
            rec(step_idx + 1, start_pos, path, start_time)
        max_rep = step.max_repeats if step.max_repeats is not None else max(1, len(events) - start_pos)

        def consume_repeats(pos: int, reps_done: int, cur_path: list[dict[str, Any]], cur_start: int | None) -> None:
            if len(out) >= max_matches:
                return
            if reps_done >= step.min_repeats:
                rec(step_idx + 1, pos, cur_path, cur_start)
            if reps_done >= max_rep:
                return
            for i in range(pos, len(events)):
                ev = events[i]
                if not _step_matches(step, ev):
                    continue
                ev_time = _event_time(ev)
                effective_start = ev_time if cur_start is None else cur_start
                if ev_time < effective_start:
                    continue
                if ev_time - effective_start > int(query.within_ms):
                    continue
                # prevent duplicate event in path
                if any(_event_id(x) == _event_id(ev) for x in cur_path):
                    continue
                consume_repeats(i + 1, reps_done + 1, cur_path + [ev], effective_start)
        consume_repeats(start_pos, 0, path, start_time)

    rec(0, 0, [], None)
    return out


def trend_event_to_json(event: StandingTrendEvent) -> str:
    return json.dumps(event.as_dict(), sort_keys=True, default=str)
