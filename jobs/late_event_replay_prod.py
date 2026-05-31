# Databricks notebook/job source
"""
Materialize impacted CET replay windows for late or corrected events.

This job does not recompute trends itself. It writes cet_impacted_windows_v2,
which jobs/recompute_trends_prod.py consumes.

Recommended usage:
  1. Provide replay_id, start_ms, end_ms for the event-time range that changed.
  2. Run this job.
  3. Run recompute_trends_prod.py for the same replay_id.
  4. Run retract_and_upsert_v2.sql.
"""
from __future__ import annotations

import uuid
from pyspark.sql import functions as F

CATALOG = spark.conf.get("oxdsi.cet.catalog", "main")
SCHEMA = spark.conf.get("oxdsi.cet.schema", "cet")
T_EVENTS = f"{CATALOG}.{SCHEMA}.cet_events"
T_QUERIES = f"{CATALOG}.{SCHEMA}.cet_queries"
T_IMPACTED = f"{CATALOG}.{SCHEMA}.cet_impacted_windows_v2"

DEFAULT_ALLOWED_LATENESS_MS = int(spark.conf.get("oxdsi.cet.allowedLatenessMs", str(30 * 60 * 1000)))
DEFAULT_LOOKBACK_DAYS = int(spark.conf.get("oxdsi.cet.replayLookbackDays", "7"))


def widget_or_conf(name: str, default: str) -> str:
    try:
        value = dbutils.widgets.get(name)  # type: ignore[name-defined]
        return value if value else default
    except Exception:
        return spark.conf.get(f"oxdsi.cet.{name}", default)


REPLAY_ID = widget_or_conf("replayId", f"replay-{uuid.uuid4()}")
START_MS = widget_or_conf("startMs", "")
END_MS = widget_or_conf("endMs", "")

spark.sql(f"""
  CREATE TABLE IF NOT EXISTS {T_IMPACTED} (
    replay_id STRING,
    query_id STRING,
    query_version STRING,
    partition_key STRING,
    window_start_ms BIGINT,
    window_end_ms BIGINT,
    source_min_event_time_ms BIGINT,
    source_max_event_time_ms BIGINT,
    reason STRING,
    created_at TIMESTAMP
  ) USING DELTA
""")

events = spark.table(T_EVENTS)

if START_MS and END_MS:
    changed = events.where(
        (F.col("event_time_ms") >= F.lit(int(START_MS))) &
        (F.col("event_time_ms") <= F.lit(int(END_MS)))
    )
    reason = f"manual_range:{START_MS}:{END_MS}"
else:
    # Fallback: last N days of event time that are older than the lateness threshold.
    # For best precision, pass explicit startMs/endMs from your ingestion correction job.
    changed = events.where(
        F.col("event_time") >= F.current_timestamp() - F.expr(f"INTERVAL {DEFAULT_LOOKBACK_DAYS} DAYS")
    ).where(
        F.col("event_time") < F.current_timestamp() - F.expr(f"INTERVAL {int(DEFAULT_ALLOWED_LATENESS_MS / 60000)} MINUTES")
    )
    reason = f"late_event_scan:last_{DEFAULT_LOOKBACK_DAYS}_days"

active_queries = (
    spark.table(T_QUERIES)
    .where(F.col("enabled") == True)
    .select("query_id", "query_version", "within_ms")
)

impacted = (
    changed
    .select("partition_key", "event_time_ms")
    .crossJoin(active_queries)
    .groupBy("query_id", "query_version", "partition_key", "within_ms")
    .agg(
        F.min("event_time_ms").alias("source_min_event_time_ms"),
        F.max("event_time_ms").alias("source_max_event_time_ms"),
    )
    .withColumn("replay_id", F.lit(REPLAY_ID))
    .withColumn("window_start_ms", F.col("source_min_event_time_ms") - F.col("within_ms") - F.lit(DEFAULT_ALLOWED_LATENESS_MS))
    .withColumn("window_end_ms", F.col("source_max_event_time_ms") + F.col("within_ms") + F.lit(DEFAULT_ALLOWED_LATENESS_MS))
    .withColumn("reason", F.lit(reason))
    .withColumn("created_at", F.current_timestamp())
    .select(
        "replay_id",
        "query_id",
        "query_version",
        "partition_key",
        "window_start_ms",
        "window_end_ms",
        "source_min_event_time_ms",
        "source_max_event_time_ms",
        "reason",
        "created_at",
    )
)

impacted.write.mode("append").saveAsTable(T_IMPACTED)

print(f"Replay windows materialized: replay_id={REPLAY_ID}")
