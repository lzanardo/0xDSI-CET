"""Agentic investigation package generation for CET trends."""
from __future__ import annotations

from typing import Any


def build_timeline(trend: dict[str, Any], events_by_id: dict[int, dict[str, Any]]) -> list[dict[str, Any]]:
    out = []
    for ordinal, eid in enumerate(trend.get("path", [])):
        event = events_by_id.get(int(eid), {"event_id": eid})
        out.append({
            "ordinal": ordinal,
            "event_id": int(eid),
            "event_type": event.get("event_type"),
            "event_time_ms": event.get("event_time_ms"),
            "partition_key": event.get("partition_key"),
            "attributes": event.get("attributes", {}),
        })
    return sorted(out, key=lambda x: (x.get("event_time_ms") or 0, x["ordinal"]))


def next_best_questions(trend: dict[str, Any]) -> list[str]:
    qid = trend.get("query_id", "this trend")
    return [
        f"Which entity is common across all events in {qid}?",
        "Was MFA, EDR, or a policy control expected between the first and last event?",
        "Did the user/host have prior similar trends in the last 7/30 days?",
        "Which high-value assets or sensitive objects were touched?",
        "Would changing the query window materially alter the detection?",
    ]


def false_positive_checks(trend: dict[str, Any]) -> list[str]:
    sev = str(trend.get("severity", "medium"))
    checks = [
        "Confirm all events refer to the same intended identity/session/host scope.",
        "Check for approved admin activity or maintenance window.",
        "Check duplicate or replayed telemetry event IDs.",
        "Validate event timestamps and source clock skew.",
    ]
    if sev in {"critical", "high"}:
        checks.append("Require explicit analyst disposition before suppression.")
    return checks


def build_investigation_package(
    trend: dict[str, Any],
    events_by_id: dict[int, dict[str, Any]],
    *,
    mitre_map: dict[str, list[str]] | None = None,
) -> dict[str, Any]:
    timeline = build_timeline(trend, events_by_id)
    query_id = str(trend.get("query_id", "unknown"))
    return {
        "trend_id": trend.get("trend_id"),
        "query_id": query_id,
        "severity": trend.get("severity"),
        "risk_score": trend.get("risk_score"),
        "timeline": timeline,
        "mitre_techniques": (mitre_map or {}).get(query_id, []),
        "why": trend.get("risk_explanation") or ["CET path matched the configured temporal/causal query."],
        "next_best_questions": next_best_questions(trend),
        "false_positive_checks": false_positive_checks(trend),
        "recommended_actions": [
            "Preserve raw evidence for the full path.",
            "Pivot on shared identity, host, session, and source IP.",
            "Run counterfactual replay before changing the rule.",
        ],
    }
