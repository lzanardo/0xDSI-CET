"""Feature extraction for CET trends."""
from __future__ import annotations

from typing import Any


def extract_entities(path_events: list[dict[str, Any]]) -> dict[str, list[str]]:
    buckets: dict[str, set[str]] = {
        "users": set(),
        "hosts": set(),
        "sessions": set(),
        "ips": set(),
        "assets": set(),
        "cloud_accounts": set(),
        "processes": set(),
    }
    mapping = {
        "user_id": "users",
        "host_id": "hosts",
        "session_id": "sessions",
        "source_ip": "ips",
        "asset_id": "assets",
        "cloud_account_id": "cloud_accounts",
        "process_id": "processes",
    }
    for event in path_events:
        attrs = event.get("attributes") or event
        for field, bucket in mapping.items():
            value = attrs.get(field)
            if value is not None and value != "":
                buckets[bucket].add(str(value))
    return {k: sorted(v) for k, v in buckets.items() if v}


def extract_trend_features(path_events: list[dict[str, Any]]) -> dict[str, Any]:
    if not path_events:
        return {"event_count": 0}
    times = [int(e.get("event_time_ms", 0)) for e in path_events]
    types = [str(e.get("event_type")) for e in path_events]
    return {
        "event_count": len(path_events),
        "trend_start_time_ms": min(times),
        "trend_end_time_ms": max(times),
        "duration_ms": max(times) - min(times),
        "event_types": types,
        "distinct_event_types": sorted(set(types)),
        "entities": extract_entities(path_events),
    }
