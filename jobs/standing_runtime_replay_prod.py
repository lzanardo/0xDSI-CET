"""Replay Delta events through the Standing Trend Runtime and write trend events.

Databricks job parameters:
  --catalog main --schema cet --source_table cet_events --target_table cet_standing_trend_events_v7
"""
from __future__ import annotations

import argparse
import json
from pyspark.sql import functions as F  # type: ignore
from pyspark.sql.types import *  # type: ignore


def parse_args() -> argparse.Namespace:
    p = argparse.ArgumentParser()
    p.add_argument("--catalog", default="main")
    p.add_argument("--schema", default="cet")
    p.add_argument("--source_table", default="cet_events")
    p.add_argument("--target_table", default="cet_standing_trend_events_v7")
    p.add_argument("--query_registry_table", default="cet_query_registry_v4")
    return p.parse_args()


args = parse_args()
CATALOG = args.catalog
SCHEMA = args.schema
SOURCE = f"{CATALOG}.{SCHEMA}.{args.source_table}"
TARGET = f"{CATALOG}.{SCHEMA}.{args.target_table}"
REGISTRY_TABLE = f"{CATALOG}.{SCHEMA}.{args.query_registry_table}"

spark.sql(f"""
CREATE TABLE IF NOT EXISTS {TARGET} (
  event_type STRING,
  trend_id STRING,
  query_id STRING,
  query_version STRING,
  partition_key STRING,
  path ARRAY<BIGINT>,
  revision BIGINT,
  trend_json STRING,
  reason STRING,
  created_at TIMESTAMP
) USING DELTA
""")

try:
    registry_rows = spark.table(REGISTRY_TABLE).where("enabled = true").collect()
    registry_json = {"queries": [json.loads(r.query_json) for r in registry_rows]}
except Exception:
    registry_json = json.load(open("contracts/query_registry_v4.example.json"))

bc_registry = spark.sparkContext.broadcast(registry_json)


def process_partition(rows):
    from bindings.python.query_registry import QueryRegistry
    from bindings.python.standing import StandingTrendRuntime, StandingTrendOptions, TemporalGraphState

    registry = QueryRegistry.from_dict(bc_registry.value)
    runtime = StandingTrendRuntime(
        registry=registry,
        state=TemporalGraphState(max_gap_ms=24 * 60 * 60 * 1000),
        options=StandingTrendOptions(max_window_events_per_partition=50000),
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

out = spark.createDataFrame(spark.table(SOURCE).rdd.mapPartitions(process_partition), schema)
out.withColumn("created_at", F.current_timestamp()).write.mode("append").saveAsTable(TARGET)
print("standing runtime replay completed")
