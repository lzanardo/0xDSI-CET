# Databricks notebook source
"""0xDSI CET main-ready Databricks runtime.

This notebook supersedes the v4 notebook for production deployment. It adds:
- Delta event buffer for cross-microbatch CET windows;
- query-registry broadcast only from the driver;
- richer operational metrics;
- durable checkpoint path;
- idempotent trend MERGE;
- DLQ with raw partition payloads.
"""
from __future__ import annotations

import json
import time
from pyspark.sql import functions as F
from pyspark.sql.types import *

from bindings.python.query_registry import QueryRegistry
from bindings.python.security_graph import SecurityGraphBuilder
from bindings.python.multi_query_runtime import CETRuntimeV4, RuntimeOptions
from bindings.python.bridge import CETBridge


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
BUFFER_RETENTION_DAYS = int(_param("buffer_retention_days", "7"))
QUERY_REGISTRY_TABLE = f"{CATALOG}.{SCHEMA}.cet_query_registry_v4"

spark.sql(f"CREATE SCHEMA IF NOT EXISTS {CATALOG}.{SCHEMA}")

spark.sql(f"""
CREATE TABLE IF NOT EXISTS {CATALOG}.{SCHEMA}.cet_event_buffer_v5 (
  event_id BIGINT,
  partition_key STRING,
  event_type STRING,
  event_time TIMESTAMP,
  event_time_ms BIGINT,
  payload STRING,
  batch_id BIGINT,
  ingested_at TIMESTAMP
) USING DELTA
""")

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
CREATE TABLE IF NOT EXISTS {CATALOG}.{SCHEMA}.cet_metrics_v5 (
  batch_id BIGINT,
  query_id STRING,
  metric_name STRING,
  metric_value DOUBLE,
  dimensions STRING,
  created_at TIMESTAMP
) USING DELTA
""")

spark.sql(f"""
CREATE TABLE IF NOT EXISTS {CATALOG}.{SCHEMA}.cet_dead_letter_v5 (
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
        if rows:
            return {"queries": [json.loads(r.query_json) for r in rows]}
    except Exception:
        pass
    with open("contracts/query_registry_v4.example.json") as fh:
        return json.load(fh)


REGISTRY_JSON = load_registry_json()
REGISTRY = QueryRegistry.from_dict(REGISTRY_JSON)
MAX_WITHIN_MS = max((q.within_ms for q in REGISTRY.enabled()), default=30 * 60 * 1000)
REGISTRY_BC = spark.sparkContext.broadcast(REGISTRY_JSON)


def _runtime_for_executor() -> CETRuntimeV4:
    return CETRuntimeV4(
        registry=QueryRegistry.from_dict(REGISTRY_BC.value),
        bridge=CETBridge(),
        graph_builder=SecurityGraphBuilder(max_gap_ms=max(MAX_WITHIN_MS, 24 * 60 * 60 * 1000)),
        options=RuntimeOptions(native_threads=1, enable_mmap_arena=False, raise_on_overflow=False),
    )


def process_partition(rows):
    runtime = _runtime_for_executor()
    bucket = {}
    raw_payload_count = 0
    for r in rows:
        payload = json.loads(r.payload) if isinstance(r.payload, str) else r.asDict(recursive=True)
        payload.setdefault("attributes", {})
        bucket.setdefault(str(payload["partition_key"]), []).append(payload)
        raw_payload_count += 1
    for pkey, events in bucket.items():
        try:
            for trend in runtime.run(events):
                yield ("ok", json.dumps(trend, sort_keys=True), pkey, "", raw_payload_count)
        except Exception as exc:
            sample = json.dumps(events[:50], sort_keys=True)[:50000]
            yield ("err", sample, pkey, str(exc)[:4000], raw_payload_count)


def write_metric_rows(rows):
    if not rows:
        return
    spark.createDataFrame(rows, ["batch_id", "query_id", "metric_name", "metric_value", "dimensions"]) \
        .withColumn("created_at", F.current_timestamp()) \
        .write.mode("append").saveAsTable(f"{CATALOG}.{SCHEMA}.cet_metrics_v5")


def process_batch(df, batch_id: int):
    t0 = time.perf_counter()
    required = {"event_id", "partition_key", "event_type", "event_time_ms", "event_time"}
    missing = required - set(df.columns)
    if missing:
        raise RuntimeError(f"missing required columns: {sorted(missing)}")

    incoming = df.select(
        F.col("event_id").cast("long"),
        F.col("partition_key").cast("string"),
        F.col("event_type").cast("string"),
        F.col("event_time"),
        F.col("event_time_ms").cast("long"),
        F.to_json(F.struct(*[F.col(c) for c in df.columns])).alias("payload"),
        F.lit(batch_id).cast("long").alias("batch_id"),
        F.current_timestamp().alias("ingested_at"),
    )
    incoming.write.mode("append").saveAsTable(f"{CATALOG}.{SCHEMA}.cet_event_buffer_v5")

    bounds = incoming.agg(F.min("event_time_ms").alias("min_ms"), F.max("event_time_ms").alias("max_ms")).first()
    if bounds is None or bounds.min_ms is None:
        return
    lower = int(bounds.min_ms) - int(MAX_WITHIN_MS)
    impacted = [r.partition_key for r in incoming.select("partition_key").distinct().collect()]
    if not impacted:
        return

    history = spark.table(f"{CATALOG}.{SCHEMA}.cet_event_buffer_v5") \
        .where(F.col("event_time_ms") >= F.lit(lower)) \
        .where(F.col("partition_key").isin(impacted)) \
        .select("payload", "partition_key")

    rows = history.rdd.mapPartitions(process_partition)
    out_schema = StructType([
        StructField("status", StringType()),
        StructField("trend_json", StringType()),
        StructField("partition_key", StringType()),
        StructField("error", StringType()),
        StructField("raw_payload_count", LongType()),
    ])
    out = spark.createDataFrame(rows, out_schema).cache()
    if out.rdd.isEmpty():
        write_metric_rows([(batch_id, "_all", "batch_duration_ms", (time.perf_counter() - t0) * 1000.0, "{}")])
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
        ])
        parsed = ok.select(F.from_json("trend_json", parsed_schema).alias("t"), "trend_json").select("t.*", "trend_json")
        parsed = parsed.withColumn("batch_id", F.lit(batch_id)) \
            .withColumn("features", F.col("trend_json")) \
            .withColumn("native_stats", F.col("trend_json")) \
            .withColumn("state", F.lit("active")) \
            .withColumn("created_at", F.current_timestamp()) \
            .withColumn("updated_at", F.current_timestamp())
        parsed.createOrReplaceTempView("cet_v5_out")
        spark.sql(f"""
          MERGE INTO {CATALOG}.{SCHEMA}.cet_complete_trends_v2 t
          USING cet_v5_out s
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
        metric_rows = [(batch_id, "_all", "trends_emitted", float(parsed.count()), "{}")]
        for r in parsed.groupBy("query_id").count().collect():
            metric_rows.append((batch_id, r.query_id, "trends_emitted", float(r["count"]), "{}"))
        write_metric_rows(metric_rows)

    err = out.where("status = 'err'")
    if not err.rdd.isEmpty():
        err.select(
            F.lit(batch_id).alias("batch_id"), "partition_key", "error",
            F.col("trend_json").alias("payload"), F.current_timestamp().alias("created_at")
        ).write.mode("append").saveAsTable(f"{CATALOG}.{SCHEMA}.cet_dead_letter_v5")

    spark.sql(f"DELETE FROM {CATALOG}.{SCHEMA}.cet_event_buffer_v5 WHERE event_time < current_timestamp() - INTERVAL {BUFFER_RETENTION_DAYS} DAYS")

    write_metric_rows([
        (batch_id, "_all", "events_in", float(incoming.count()), "{}"),
        (batch_id, "_all", "history_events_scanned", float(history.count()), "{}"),
        (batch_id, "_all", "batch_duration_ms", (time.perf_counter() - t0) * 1000.0, "{}"),
    ])


stream_df = spark.readStream.table(f"{CATALOG}.{SCHEMA}.{SOURCE_TABLE}").withWatermark("event_time", "30 minutes")
query = stream_df.writeStream.foreachBatch(process_batch).option("checkpointLocation", f"{CHECKPOINT_BASE}/streaming-main").start()
