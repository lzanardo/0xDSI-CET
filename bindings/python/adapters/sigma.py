"""Small Sigma-like rule adapter for attack-chain-oriented CET queries.

This is not a complete Sigma implementation. It provides a deterministic bridge
for simple internal rules that define `sequence`, `condition_fields`, and
`timeframe_ms`.
"""
from __future__ import annotations
from typing import Any


def sigma_like_to_query(rule: dict[str, Any]) -> dict[str, Any]:
    sequence = rule.get("sequence") or rule.get("detection", {}).get("sequence")
    if not sequence:
        raise ValueError("sigma-like rule requires a sequence list")
    where = []
    for field, value in (rule.get("condition_fields") or {}).items():
        where.append({"field": field, "op": "eq", "value": value})
    return {
        "query_id": str(rule.get("id") or rule.get("title") or "sigma_like_rule"),
        "version": str(rule.get("version", "v1")),
        "name": str(rule.get("title") or rule.get("id") or "Sigma-like CET rule"),
        "pattern": ",".join(str(x) for x in sequence),
        "within_ms": int(rule.get("timeframe_ms") or rule.get("within_ms") or 30 * 60 * 1000),
        "slide_ms": int(rule.get("slide_ms") or 5 * 60 * 1000),
        "severity": str(rule.get("level") or rule.get("severity") or "medium"),
        "where": where,
        "tags": list(rule.get("tags", [])) + ["sigma_like"],
        "enabled": True,
    }
