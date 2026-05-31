"""Standing Trend Runtime v7.

This package adds a Quine-class, user-space streaming graph layer on top of the
0xDSI CET kernel. It keeps mutable graph state, evaluates standing CET queries,
and emits positive/cancel trend events while still preserving the Databricks /
Delta replay model from v5/v6.
"""
from .runtime import (
    TemporalGraphState,
    StandingTrendRuntime,
    StandingTrendOptions,
    StandingTrendEvent,
    GraphEdge,
    GraphMutation,
)
from .sinks import MemoryTrendSink, JsonlTrendSink, ZeroBusTrendSink

__all__ = [
    "TemporalGraphState",
    "StandingTrendRuntime",
    "StandingTrendOptions",
    "StandingTrendEvent",
    "GraphEdge",
    "GraphMutation",
    "MemoryTrendSink",
    "JsonlTrendSink",
    "ZeroBusTrendSink",
]
