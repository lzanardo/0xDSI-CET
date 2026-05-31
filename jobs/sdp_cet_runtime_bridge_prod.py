"""Bridge Spark Declarative Pipeline output into the CET runtime.

This job is intentionally small: SDP owns declarative ingestion and quality;
CET runtime owns native graph/trend execution. The bridge exposes the configured
SDP event buffer as the source table for the existing main-ready CET job.
"""
from __future__ import annotations

import argparse


def parse_args() -> argparse.Namespace:
    p = argparse.ArgumentParser()
    p.add_argument("--catalog", default="main")
    p.add_argument("--schema", default="cet")
    p.add_argument("--sdp-buffer-table", default="cet_sdp_event_buffer")
    p.add_argument("--runtime-source-view", default="cet_events_from_sdp")
    return p.parse_args()


args = parse_args()
spark.sql(f"CREATE OR REPLACE VIEW {args.catalog}.{args.schema}.{args.runtime_source_view} AS SELECT * FROM {args.catalog}.{args.schema}.{args.sdp_buffer_table}")  # type: ignore[name-defined]
print(f"created runtime source view {args.catalog}.{args.schema}.{args.runtime_source_view}")
