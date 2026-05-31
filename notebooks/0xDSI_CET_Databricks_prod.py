# Databricks notebook source
"""
0xDSI CET production runtime v3.

Major improvements over the original notebook:
- durable, configurable checkpoint path,
- driver-side query/config loading + executor broadcast,
- no SparkSession calls inside mapPartitions,
- event-buffer state so trends can cross microbatch boundaries,
- richer trend output schema,
- idempotent metrics/DLQ writes,
- engine overflow/truncation metrics.
"""
from __future__ import annotations

import hashlib
import json
import time
from typing import Any

from pyspark.sql import functions as F
from pyspark.sql.types import (
    ArrayType,
    BooleanType,
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
STREAM_ID = spark.conf.get("oxdsi.cet.streamId", "0xdsi-cet-prod-v3")
CHECKPOINT_LOCATION = spark.conf.get(
    "oxdsi.cet.checkpointLocation",
    "dbfs:/checkpoints/0xdsi/cet/prod-v3",
)
ALLOWED_LATENESS_MS = int(spark.conf.get("oxdsi.cet.allowedLatenessMs", str(30 * 60 * 1000)))

# Native runtime defaults are conservative for Spark: one native thread per task
# to avoid executor oversubscription. Raise for offline recompute/backfill only.
NATIVE_RUNTIME = {
    "native_threads": int(spark.conf.get("oxdsi.cet.nativeThreads", "1")),
    "enable_mmap_arena": spark.conf.get("oxdsi.cet.enableMmapArena", "false").lower() in {"1", "true", "yes", "on"},
    "mmap_workspace_bytes": int(spark.conf.get("oxdsi.cet.mmapWorkspaceBytes", spark.conf.get("oxdsi.cet.mmapArenaBytes", "0"))),
    "madvise_hugepage": spark.conf.get("oxdsi.cet.madviseHugepage", "false").lower() in {"1", "true", "yes", "on"},
}

T_EVENTS = f"{CATALOG}.{SCHEMA}.cet_events"
T_QUERIES = f"{CATALOG}.{SCHEMA}.cet_queries"
T_TRENDS = f"{CATALOG}.{SCHEMA}.cet_complete_trends_v2"
T_METRICS = f"{CATALOG}.{SCHEMA}.cet_metrics_v2"
T_DLQ = f"{CATALOG}.{SCHEMA}.cet_dead_letter_v2"
T_BUFFER = f"{CATALOG}.{SCHEMA}.cet_event_buffer"
T_COEFFICIENTS = f"{CATALOG}.{SCHEMA}.cet_optimizer_coefficients"

REQUIRED_COLS = {"event_id", "partition_key", "event_type", "event_time_ms", "event_time"}


def ensure_tables() -> None:
    spark.sql(f"CREATE SCHEMA IF NOT EXISTS {CATALOG}.{SCHEMA}")

    spark.sql(f"""
      CREATE TABLE IF NOT EXISTS {T_QUERIES} (
        query_id STRING,
        query_version STRING,
        pattern_csv STRING,
        within_ms BIGINT,
        slide_ms BIGINT,
        switch_depth INT,
        severity STRING,
        score DOUBLE,
        enabled BOOLEAN,
        owner STRING,
        created_at TIMESTAMP,
        updated_at TIMESTAMP
      ) USING DELTA
    """)

    spark.sql(f"""
      CREATE TABLE IF NOT EXISTS {T_TRENDS} (
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

    spark.sql(f"""
      CREATE TABLE IF NOT EXISTS {T_METRICS} (
        stream_id STRING,
        batch_id BIGINT,
        query_id STRING,
        query_version STRING,
        partition_key STRING,
        metric_name STRING,
        metric_value DOUBLE,
        created_at TIMESTAMP
      ) USING DELTA
    """)

    spark.sql(f"""
      CREATE TABLE IF NOT EXISTS {T_DLQ} (
        dlq_id STRING,
        stream_id STRING,
        batch_id BIGINT,
        query_id STRING,
        query_version STRING,
        partition_key STRING,
        error STRING,
        payload STRING,
        created_at TIMESTAMP
      ) USING DELTA
    """)

    spark.sql(f"""
      CREATE TABLE IF NOT EXISTS {T_BUFFER} (
        query_id STRING,
        query_version STRING,
        partition_key STRING,
        event_id BIGINT,
        event_type STRING,
        event_time_ms BIGINT,
        event_time TIMESTAMP,
        payload_json STRING,
        expires_at TIMESTAMP,
        updated_at TIMESTAMP
      ) USING DELTA
    """)

    # Seed the default 0xDSI security query only if it is absent.
    spark.sql(f"""
      INSERT INTO {T_QUERIES}
      SELECT
        'security_escalation',
        'v1',
        'AuthFail+,PrivEsc,DataAccess',
        CAST(1800000 AS BIGINT),
        CAST(300000 AS BIGINT),
        CAST(2 AS INT),
        'high',
        CAST(1.0 AS DOUBLE),
        true,
        '0xDSI',
        current_timestamp(),
        current_timestamp()
      WHERE NOT EXISTS (
        SELECT 1 FROM {T_QUERIES}
        WHERE query_id = 'security_escalation' AND query_version = 'v1'
      )
    """)


def load_active_queries() -> list[dict[str, Any]]:
    rows = (
        spark.table(T_QUERIES)
        .where(F.col("enabled") == True)
        .select(
            "query_id",
            "query_version",
            "pattern_csv",
            "within_ms",
            "slide_ms",
            "switch_depth",
            "severity",
            "score",
        )
        .collect()
    )
    return [r.asDict() for r in rows]


def load_optimizer_coefficients() -> dict[str, float]:
    try:
        row = spark.table(T_COEFFICIENTS).orderBy(F.col("calibrated_at").desc()).first()
        if row is None:
            return {"mem_vertex": 0.7, "mem_edge": 0.3, "cpu_edge": 0.8, "cpu_vertex": 0.2}
        return {
            "mem_vertex": float(getattr(row, "mem_vertex", getattr(row, "mem_coef", 0.7))),
            "mem_edge": float(getattr(row, "mem_edge", 0.3)),
            "cpu_edge": float(getattr(row, "cpu_edge", getattr(row, "cpu_coef", 0.8))),
            "cpu_vertex": float(getattr(row, "cpu_vertex", 0.2)),
        }
    except Exception:
        return {"mem_vertex": 0.7, "mem_edge": 0.3, "cpu_edge": 0.8, "cpu_vertex": 0.2}


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


def make_process_partition(query_configs: list[dict[str, Any]], coefficients: dict[str, float], native_runtime: dict[str, Any]):
    # Broadcast values are plain dictionaries, not Spark objects.
    config_by_key = {
        (str(q["query_id"]), str(q["query_version"])): q
        for q in query_configs
    }

    def process_partition(rows):
        bridge = CETBridge()
        bridge.set_cost_coefficients(
            coefficients["mem_vertex"],
            coefficients["mem_edge"],
            coefficients["cpu_edge"],
            coefficients["cpu_vertex"],
        )

        bucket: dict[tuple[str, str, str], list[tuple[int, str, str, int]]] = {}
        for r in rows:
            query_id = str(r.query_id)
            query_version = str(r.query_version)
            partition_key = str(r.partition_key)
            key = (query_id, query_version, partition_key)
            bucket.setdefault(key, []).append(
                (
                    int(r.event_id),
                    partition_key,
                    str(r.event_type),
                    int(r.event_time_ms),
                )
            )

        for (query_id, query_version, partition_key), events in bucket.items():
            cfg = config_by_key.get((query_id, query_version))
            if cfg is None:
                yield (
                    "err",
                    query_id,
                    query_version,
                    "",
                    partition_key,
                    [],
                    None,
                    None,
                    0,
                    0.0,
                    "unknown",
                    0.0,
                    0.0,
                    0.0,
                    0.0,
                    "{}",
                    "query config missing on executor",
                )
                continue

            try:
                events = sorted(events, key=lambda x: (x[3], x[0]))
                deduped = []
                seen = set()
                for ev in events:
                    if ev[0] not in seen:
                        deduped.append(ev)
                        seen.add(ev[0])
                events = deduped

                q = bridge.parse_query(
                    query_id,
                    str(cfg["pattern_csv"]),
                    int(cfg["within_ms"]),
                    int(cfg["slide_ms"]),
                )

                edges = build_temporal_edges(events)
                switch_depth = int(cfg.get("switch_depth") or 2)
                match = bridge.run_hcet_parallel(
                    q,
                    events,
                    edges,
                    switch_depth=switch_depth,
                    native_threads=int(native_runtime.get("native_threads", 1)),
                    enable_mmap_arena=bool(native_runtime.get("enable_mmap_arena", False)),
                    mmap_workspace_bytes=int(native_runtime.get("mmap_workspace_bytes", 0)),
                    madvise_hugepage=bool(native_runtime.get("madvise_hugepage", False)),
                )
                event_time_by_id = {int(e[0]): int(e[3]) for e in events}
                stats_json = json.dumps(match.stats, sort_keys=True)

                for path in match.paths:
                    times = [event_time_by_id.get(int(eid)) for eid in path]
                    times = [t for t in times if t is not None]
                    yield (
                        "ok",
                        query_id,
                        query_version,
                        trend_id(query_id, query_version, partition_key, [int(x) for x in path]),
                        partition_key,
                        [int(x) for x in path],
                        min(times) if times else None,
                        max(times) if times else None,
                        len(path),
                        float(cfg.get("score") or 1.0),
                        str(cfg.get("severity") or "medium"),
                        float(len(match.paths)),
                        float(match.stats.get("paths_truncated", 0)),
                        float(match.stats.get("states_truncated", 0)),
                        float(match.stats.get("temporal_rejects", 0)),
                        stats_json,
                        "",
                    )
            except Exception as e:
                payload = {
                    "event_count": len(events),
                    "first_event_time_ms": min([e[3] for e in events]) if events else None,
                    "last_event_time_ms": max([e[3] for e in events]) if events else None,
                }
                yield (
                    "err",
                    query_id,
                    query_version,
                    "",
                    partition_key,
                    [],
                    None,
                    None,
                    0,
                    0.0,
                    str(cfg.get("severity") or "unknown"),
                    0.0,
                    0.0,
                    0.0,
                    0.0,
                    json.dumps(payload, sort_keys=True),
                    str(e)[:2000],
                )

    return process_partition


def normalize_current_events(df):
    cols = set(df.columns)
    missing = REQUIRED_COLS - cols
    if missing:
        raise RuntimeError(f"Missing required columns: {sorted(missing)}")

    payload_cols = [F.col(c) for c in df.columns]
    return (
        df
        .select(
            F.col("event_id").cast("long").alias("event_id"),
            F.col("partition_key").cast("string").alias("partition_key"),
            F.col("event_type").cast("string").alias("event_type"),
            F.col("event_time_ms").cast("long").alias("event_time_ms"),
            F.col("event_time").cast("timestamp").alias("event_time"),
            F.to_json(F.struct(*payload_cols)).alias("payload_json"),
        )
        .where(
            F.col("event_id").isNotNull()
            & F.col("partition_key").isNotNull()
            & F.col("event_type").isNotNull()
            & F.col("event_time_ms").isNotNull()
            & F.col("event_time").isNotNull()
        )
        .dropDuplicates(["partition_key", "event_id"])
    )


def query_config_df(query_configs: list[dict[str, Any]]):
    schema = StructType([
        StructField("query_id", StringType(), False),
        StructField("query_version", StringType(), False),
        StructField("pattern_csv", StringType(), False),
        StructField("within_ms", LongType(), False),
        StructField("slide_ms", LongType(), False),
        StructField("switch_depth", IntegerType(), False),
        StructField("severity", StringType(), True),
        StructField("score", DoubleType(), True),
    ])
    rows = [
        (
            str(q["query_id"]),
            str(q["query_version"]),
            str(q["pattern_csv"]),
            int(q["within_ms"]),
            int(q["slide_ms"]),
            int(q.get("switch_depth") or 2),
            str(q.get("severity") or "medium"),
            float(q.get("score") or 1.0),
        )
        for q in query_configs
    ]
    return spark.createDataFrame(rows, schema)


def upsert_event_buffer(current_events_df, qdf) -> None:
    buffer_df = (
        current_events_df
        .crossJoin(qdf.select("query_id", "query_version", "within_ms"))
        .withColumn("expires_at_ms", F.col("event_time_ms") + F.col("within_ms") + F.lit(ALLOWED_LATENESS_MS))
        .withColumn("expires_at", F.expr("timestamp_millis(expires_at_ms)"))
        .withColumn("updated_at", F.current_timestamp())
        .select(
            "query_id",
            "query_version",
            "partition_key",
            "event_id",
            "event_type",
            "event_time_ms",
            "event_time",
            "payload_json",
            "expires_at",
            "updated_at",
        )
    )

    buffer_df.createOrReplaceTempView("cet_event_buffer_upsert_tmp")
    spark.sql(f"""
      MERGE INTO {T_BUFFER} t
      USING cet_event_buffer_upsert_tmp s
      ON t.query_id = s.query_id
         AND t.query_version = s.query_version
         AND t.partition_key = s.partition_key
         AND t.event_id = s.event_id
      WHEN MATCHED THEN UPDATE SET
        t.event_type = s.event_type,
        t.event_time_ms = s.event_time_ms,
        t.event_time = s.event_time,
        t.payload_json = s.payload_json,
        t.expires_at = s.expires_at,
        t.updated_at = s.updated_at
      WHEN NOT MATCHED THEN INSERT *
    """)

    spark.sql(f"DELETE FROM {T_BUFFER} WHERE expires_at < current_timestamp()")


def combined_buffer_and_current(current_events_df, qdf):
    keys = current_events_df.select("partition_key").distinct()

    existing = (
        spark.table(T_BUFFER)
        .join(keys, "partition_key", "inner")
        .join(qdf.select("query_id", "query_version"), ["query_id", "query_version"], "inner")
        .where(F.col("expires_at") >= F.current_timestamp())
        .select(
            "query_id",
            "query_version",
            "partition_key",
            "event_id",
            "event_type",
            "event_time_ms",
            "event_time",
            "payload_json",
        )
    )

    current_expanded = (
        current_events_df
        .crossJoin(qdf.select("query_id", "query_version"))
        .select(
            "query_id",
            "query_version",
            "partition_key",
            "event_id",
            "event_type",
            "event_time_ms",
            "event_time",
            "payload_json",
        )
    )

    return (
        existing
        .unionByName(current_expanded)
        .dropDuplicates(["query_id", "query_version", "partition_key", "event_id"])
    )


def merge_trends(ok_df, batch_id: int) -> None:
    trends = (
        ok_df
        .withColumn("batch_id", F.lit(int(batch_id)).cast("long"))
        .withColumn("status", F.lit("active"))
        .withColumn("created_at", F.current_timestamp())
        .withColumn("updated_at", F.current_timestamp())
        .withColumn("retracted_at", F.lit(None).cast("timestamp"))
        .withColumn("replay_id", F.lit(None).cast("string"))
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
    )
    trends.createOrReplaceTempView("cet_trends_upsert_tmp")
    spark.sql(f"""
      MERGE INTO {T_TRENDS} t
      USING cet_trends_upsert_tmp s
      ON t.trend_id = s.trend_id
      WHEN MATCHED THEN UPDATE SET
        t.batch_id = s.batch_id,
        t.status = 'active',
        t.updated_at = s.updated_at,
        t.retracted_at = NULL,
        t.replay_id = s.replay_id,
        t.engine_stats = s.engine_stats
      WHEN NOT MATCHED THEN INSERT *
    """)


def merge_metrics(out_df, batch_id: int, batch_duration_seconds: float) -> None:
    enriched = (
        out_df
        .withColumn("native_thread_count", F.coalesce(F.get_json_object("engine_stats", "$.thread_count").cast("double"), F.get_json_object("engine_stats", "$.runtime.native_threads_used").cast("double"), F.lit(0.0)))
        .withColumn("used_parallel_runtime", F.when((F.get_json_object("engine_stats", "$.used_parallel_runtime") == "true") | (F.get_json_object("engine_stats", "$.runtime.parallel_enabled") == "true"), F.lit(1.0)).otherwise(F.lit(0.0)))
        .withColumn("used_mmap_arena", F.when((F.get_json_object("engine_stats", "$.used_mmap_arena") == "true") | (F.get_json_object("engine_stats", "$.runtime.used_mmap_arena") == "true"), F.lit(1.0)).otherwise(F.lit(0.0)))
        .withColumn("mmap_bytes_reserved", F.coalesce(F.get_json_object("engine_stats", "$.mmap_bytes_reserved").cast("double"), F.get_json_object("engine_stats", "$.runtime.workspace_bytes").cast("double"), F.lit(0.0)))
        .withColumn("mmap_bytes_used", F.coalesce(F.get_json_object("engine_stats", "$.mmap_bytes_used").cast("double"), F.lit(0.0)))
    )

    base_metrics = (
        enriched
        .groupBy("query_id", "query_version", "partition_key")
        .agg(
            F.sum("paths_found").alias("paths_found"),
            F.sum("paths_truncated").alias("paths_truncated"),
            F.sum("states_truncated").alias("states_truncated"),
            F.sum("temporal_rejects").alias("temporal_rejects"),
            F.count(F.when(F.col("status") == "err", 1)).alias("error_records"),
            F.count(F.when(F.col("status") == "ok", 1)).alias("success_records"),
            F.max("native_thread_count").alias("native_thread_count"),
            F.max("used_parallel_runtime").alias("used_parallel_runtime"),
            F.max("used_mmap_arena").alias("used_mmap_arena"),
            F.max("mmap_bytes_reserved").alias("mmap_bytes_reserved"),
            F.max("mmap_bytes_used").alias("mmap_bytes_used"),
        )
        .selectExpr(
            f"'{STREAM_ID}' as stream_id",
            f"CAST({int(batch_id)} AS BIGINT) as batch_id",
            "query_id",
            "query_version",
            "partition_key",
            """
            stack(
              11,
              'paths_found', CAST(paths_found AS DOUBLE),
              'paths_truncated', CAST(paths_truncated AS DOUBLE),
              'states_truncated', CAST(states_truncated AS DOUBLE),
              'temporal_rejects', CAST(temporal_rejects AS DOUBLE),
              'error_records', CAST(error_records AS DOUBLE),
              'success_records', CAST(success_records AS DOUBLE),
              'native_thread_count', CAST(native_thread_count AS DOUBLE),
              'used_parallel_runtime', CAST(used_parallel_runtime AS DOUBLE),
              'used_mmap_arena', CAST(used_mmap_arena AS DOUBLE),
              'mmap_bytes_reserved', CAST(mmap_bytes_reserved AS DOUBLE),
              'mmap_bytes_used', CAST(mmap_bytes_used AS DOUBLE)
            ) as (metric_name, metric_value)
            """,
        )
    )

    duration_schema = StructType([
        StructField("stream_id", StringType(), False),
        StructField("batch_id", LongType(), False),
        StructField("query_id", StringType(), True),
        StructField("query_version", StringType(), True),
        StructField("partition_key", StringType(), True),
        StructField("metric_name", StringType(), False),
        StructField("metric_value", DoubleType(), False),
    ])
    duration_df = spark.createDataFrame(
        [(STREAM_ID, int(batch_id), None, None, None, "batch_duration_seconds", float(batch_duration_seconds))],
        duration_schema,
    )

    metrics = base_metrics.unionByName(duration_df).withColumn("created_at", F.current_timestamp())
    metrics.createOrReplaceTempView("cet_metrics_upsert_tmp")
    spark.sql(f"""
      MERGE INTO {T_METRICS} t
      USING cet_metrics_upsert_tmp s
      ON t.stream_id <=> s.stream_id
         AND t.batch_id <=> s.batch_id
         AND t.query_id <=> s.query_id
         AND t.query_version <=> s.query_version
         AND t.partition_key <=> s.partition_key
         AND t.metric_name <=> s.metric_name
      WHEN MATCHED THEN UPDATE SET
        t.metric_value = s.metric_value,
        t.created_at = s.created_at
      WHEN NOT MATCHED THEN INSERT *
    """)


def merge_dlq(err_df, batch_id: int) -> None:
    dlq = (
        err_df
        .withColumn("stream_id", F.lit(STREAM_ID))
        .withColumn("batch_id", F.lit(int(batch_id)).cast("long"))
        .withColumn(
            "payload",
            F.to_json(F.struct(
                "engine_stats",
                "paths_found",
                "paths_truncated",
                "states_truncated",
                "temporal_rejects",
            )),
        )
        .withColumn(
            "dlq_id",
            F.sha2(F.concat_ws(":", "stream_id", F.col("batch_id").cast("string"), "query_id", "query_version", "partition_key", "error"), 256),
        )
        .withColumn("created_at", F.current_timestamp())
        .select(
            "dlq_id",
            "stream_id",
            "batch_id",
            "query_id",
            "query_version",
            "partition_key",
            "error",
            "payload",
            "created_at",
        )
    )
    dlq.createOrReplaceTempView("cet_dlq_upsert_tmp")
    spark.sql(f"""
      MERGE INTO {T_DLQ} t
      USING cet_dlq_upsert_tmp s
      ON t.dlq_id = s.dlq_id
      WHEN MATCHED THEN UPDATE SET
        t.error = s.error,
        t.payload = s.payload,
        t.created_at = s.created_at
      WHEN NOT MATCHED THEN INSERT *
    """)


def process_batch(df, batch_id: int) -> None:
    start = time.time()

    query_configs = load_active_queries()
    if not query_configs:
        return

    qdf = query_config_df(query_configs).cache()
    current_events = normalize_current_events(df).cache()

    if current_events.rdd.isEmpty():
        return

    coefficients = load_optimizer_coefficients()
    combined = combined_buffer_and_current(current_events, qdf)

    out_schema = StructType([
        StructField("status", StringType(), False),
        StructField("query_id", StringType(), False),
        StructField("query_version", StringType(), False),
        StructField("trend_id", StringType(), True),
        StructField("partition_key", StringType(), True),
        StructField("path", ArrayType(LongType()), True),
        StructField("trend_start_ms", LongType(), True),
        StructField("trend_end_ms", LongType(), True),
        StructField("event_count", IntegerType(), True),
        StructField("score", DoubleType(), True),
        StructField("severity", StringType(), True),
        StructField("paths_found", DoubleType(), True),
        StructField("paths_truncated", DoubleType(), True),
        StructField("states_truncated", DoubleType(), True),
        StructField("temporal_rejects", DoubleType(), True),
        StructField("engine_stats", StringType(), True),
        StructField("error", StringType(), True),
    ])

    partition_fn = make_process_partition(query_configs, coefficients, NATIVE_RUNTIME)
    out_rdd = combined.repartition("query_id", "query_version", "partition_key").rdd.mapPartitions(partition_fn)
    out_df = spark.createDataFrame(out_rdd, out_schema).cache()

    # Persist the current microbatch into the event buffer after building the combined view.
    upsert_event_buffer(current_events, qdf)

    if out_df.rdd.isEmpty():
        duration = float(time.time() - start)
        empty_schema = StructType([
            StructField("status", StringType(), False),
            StructField("query_id", StringType(), False),
            StructField("query_version", StringType(), False),
            StructField("partition_key", StringType(), True),
            StructField("paths_found", DoubleType(), True),
            StructField("paths_truncated", DoubleType(), True),
            StructField("states_truncated", DoubleType(), True),
            StructField("temporal_rejects", DoubleType(), True),
        ])
        empty = spark.createDataFrame([], empty_schema)
        merge_metrics(empty, batch_id, duration)
        return

    ok_df = out_df.where(F.col("status") == "ok")
    err_df = out_df.where(F.col("status") == "err")

    if not ok_df.rdd.isEmpty():
        merge_trends(ok_df, batch_id)

    if not err_df.rdd.isEmpty():
        merge_dlq(err_df, batch_id)

    duration = float(time.time() - start)
    merge_metrics(out_df, batch_id, duration)


ensure_tables()

stream_df = spark.readStream.table(T_EVENTS).withWatermark("event_time", "30 minutes")

query = (
    stream_df
    .writeStream
    .foreachBatch(process_batch)
    .queryName("0xdsi_cet_prod_v2")
    .option("checkpointLocation", CHECKPOINT_LOCATION)
    .start()
)
