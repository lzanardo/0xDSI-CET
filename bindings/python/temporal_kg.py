"""Temporal Security Knowledge Graph materialization helpers."""
from __future__ import annotations

from dataclasses import dataclass
from typing import Any, Iterable
import hashlib
import json

ENTITY_FIELDS = {
    "user": "user_id",
    "host": "host_id",
    "session": "session_id",
    "ip": "source_ip",
    "cloud_account": "cloud_account_id",
    "process": "process_id",
    "asset": "asset_id",
}


def _get(event: dict[str, Any], field: str) -> Any:
    if field in event:
        return event[field]
    attrs = event.get("attributes") or event.get("payload") or {}
    return attrs.get(field)


def entity_id(entity_type: str, value: Any) -> str:
    return hashlib.sha256(f"{entity_type}:{value}".encode()).hexdigest()


@dataclass(frozen=True)
class TemporalKG:
    entities: list[dict[str, Any]]
    relations: list[dict[str, Any]]
    trend_evidence: list[dict[str, Any]]


def build_temporal_kg(events: Iterable[dict[str, Any]], trends: Iterable[dict[str, Any]]) -> TemporalKG:
    entity_latest: dict[str, dict[str, Any]] = {}
    relations: list[dict[str, Any]] = []
    for e in events:
        ts = int(e.get("event_time_ms", 0))
        event_entities: list[tuple[str, str]] = []
        for typ, field in ENTITY_FIELDS.items():
            value = _get(e, field)
            if value in (None, ""):
                continue
            eid = entity_id(typ, value)
            prev = entity_latest.get(eid)
            row = {
                "entity_id": eid,
                "entity_type": typ,
                "entity_key": str(value),
                "first_seen_ms": min(ts, prev["first_seen_ms"]) if prev else ts,
                "last_seen_ms": max(ts, prev["last_seen_ms"]) if prev else ts,
                "attributes": json.dumps({"source_field": field}, sort_keys=True),
            }
            entity_latest[eid] = row
            event_entities.append((typ, eid))
        for i, (a_typ, a_id) in enumerate(event_entities):
            for b_typ, b_id in event_entities[i + 1:]:
                relations.append({
                    "src_entity_id": a_id,
                    "dst_entity_id": b_id,
                    "relation_type": f"co_observed:{a_typ}:{b_typ}",
                    "event_id": int(e["event_id"]),
                    "event_time_ms": ts,
                })
    evidence: list[dict[str, Any]] = []
    for t in trends:
        for ordinal, eid in enumerate(t.get("path", [])):
            evidence.append({
                "trend_id": t["trend_id"],
                "query_id": t.get("query_id"),
                "event_id": int(eid),
                "ordinal": int(ordinal),
                "risk_score": float(t.get("risk_score", 0.0)),
            })
    return TemporalKG(sorted(entity_latest.values(), key=lambda x: x["entity_id"]), relations, evidence)


def temporal_kg_ddls(catalog: str = "main", schema: str = "cet") -> list[str]:
    return [
        f"""
CREATE TABLE IF NOT EXISTS {catalog}.{schema}.cet_entities_v5 (
  entity_id STRING, entity_type STRING, entity_key STRING,
  first_seen_ms BIGINT, last_seen_ms BIGINT, attributes STRING, updated_at TIMESTAMP
) USING DELTA
""".strip(),
        f"""
CREATE TABLE IF NOT EXISTS {catalog}.{schema}.cet_entity_relations_v5 (
  src_entity_id STRING, dst_entity_id STRING, relation_type STRING,
  event_id BIGINT, event_time_ms BIGINT, created_at TIMESTAMP
) USING DELTA
""".strip(),
        f"""
CREATE TABLE IF NOT EXISTS {catalog}.{schema}.cet_trend_evidence_v5 (
  trend_id STRING, query_id STRING, event_id BIGINT, ordinal INT, risk_score DOUBLE, created_at TIMESTAMP
) USING DELTA
""".strip(),
    ]
