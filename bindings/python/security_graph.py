"""Security event normalization and graph construction for CET v4.

The native C engine consumes event vertices and integer edges. This builder turns
security telemetry into a richer causal graph before native matching:

- temporal sequence edges per partition
- same_user / same_host / same_session / same_ip / same_cloud_account edges
- process parent-child edges when process_id and parent_process_id are present
- deterministic edge metadata for audit and future relation-aware execution
"""
from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any, Iterable
from collections import defaultdict


ENTITY_KEYS = (
    "user_id",
    "host_id",
    "session_id",
    "source_ip",
    "cloud_account_id",
    "process_id",
    "asset_id",
)

RELATION_BY_KEY = {
    "user_id": "same_user",
    "host_id": "same_host",
    "session_id": "same_session",
    "source_ip": "same_source_ip",
    "cloud_account_id": "same_cloud_account",
    "process_id": "same_process",
    "asset_id": "same_asset",
}


def _nested_get(d: dict[str, Any], key: str) -> Any:
    if key in d:
        return d[key]
    attrs = d.get("attributes") or d.get("payload") or {}
    if key in attrs:
        return attrs[key]
    cur: Any = attrs
    for part in key.split("."):
        if isinstance(cur, dict) and part in cur:
            cur = cur[part]
        else:
            return None
    return cur


@dataclass(frozen=True)
class SecurityEvent:
    event_id: int
    partition_key: str
    event_type: str
    event_time_ms: int
    attributes: dict[str, Any] = field(default_factory=dict)

    @classmethod
    def from_dict(cls, raw: dict[str, Any]) -> "SecurityEvent":
        attrs = dict(raw.get("attributes") or raw.get("payload") or {})
        for k, v in raw.items():
            if k not in {"event_id", "partition_key", "event_type", "event_time_ms", "event_time", "attributes", "payload"}:
                attrs.setdefault(k, v)
        return cls(
            event_id=int(raw["event_id"]),
            partition_key=str(raw.get("partition_key") or attrs.get("partition_key") or "default"),
            event_type=str(raw["event_type"]),
            event_time_ms=int(raw["event_time_ms"]),
            attributes=attrs,
        )

    def as_native_vertex(self) -> tuple[int, str, str, int]:
        return (self.event_id, self.partition_key, self.event_type, self.event_time_ms)

    def as_dict(self) -> dict[str, Any]:
        d = {
            "event_id": self.event_id,
            "partition_key": self.partition_key,
            "event_type": self.event_type,
            "event_time_ms": self.event_time_ms,
            "attributes": dict(self.attributes),
        }
        d.update(self.attributes)
        return d


@dataclass(frozen=True)
class EdgeMetadata:
    src: int
    dst: int
    relation: str
    window_start_ms: int
    window_end_ms: int
    evidence: dict[str, Any] = field(default_factory=dict)


@dataclass
class SecurityGraph:
    events: list[SecurityEvent]
    native_events: list[tuple[int, str, str, int]]
    native_edges: list[tuple[int, int, int, int]]
    edge_metadata: list[EdgeMetadata]

    def events_by_id(self) -> dict[int, dict[str, Any]]:
        return {e.event_id: e.as_dict() for e in self.events}


class SecurityGraphBuilder:
    def __init__(self, entity_keys: Iterable[str] = ENTITY_KEYS, max_gap_ms: int | None = None):
        self.entity_keys = tuple(entity_keys)
        self.max_gap_ms = max_gap_ms

    @staticmethod
    def _edge_window(a: SecurityEvent, b: SecurityEvent) -> tuple[int, int]:
        return (min(a.event_time_ms, b.event_time_ms), max(a.event_time_ms, b.event_time_ms))

    def _add_edge(
        self,
        seen: set[tuple[int, int, str]],
        meta: list[EdgeMetadata],
        a: SecurityEvent,
        b: SecurityEvent,
        relation: str,
        evidence: dict[str, Any] | None = None,
    ) -> None:
        if a.event_id == b.event_id:
            return
        if self.max_gap_ms is not None and abs(b.event_time_ms - a.event_time_ms) > self.max_gap_ms:
            return
        src, dst = (a, b) if a.event_time_ms <= b.event_time_ms else (b, a)
        key = (src.event_id, dst.event_id, relation)
        if key in seen:
            return
        seen.add(key)
        ws, we = self._edge_window(src, dst)
        meta.append(EdgeMetadata(src.event_id, dst.event_id, relation, ws, we, evidence or {}))

    def build(self, raw_events: Iterable[dict[str, Any] | SecurityEvent]) -> SecurityGraph:
        events = [e if isinstance(e, SecurityEvent) else SecurityEvent.from_dict(e) for e in raw_events]
        events = sorted(events, key=lambda e: (e.partition_key, e.event_time_ms, e.event_id))
        by_partition: dict[str, list[SecurityEvent]] = defaultdict(list)
        for e in events:
            by_partition[e.partition_key].append(e)

        seen: set[tuple[int, int, str]] = set()
        meta: list[EdgeMetadata] = []

        # Always keep temporal adjacency per partition. This preserves original
        # sequence semantics and lets the native engine run even when no entity
        # attributes are available.
        for partition_events in by_partition.values():
            for prev, curr in zip(partition_events, partition_events[1:]):
                self._add_edge(seen, meta, prev, curr, "temporal_next", {"partition_key": prev.partition_key})

        # Entity relationship edges.
        for key in self.entity_keys:
            grouped: dict[Any, list[SecurityEvent]] = defaultdict(list)
            for e in events:
                value = _nested_get(e.as_dict(), key)
                if value is not None and value != "":
                    grouped[value].append(e)
            relation = RELATION_BY_KEY.get(key, f"same_{key}")
            for value, group in grouped.items():
                group = sorted(group, key=lambda e: (e.event_time_ms, e.event_id))
                for prev, curr in zip(group, group[1:]):
                    self._add_edge(seen, meta, prev, curr, relation, {"field": key, "value": value})

        # Process parent-child causal edges.
        by_process: dict[Any, list[SecurityEvent]] = defaultdict(list)
        for e in events:
            pid = _nested_get(e.as_dict(), "process_id")
            if pid is not None:
                by_process[pid].append(e)
        for child in events:
            parent_pid = _nested_get(child.as_dict(), "parent_process_id")
            if parent_pid is None:
                continue
            candidates = [e for e in by_process.get(parent_pid, []) if e.event_time_ms <= child.event_time_ms]
            if not candidates:
                continue
            parent = max(candidates, key=lambda e: (e.event_time_ms, e.event_id))
            self._add_edge(seen, meta, parent, child, "parent_process", {"parent_process_id": parent_pid})

        native_edges = [(m.src, m.dst, m.window_start_ms, m.window_end_ms) for m in meta]
        return SecurityGraph(
            events=events,
            native_events=[e.as_native_vertex() for e in events],
            native_edges=native_edges,
            edge_metadata=meta,
        )
