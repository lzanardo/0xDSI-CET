# Databricks pipeline source
"""0xDSI-CET Spark Declarative Pipelines integration.

This file is intended to be used as a Lakeflow/Spark Declarative Pipelines
source. It creates a declarative ingestion/normalization layer that feeds the
CET runtime tables. The native CET engine still runs in the v5 main runtime or
bridge jobs; SDP owns the managed, declarative data plane.
"""
from __future__ import annotations

from pyspark import pipelines as dp
from pyspark.sql import functions as F


def _conf(name: str, default: str) -> str:
    try:
        return spark.conf.get(name, default)  # type: ignore[name-defined]
    except Exception:
        return default


SOURCE_TABLE = _conf("oxdsi.cet.sdp.source_table", "main.cet.cet_events")
QUERY_REGISTRY_TABLE = _conf("oxdsi.cet.sdp.query_registry_table", "main.cet.cet_query_registry_v4")
BUFFER_RETENTION_DAYS = int(_conf("oxdsi.cet.sdp.buffer_retention_days", "7"))


@dp.table(
    name="cet_sdp_bronze_events",
    comment="Raw security events ingested through Spark Declarative Pipelines for 0xDSI-CET.",
)
@dp.expect_or_drop("event_id_present", "event_id IS NOT NULL")
@dp.expect_or_drop("event_type_present", "event_type IS NOT NULL AND length(event_type) > 0")
@dp.expect_or_drop("event_time_present", "event_time IS NOT NULL")
def cet_sdp_bronze_events():
    return spark.readStream.table(SOURCE_TABLE)  # type: ignore[name-defined]


@dp.table(
    name="cet_sdp_silver_events",
    comment="Canonical 0xDSI-CET security event stream normalized by SDP.",
)
@dp.expect_or_drop("partition_key_present", "partition_key IS NOT NULL AND length(partition_key) > 0")
@dp.expect_or_drop("event_time_ms_present", "event_time_ms IS NOT NULL")
def cet_sdp_silver_events():
    raw = spark.readStream.table("cet_sdp_bronze_events")  # type: ignore[name-defined]
    passthrough = [c for c in raw.columns if c not in {"attributes", "payload"}]
    return (
        raw
        .withColumn("event_id", F.col("event_id").cast("long"))
        .withColumn("partition_key", F.col("partition_key").cast("string"))
        .withColumn("event_type", F.col("event_type").cast("string"))
        .withColumn("event_time_ms", F.col("event_time_ms").cast("long"))
        .withColumn("event_time", F.col("event_time").cast("timestamp"))
        .withColumn("attributes_json", F.to_json(F.struct(*[F.col(c) for c in passthrough])))
        .withColumn("ingested_at", F.current_timestamp())
        .select("event_id", "partition_key", "event_type", "event_time_ms", "event_time", "attributes_json", "ingested_at")
    )


# This target is used by downstream CET runtime jobs. Keeping it as a streaming
# table gives SDP ownership of lineage, expectations, and incremental planning.
dp.create_streaming_table(
    name="cet_sdp_event_buffer",
    comment="Managed SDP streaming table used as the event buffer source for CET runtime.",
)


@dp.append_flow(target="cet_sdp_event_buffer", name="append_canonical_events_to_cet_buffer")
def append_canonical_events_to_cet_buffer():
    return spark.readStream.table("cet_sdp_silver_events")  # type: ignore[name-defined]


@dp.materialized_view(
    name="cet_sdp_event_quality_metrics",
    comment="Event quality metrics computed declaratively for 0xDSI-CET SDP ingestion.",
)
def cet_sdp_event_quality_metrics():
    silver = spark.read.table("cet_sdp_silver_events")  # type: ignore[name-defined]
    return (
        silver.groupBy("event_type")
        .agg(
            F.count("*").alias("event_count"),
            F.countDistinct("partition_key").alias("partition_count"),
            F.min("event_time").alias("min_event_time"),
            F.max("event_time").alias("max_event_time"),
        )
        .withColumn("computed_at", F.current_timestamp())
    )


@dp.materialized_view(
    name="cet_sdp_query_registry",
    comment="Read-only declarative view of enabled CET query registry entries.",
)
def cet_sdp_query_registry():
    return spark.read.table(QUERY_REGISTRY_TABLE).where("enabled = true")  # type: ignore[name-defined]
