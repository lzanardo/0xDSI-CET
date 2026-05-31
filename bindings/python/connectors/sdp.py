"""Helpers for Spark Declarative Pipelines integration.

This module contains only dependency-light helpers. The actual SDP/Lakeflow
pipeline is in `notebooks/0xDSI_CET_SDP.py` because Databricks executes pipeline
source files rather than importing them as ordinary Python packages.
"""
from __future__ import annotations

from dataclasses import dataclass
from typing import Any
import json


@dataclass(frozen=True)
class SDPDatasetNames:
    bronze_events: str = "cet_sdp_bronze_events"
    silver_events: str = "cet_sdp_silver_events"
    event_buffer: str = "cet_sdp_event_buffer"
    event_quality_metrics: str = "cet_sdp_event_quality_metrics"
    query_registry: str = "cet_sdp_query_registry"

    def as_dict(self) -> dict[str, str]:
        return {
            "bronze_events": self.bronze_events,
            "silver_events": self.silver_events,
            "event_buffer": self.event_buffer,
            "event_quality_metrics": self.event_quality_metrics,
            "query_registry": self.query_registry,
        }


def sdp_contract_document(source_table: str, names: SDPDatasetNames | None = None) -> dict[str, Any]:
    n = names or SDPDatasetNames()
    return {
        "source_table": source_table,
        "datasets": n.as_dict(),
        "required_columns": ["event_id", "partition_key", "event_type", "event_time_ms", "event_time"],
    }


def sdp_contract_json(source_table: str, names: SDPDatasetNames | None = None) -> str:
    return json.dumps(sdp_contract_document(source_table, names), sort_keys=True, indent=2)
