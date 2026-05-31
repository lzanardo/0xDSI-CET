# Databricks notebook/job source
"""
Recompute CET trends for impacted replay windows.

Input:
  - cet_impacted_windows_v2, written by late_event_replay_prod.py.

Output:
  - cet_recomputed_trends_v2, consumed by retract_and_upsert_v2.sql.
"""
from __future__ import annotations

import hashlib
import json
from typing import Any

from pyspark.sql import functions as F
from pyspark.sql.types import (
    ArrayType,
    DoubleType,
    IntegerType,
    LongType,
    StringType,
    StructField,
    StructType,
)

from bindings.python.bridge import CETBridge

CATALOG = spark.conf.get("oxdsi.cet.catalog", "main")
SCHEMA = spark.conf.get("oxdsi.cet.schema", "cet")
T_EVENTS = f"{CATALOG}.{SCHEMA}.cet_events"
T_QUERIES = f"{CATALOG}.{SCHEMA}.cet_queries"
T_IMPACTED = f"{CATALOG}.{SCHEMA}.cet_impacted_windows_v2"
T_RECOMPUTED = f"{CATALOG}.{SCHEMA}.cet_recomputed_trends_v2"


def widget_or_conf(name: str, default: str) -> str:
    try:
        value = dbutils.widgets.get(name)  # type: ignore[name-defined]
        return value if value else default
    except Exception:
        return spark.conf.get(f"oxdsi.cet.{name}", default)


REPLAY_ID = widget_or_conf("replayId", "")
if not REPLAY_ID:
    raise RuntimeError("Missing replayId. Run late_event_replay_prod.py first or set oxdsi.cet.replayId.")

spark.sql(f"""
  CREATE TABLE IF NOT EXISTS {T_RECOMPUTED} (
    trend_id STRING,
    query_id STRING,
    query_version STRING,
    batch_id BIGINT,
    partition_key STRING,
    path ARRAY<BIGINT>,
    trend_start_ms BIGINT,
    trend_end_ms BIGINT,
    event_count INT,
    score DOUBLE,
    severity STRING,
    status STRING,
    created_at TIMESTAMP,
    updated_at TIMESTAMP,
    retracted_at TIMESTAMP,
    replay_id STRING,
    engine_stats STRING
  ) USING DELTA
""")


def trend_id(query_id: str, query_version: str, partition_key: str, path: list[int]) -> str:
    raw = f"{query_id}:{query_version}:{partition_key}:{','.join(map(str, path))}"
    return hashlib.sha256(raw.encode("utf-8")).hexdigest()


def build_temporal_edges(events: list[tuple[int, str, str, int]]) -> list[tuple[int, int, int, int]]:
    if len(events) <= 1:
        return []
    window_start = int(events[0][3])
    window_end = int(events[-1][3])
    return [
        (int(events[i - 1][0]), int(events[i][0]), window_start, window_end)
        for i in range(1, len(events))
    ]


def process_partition(rows):
    bridge = CETBridge()
    bucket: dict[tuple[str, str, str], dict[str, Any]] = {}

    for r in rows:
        key = (str(r.query_id), str(r.query_version), str(r.partition_key))
        if key not in bucket:
            bucket[key] = {
                "query_id": str(r.query_id),
                "query_version": str(r.query_version),
                "partition_key": str(r.partition_key),
                "pattern_csv": str(r.pattern_csv),
                "within_ms": int(r.within_ms),
                "slide_ms": int(r.slide_ms),
                "switch_depth": int(r.switch_depth or 2),
                "severity": str(r.severity or "medium"),
                "score": float(r.score or 1.0),
                "events": [],
            }
        bucket[key]["events"].append(
            (int(r.event_id), str(r.partition_key), str(r.event_type), int(r.event_time_ms))
        )

    for cfg in bucket.values():
        try:
            events = sorted(cfg["events"], key=lambda x: (x[3], x[0]))
            deduped = []
            seen = set()
            for ev in events:
                if ev[0] not in seen:
                    deduped.append(ev)
                    seen.add(ev[0])
            events = deduped

            query = bridge.parse_query(
                cfg["query_id"],
                cfg["pattern_csv"],
                cfg["within_ms"],
                cfg["slide_ms"],
            )
            match = bridge.run_hcet(
                query,
                events,
                build_temporal_edges(events),
                switch_depth=cfg["switch_depth"],
            )
            event_time_by_id = {int(e[0]): int(e[3]) for e in events}
            stats_json = json.dumps(match.stats, sort_keys=True)

            for path in match.paths:
                times = [event_time_by_id.get(int(eid)) for eid in path]
                times = [t for t in times if t is not None]
                yield (
                    trend_id(cfg["query_id"], cfg["query_version"], cfg["partition_key"], [int(x) for x in path]),
                    cfg["query_id"],
                    cfg["query_version"],
                    -1,
                    cfg["partition_key"],
                    [int(x) for x in path],
                    min(times) if times else None,
                    max(times) if times else None,
                    len(path),
                    cfg["score"],
                    cfg["severity"],
                    "active",
                    REPLAY_ID,
                    stats_json,
                )
        except Exception as e:
            # Recompute errors should fail the job rather than silently leave replay incomplete.
            raise RuntimeError(
                f"recompute failed for query={cfg['query_id']} version={cfg['query_version']} "
                f"partition={cfg['partition_key']}: {e}"
            )


