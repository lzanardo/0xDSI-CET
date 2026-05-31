"""Base contracts for external event connectors."""
from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any, Iterable
import json
import time


@dataclass(frozen=True)
class ConnectorConfig:
    """Common connector configuration.

    `source_name` is used for lineage. `target_table` is the Delta/Spark table
    that receives normalized events.
    """

    source_name: str
    target_table: str = "main.cet.cet_events"
    batch_size: int = 1000
    max_messages: int | None = None
    poll_interval_seconds: float = 1.0
    attributes: dict[str, Any] = field(default_factory=dict)


@dataclass(frozen=True)
class ConnectorEvent:
    event_id: int | str
    partition_key: str
    event_type: str
    event_time_ms: int
    event_time: str | None = None
    attributes: dict[str, Any] = field(default_factory=dict)
    source_name: str = "unknown"
    raw: dict[str, Any] = field(default_factory=dict)

    def as_dict(self) -> dict[str, Any]:
        out = {
            "event_id": self.event_id,
            "partition_key": self.partition_key,
            "event_type": self.event_type,
            "event_time_ms": self.event_time_ms,
            "event_time": self.event_time,
            "attributes": dict(self.attributes),
            "source_name": self.source_name,
        }
        out["raw_json"] = json.dumps(self.raw or out, sort_keys=True, default=str)
        return out


def _first(d: dict[str, Any], *names: str, default: Any = None) -> Any:
    for name in names:
        if name in d and d[name] not in (None, ""):
            return d[name]
    attrs = d.get("attributes") or d.get("payload") or {}
    if isinstance(attrs, dict):
        for name in names:
            if name in attrs and attrs[name] not in (None, ""):
                return attrs[name]
    return default


def normalize_connector_event(raw: dict[str, Any], source_name: str = "unknown") -> ConnectorEvent:
    """Normalize arbitrary connector payload into the CET canonical event shape.

    The function is deliberately permissive so ZeroBus/SDP payloads can evolve
    while still landing in a stable Delta/Spark table contract.
    """

    now_ms = int(time.time() * 1000)
    attrs = dict(raw.get("attributes") or raw.get("payload") or {})
    for k, v in raw.items():
        if k not in {"event_id", "id", "partition_key", "event_type", "type", "event_time_ms", "event_time", "timestamp", "attributes", "payload"}:
            attrs.setdefault(k, v)
    event_id = _first(raw, "event_id", "id", "message_id", default=abs(hash(json.dumps(raw, sort_keys=True, default=str))) % (2**63))
    partition_key = str(_first(raw, "partition_key", "tenant_id", "user_id", "host_id", "source_ip", default="default"))
    event_type = str(_first(raw, "event_type", "type", "eventName", "activity_name", default="Unknown"))
    event_time_ms = int(_first(raw, "event_time_ms", "timestamp_ms", default=now_ms))
    event_time = _first(raw, "event_time", "timestamp", default=None)
    return ConnectorEvent(
        event_id=event_id,
        partition_key=partition_key,
        event_type=event_type,
        event_time_ms=event_time_ms,
        event_time=event_time,
        attributes=attrs,
        source_name=source_name,
        raw=raw,
    )


def normalize_many(events: Iterable[dict[str, Any]], source_name: str) -> list[dict[str, Any]]:
    return [normalize_connector_event(e, source_name=source_name).as_dict() for e in events]
