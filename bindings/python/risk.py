"""Deterministic risk scoring for CET trends."""
from __future__ import annotations

from typing import Any

SEVERITY_BASE = {
    "info": 10.0,
    "low": 25.0,
    "medium": 50.0,
    "high": 75.0,
    "critical": 90.0,
}


def clamp_score(x: float) -> float:
    return max(0.0, min(100.0, float(x)))


def score_trend(query: Any, path_events: list[dict[str, Any]], stats: dict[str, Any] | None = None) -> dict[str, Any]:
    stats = stats or {}
    base = SEVERITY_BASE.get(str(getattr(query, "severity", "medium")).lower(), 50.0)
    path_bonus = min(10.0, max(0, len(path_events) - 2) * 2.0)
    criticality = 0.0
    for event in path_events:
        attrs = event.get("attributes") or event
        try:
            criticality = max(criticality, float(attrs.get("asset_criticality", 0.0)))
        except (TypeError, ValueError):
            pass
    overflow_penalty = -10.0 if stats.get("overflow") else 0.0
    score_cfg = getattr(query, "score", {}) or {}
    manual_boost = float(score_cfg.get("boost", 0.0)) if isinstance(score_cfg, dict) else 0.0
    score = clamp_score(base + path_bonus + criticality + manual_boost + overflow_penalty)
    if score >= 90:
        severity = "critical"
    elif score >= 75:
        severity = "high"
    elif score >= 50:
        severity = "medium"
    elif score >= 25:
        severity = "low"
    else:
        severity = "info"
    return {
        "risk_score": score,
        "severity": severity,
        "explanation": {
            "base": base,
            "path_bonus": path_bonus,
            "asset_criticality_bonus": criticality,
            "manual_boost": manual_boost,
            "overflow_penalty": overflow_penalty,
        },
    }