impacted = spark.table(T_IMPACTED).where(F.col("replay_id") == F.lit(REPLAY_ID)).cache()
if impacted.rdd.isEmpty():
    raise RuntimeError(f"No impacted windows found for replay_id={REPLAY_ID}")

queries = spark.table(T_QUERIES).where(F.col("enabled") == True)

events = (
    spark.table(T_EVENTS).alias("e")
    .join(impacted.alias("w"), F.col("e.partition_key") == F.col("w.partition_key"), "inner")
    .where(
        (F.col("e.event_time_ms") >= F.col("w.window_start_ms")) &
        (F.col("e.event_time_ms") <= F.col("w.window_end_ms"))
    )
    .join(
        queries.alias("q"),
        (F.col("w.query_id") == F.col("q.query_id")) &
        (F.col("w.query_version") == F.col("q.query_version")),
        "inner",
    )
    .select(
        F.col("w.query_id").alias("query_id"),
        F.col("w.query_version").alias("query_version"),
        F.col("e.partition_key").alias("partition_key"),
        F.col("e.event_id").cast("long").alias("event_id"),
        F.col("e.event_type").alias("event_type"),
        F.col("e.event_time_ms").cast("long").alias("event_time_ms"),
        F.col("q.pattern_csv").alias("pattern_csv"),
        F.col("q.within_ms").cast("long").alias("within_ms"),
        F.col("q.slide_ms").cast("long").alias("slide_ms"),
        F.col("q.switch_depth").cast("int").alias("switch_depth"),
        F.col("q.severity").alias("severity"),
        F.col("q.score").cast("double").alias("score"),
    )
    .dropDuplicates(["query_id", "query_version", "partition_key", "event_id"])
)

out_schema = StructType([
    StructField("trend_id", StringType(), False),
    StructField("query_id", StringType(), False),
    StructField("query_version", StringType(), False),
    StructField("batch_id", LongType(), False),
    StructField("partition_key", StringType(), False),
    StructField("path", ArrayType(LongType()), False),
    StructField("trend_start_ms", LongType(), True),
    StructField("trend_end_ms", LongType(), True),
    StructField("event_count", IntegerType(), False),
    StructField("score", DoubleType(), False),
    StructField("severity", StringType(), True),
    StructField("status", StringType(), False),
    StructField("replay_id", StringType(), False),
    StructField("engine_stats", StringType(), True),
])

out = spark.createDataFrame(
    events.repartition("query_id", "query_version", "partition_key").rdd.mapPartitions(process_partition),
    out_schema,
)

(
    out
    .withColumn("created_at", F.current_timestamp())
    .withColumn("updated_at", F.current_timestamp())
    .withColumn("retracted_at", F.lit(None).cast("timestamp"))
    .select(
        "trend_id",
        "query_id",
        "query_version",
        "batch_id",
        "partition_key",
        "path",
        "trend_start_ms",
        "trend_end_ms",
        "event_count",
        "score",
        "severity",
        "status",
        "created_at",
        "updated_at",
        "retracted_at",
        "replay_id",
        "engine_stats",
    )
    .write.mode("overwrite")
    .option("replaceWhere", f"replay_id = '{REPLAY_ID}'")
    .saveAsTable(T_RECOMPUTED)
)

print(f"Recompute complete: replay_id={REPLAY_ID}")
