# Databricks notebook source
"""0xDSI CET v4 Databricks runtime.

This notebook is designed for Databricks Asset Bundles. It keeps Spark work on
the driver and sends only serializable config into partition functions. The
native engine remains single-thread by default inside streaming tasks to avoid
Spark executor oversubscription.
"""
from __future__ import annotations

import json
from pyspark.sql import functions as F
from pyspark.sql.types import *

from bindings.python.query_registry import QueryRegistry
from bindings.python.security_graph import SecurityGraphBuilder
from bindings.python.multi_query_runtime import CETRuntimeV4, RuntimeOptions
from bindings.python.bridge import CETBridge

# Databricks widgets are optional; defaults make local import/py_compile safe.
def _param(name: str, default: str) -> str:
    try:
        dbutils.widgets.text(name, default)  # type: ignore[name-defined]
        return dbutils.widgets.get(name)  # type: ignore[name-defined]
    except Exception:
        return default

CATALOG = _param("catalog", "main")
SCHEMA = _param("schema", "cet")
SOURCE_TABLE = _param("source_table", "cet_events")
CHECKPOINT_BASE = _param("checkpoint_base", "dbfs:/checkpoints/0xdsi-cet")
QUERY_REGISTRY_TABLE = f"{CATALOG}.{SCHEMA}.cet_query_registry_v4"

spark.sql(f"""
CREATE TABLE IF NOT EXISTS {CATALOG}.{SCHEMA}.cet_complete_trends_v2 (
  trend_id STRING,
  query_id STRING,
  query_version STRING,
  batch_id BIGINT,
  partition_key STRING,
  path ARRAY<BIGINT>,
  trend_start_time_ms BIGINT,
  trend_end_time_ms BIGINT,
  event_count BIGINT,
  risk_score DOUBLE,
  severity STRING,
  features STRING,
  native_stats STRING,
  state STRING,
  created_at TIMESTAMP,
  updated_at TIMESTAMP
) USING DELTA
""")

spark.sql(f"""
CREATE TABLE IF NOT EXISTS {CATALOG}.{SCHEMA}.cet_metrics_v4 (
  batch_id BIGINT,
  query_id STRING,
  metric_name STRING,
  metric_value DOUBLE,
  dimensions STRING,
  created_at TIMESTAMP
) USING DELTA
""")

spark.sql(f"""
CREATE TABLE IF NOT EXISTS {CATALOG}.{SCHEMA}.cet_dead_letter_v4 (
  batch_id BIGINT,
  partition_key STRING,
  error STRING,
  payload STRING,
  created_at TIMESTAMP
) USING DELTA
""")


def load_registry_json() -> dict:
    try:
        rows = spark.table(QUERY_REGISTRY_TABLE).where("enabled = true").collect()
        return {"queries": [json.loads(r.query_json) for r in rows]}
    except Exception:
        with open("contracts/query_registry_v4.example.json") as fh:
            return json.load(fh)

REGISTRY_JSON = load_registry_json()
REGISTRY_BC = spark.sparkContext.broadcast(REGISTRY_JSON)


def process_partition(rows):
    bridge = CETBridge()
    registry = QueryRegistry.from_dict(REGISTRY_BC.value)
    runtime = CETRuntimeV4(
        registry=registry,
        bridge=bridge,
        graph_builder=SecurityGraphBuilder(max_gap_ms=24 * 60 * 60 * 1000),
        options=RuntimeOptions(native_threads=1, enable_mmap_arena=False),
    )
    bucket = {}
    for r in rows:
        payload = r.asDict(recursive=True)
        payload.setdefault("attributes", {})
        for k, v in payload.items():
            if k not in {"event_id", "partition_key", "event_type", "event_time_ms", "event_time", "attributes"}:
                payload["attributes"].setdefault(k, v)
        bucket.setdefault(str(payload["partition_key"]), []).append(payload)
    for pkey, events in bucket.items():
        try:
            for trend in runtime.run(events):
                yield ("ok", json.dumps(trend, sort_keys=True), pkey, "")
        except Exception as exc:
            yield ("err", "", pkey, str(exc)[:2000])


def process_batch(df, batch_id: int):
    required = {"event_id", "partition_key", "event_type", "event_time_ms", "event_time"}
    missing = required - set(df.columns)
    if missing:
        raise RuntimeError(f"missing required columns: {sorted(missing)}")
    rows = df.rdd.mapPartitions(process_partition)
    schema = StructType([
        StructField("status", StringType()),
        StructField("trend_json", StringType()),
        StructField("partition_key", StringType()),
        StructField("error", StringType()),
    ])
    out = spark.createDataFrame(rows, schema).cache()
    if out.rdd.isEmpty():
        return

    ok = out.where("status = 'ok'")
    if not ok.rdd.isEmpty():
        parsed_schema = StructType([
            StructField("trend_id", StringType()),
            StructField("query_id", StringType()),
            StructField("query_version", StringType()),
            StructField("partition_key", StringType()),
            StructField("path", ArrayType(LongType())),
            StructField("trend_start_time_ms", LongType()),
            StructField("trend_end_time_ms", LongType()),
            StructField("event_count", LongType()),
            StructField("risk_score", DoubleType()),
            StructField("severity", StringType()),
            StructField("features", MapType(StringType(), StringType()), True),
        ])
        parsed = ok.select(F.from_json("trend_json", parsed_schema).alias("t"), "trend_json").select("t.*", "trend_json")
        parsed = parsed.withColumn("batch_id", F.lit(batch_id)).withColumn("features", F.col("trend_json")).withColumn("native_stats", F.col("trend_json")).withColumn("state", F.lit("active")).withColumn("created_at", F.current_timestamp()).withColumn("updated_at", F.current_timestamp())
        parsed.createOrReplaceTempView("cet_v4_out")
        spark.sql(f"""
          MERGE INTO {CATALOG}.{SCHEMA}.cet_complete_trends_v2 t
          USING cet_v4_out s
          ON t.trend_id = s.trend_id
          WHEN MATCHED THEN UPDATE SET
            t.risk_score = s.risk_score,
            t.severity = s.severity,
            t.features = s.features,
            t.native_stats = s.native_stats,
            t.state = 'active',
            t.updated_at = current_timestamp()
          WHEN NOT MATCHED THEN INSERT *
        """)
        metrics = parsed.groupBy("query_id").agg(F.count("*").cast("double").alias("metric_value")).withColumn("batch_id", F.lit(batch_id)).withColumn("metric_name", F.lit("trends_emitted")).withColumn("dimensions", F.lit("{}"))
        metrics.select("batch_id", "query_id", "metric_name", "metric_value", "dimensions", F.current_timestamp().alias("created_at")).write.mode("append").saveAsTable(f"{CATALOG}.{SCHEMA}.cet_metrics_v4")

    err = out.where("status = 'err'")
    if not err.rdd.isEmpty():
        err.select(F.lit(batch_id).alias("batch_id"), "partition_key", "error", F.to_json(F.struct("*")).alias("payload"), F.current_timestamp().alias("created_at")).write.mode("append").saveAsTable(f"{CATALOG}.{SCHEMA}.cet_dead_letter_v4")


stream_df = spark.readStream.table(f"{CATALOG}.{SCHEMA}.{SOURCE_TABLE}").withWatermark("event_time", "30 minutes")
query = stream_df.writeStream.foreachBatch(process_batch).option("checkpointLocation", f"{CHECKPOINT_BASE}/streaming-v4").start()
