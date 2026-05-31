"""State models and offline helpers for CET v4."""
from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any, Iterable
import hashlib
import json


@dataclass(frozen=True)
class PartialTrendState:
    query_id: str
    query_version: str
    partition_key: str
    prefix_path: tuple[int, ...]
    start_time_ms: int
    last_event_time_ms: int
    expiry_time_ms: int
    payload: dict[str, Any] = field(default_factory=dict)

    @property
    def state_id(self) -> str:
        raw = f"{self.query_id}:{self.query_version}:{self.partition_key}:{','.join(map(str, self.prefix_path))}"
        return hashlib.sha256(raw.encode("utf-8")).hexdigest()

    def to_json(self) -> str:
        return json.dumps({
            "state_id": self.state_id,
            "query_id": self.query_id,
            "query_version": self.query_version,
            "partition_key": self.partition_key,
            "prefix_path": list(self.prefix_path),
            "start_time_ms": self.start_time_ms,
            "last_event_time_ms": self.last_event_time_ms,
            "expiry_time_ms": self.expiry_time_ms,
            "payload": self.payload,
        }, sort_keys=True)


def expire_states(states: Iterable[PartialTrendState], watermark_ms: int) -> list[PartialTrendState]:
    return [s for s in states if s.expiry_time_ms >= watermark_ms]


def compact_states(states: Iterable[PartialTrendState]) -> list[PartialTrendState]:
    latest: dict[str, PartialTrendState] = {}
    for s in states:
        prev = latest.get(s.state_id)
        if prev is None or s.last_event_time_ms >= prev.last_event_time_ms:
            latest[s.state_id] = s
    return sorted(latest.values(), key=lambda s: (s.partition_key, s.query_id, s.state_id))


def delta_state_table_ddl(catalog: str = "main", schema: str = "cet") -> str:
    return f"""
CREATE TABLE IF NOT EXISTS {catalog}.{schema}.cet_partial_state_v4 (
  state_id STRING,
  query_id STRING,
  query_version STRING,
  partition_key STRING,
  prefix_path ARRAY<BIGINT>,
  start_time_ms BIGINT,
  last_event_time_ms BIGINT,
  expiry_time_ms BIGINT,
  payload STRING,
  updated_at TIMESTAMP
) USING DELTA
""".strip()
