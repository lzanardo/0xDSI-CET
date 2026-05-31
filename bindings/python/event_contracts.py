"""Runtime validation for normalized security events."""
from __future__ import annotations

from typing import Any

REQUIRED_EVENT_FIELDS = ("event_id", "partition_key", "event_type", "event_time_ms")


def validate_event(event: dict[str, Any]) -> None:
    missing = [f for f in REQUIRED_EVENT_FIELDS if f not in event]
    if missing:
        raise ValueError(f"missing required event fields: {missing}")
    int(event["event_id"])
    int(event["event_time_ms"])
    if not str(event["event_type"]):
        raise ValueError("event_type must be non-empty")


def validate_events(events: list[dict[str, Any]]) -> None:
    seen: set[int] = set()
    for e in events:
        validate_event(e)
        eid = int(e["event_id"])
        if eid in seen:
            raise ValueError(f"duplicate event_id: {eid}")
        seen.add(eid)
