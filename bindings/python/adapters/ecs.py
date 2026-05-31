"""Minimal Elastic Common Schema adapter for CET."""
from __future__ import annotations
from typing import Any
from datetime import datetime


def _parse_ts(v: Any) -> int:
    if v is None:
        return 0
    if isinstance(v, (int, float)):
        return int(v)
    s = str(v).replace("Z", "+00:00")
    try:
        return int(datetime.fromisoformat(s).timestamp() * 1000)
    except Exception:
        return 0


def normalize_ecs_event(e: dict[str, Any]) -> dict[str, Any]:
    user = e.get("user") or {}
    host = e.get("host") or {}
    src = e.get("source") or {}
    event = e.get("event") or {}
    attrs = dict(e)
    attrs.update({
        "user_id": user.get("id") or user.get("name"),
        "host_id": host.get("id") or host.get("name"),
        "source_ip": src.get("ip"),
        "session_id": (e.get("session") or {}).get("id"),
    })
    return {
        "event_id": int(event.get("id") or e.get("event_id") or abs(hash(str(e))) % (2**31 - 1)),
        "partition_key": str(attrs.get("user_id") or attrs.get("host_id") or "default"),
        "event_type": str(event.get("action") or event.get("category") or e.get("event_type") or "Unknown"),
        "event_time_ms": _parse_ts(e.get("@timestamp") or e.get("event_time_ms")),
        "attributes": {k: v for k, v in attrs.items() if v is not None},
    }
