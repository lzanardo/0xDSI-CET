"""Expire CET partial states by watermark/TTL.

Run as a Databricks job after streaming or replay windows. Parameters are kept
simple so this can be called from Databricks Asset Bundles.
"""
from __future__ import annotations

import argparse
from pyspark.sql import functions as F  # type: ignore


def parse_args() -> argparse.Namespace:
    p = argparse.ArgumentParser()
    p.add_argument("--catalog", default="main")
    p.add_argument("--schema", default="cet")
    p.add_argument("--retention-hours", type=int, default=24)
    return p.parse_args()


args = parse_args()
state_table = f"{args.catalog}.{args.schema}.cet_partial_state_v4"
audit_table = f"{args.catalog}.{args.schema}.cet_state_ttl_audit"

spark.sql(f"CREATE TABLE IF NOT EXISTS {audit_table} (expired_count BIGINT, retention_hours INT, processed_at TIMESTAMP) USING DELTA")
watermark = F.current_timestamp() - F.expr(f"INTERVAL {int(args.retention_hours)} HOURS")
state = spark.table(state_table)
expired = state.where(F.col("updated_at") < watermark)
expired_count = expired.count()
active = state.where(F.col("updated_at") >= watermark)
active.write.mode("overwrite").option("overwriteSchema", "true").saveAsTable(state_table)
spark.createDataFrame([(expired_count, int(args.retention_hours))], ["expired_count", "retention_hours"]).withColumn("processed_at", F.current_timestamp()).write.mode("append").saveAsTable(audit_table)
print(f"expired {expired_count} CET states from {state_table}")
