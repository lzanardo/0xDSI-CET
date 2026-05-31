"""Minimal OCSF-style event adapter for CET."""
from __future__ import annotations
from typing import Any


def normalize_ocsf_event(e: dict[str, Any]) -> dict[str, Any]:
    attrs = dict(e.get("attributes") or {})
    actor = e.get("actor") or {}
    device = e.get("device") or {}
    src = e.get("src_endpoint") or {}
    cloud = e.get("cloud") or {}
    attrs.setdefault("user_id", actor.get("user", {}).get("uid") or actor.get("user", {}).get("name"))
    attrs.setdefault("host_id", device.get("uid") or device.get("hostname"))
    attrs.setdefault("source_ip", src.get("ip"))
    attrs.setdefault("cloud_account_id", cloud.get("account_uid"))
    return {
        "event_id": int(e.get("event_id") or e.get("uid") or e.get("metadata", {}).get("uid")),
        "partition_key": str(attrs.get("user_id") or attrs.get("host_id") or "default"),
        "event_type": str(e.get("event_type") or e.get("class_name") or e.get("metadata", {}).get("event_code")),
        "event_time_ms": int(e.get("event_time_ms") or e.get("time") or e.get("metadata", {}).get("logged_time", 0)),
        "attributes": {k: v for k, v in attrs.items() if v is not None},
    }
