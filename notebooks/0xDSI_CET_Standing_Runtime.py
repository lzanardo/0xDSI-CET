# Databricks notebook source
"""Streaming Standing Trend Runtime v7.

This notebook complements the v5/v6 Databricks/SDP pipelines. It consumes the
canonical CET event buffer, maintains per-microbatch standing graph state within
partition tasks, and emits positive/cancel trend events. For strict long-lived
state across jobs, use `jobs/standing_runtime_replay_prod.py` plus Delta state
snapshotting or shard this runtime by partition/tenant.
"""
from __future__ import annotations

import json
from pyspark.sql import functions as F
from pyspark.sql.types import *


def _param(name: str, default: str) -> str:
    try:
        dbutils.widgets.text(name, default)  # type: ignore[name-defined]
        return dbutils.widgets.get(name)  # type: ignore[name-defined]
    except Exception:
        return default

CATALOG = _param("catalog", "main")
SCHEMA = _param("schema", "cet")
SOURCE_TABLE = _param("source_table", "cet_sdp_event_buffer")
CHECKPOINT_BASE = _param("checkpoint_base", "dbfs:/checkpoints/0xdsi-cet")
REGISTRY_TABLE = f"{CATALOG}.{SCHEMA}.cet_query_registry_v4"
TARGET_TABLE = f"{CATALOG}.{SCHEMA}.cet_standing_trend_events_v7"

spark.sql(f"""
CREATE TABLE IF NOT EXISTS {TARGET_TABLE} (
  event_type STRING,
  trend_id STRING,
  query_id STRING,
  query_version STRING,
  batch_id BIGINT,
  partition_key STRING,
  path ARRAY<BIGINT>,
  revision BIGINT,
  trend_json STRING,
  reason STRING,
  created_at TIMESTAMP
) USING DELTA
""")


def load_registry_json() -> dict:
    try:
        rows = spark.table(REGISTRY_TABLE).where("enabled = true").collect()
        return {"queries": [json.loads(r.query_json) for r in rows]}
    except Exception:
        with open("contracts/query_registry_v4.example.json") as fh:
            return json.load(fh)

REGISTRY_BC = spark.sparkContext.broadcast(load_registry_json())


def process_partition(rows):
    from bindings.python.query_registry import QueryRegistry
    from bindings.python.standing import StandingTrendRuntime, StandingTrendOptions, TemporalGraphState

    registry = QueryRegistry.from_dict(REGISTRY_BC.value)
    runtime = StandingTrendRuntime(
        registry=registry,
        state=TemporalGraphState(max_gap_ms=24 * 60 * 60 * 1000),
        options=StandingTrendOptions(max_window_events_per_partition=50000, emit_cancellations=True),
    )
    events = []
    for row in rows:
        payload = row.asDict(recursive=True)
        payload.setdefault("attributes", {})
        events.append(payload)
    for ev in sorted(events, key=lambda e: (str(e.get("partition_key", "default")), int(e.get("event_time_ms", 0)), int(e.get("event_id", 0)))):
        for trend_event in runtime.process_event(ev):
            d = trend_event.as_dict()
            yield (
                d["event_type"], d["trend_id"], d["query_id"], d["query_version"], d["partition_key"],
                [int(x) for x in d["path"]], int(d["revision"]), json.dumps(d["trend"], sort_keys=True, default=str), d.get("reason", "")
            )


def process_batch(df, batch_id: int):
    schema = StructType([
        StructField("event_type", StringType()),
        StructField("trend_id", StringType()),
        StructField("query_id", StringType()),
        StructField("query_version", StringType()),
        StructField("partition_key", StringType()),
        StructField("path", ArrayType(LongType())),
        StructField("revision", LongType()),
        StructField("trend_json", StringType()),
        StructField("reason", StringType()),
    ])
    out = spark.createDataFrame(df.rdd.mapPartitions(process_partition), schema)
    if out.rdd.isEmpty():
        return
    out.withColumn("batch_id", F.lit(batch_id)).withColumn("created_at", F.current_timestamp()).write.mode("append").saveAsTable(TARGET_TABLE)

stream_df = spark.readStream.table(f"{CATALOG}.{SCHEMA}.{SOURCE_TABLE}").withWatermark("event_time", "30 minutes")
query = stream_df.writeStream.foreachBatch(process_batch).option("checkpointLocation", f"{CHECKPOINT_BASE}/standing-runtime-v7").start()
