"""Skeleton production counterfactual replay job."""
from __future__ import annotations
import argparse
from pyspark.sql import functions as F  # type: ignore

p = argparse.ArgumentParser()
p.add_argument("--catalog", default="main")
p.add_argument("--schema", default="cet")
p.add_argument("--lookback_days", default="7")
args = p.parse_args()

spark.sql(f"""
CREATE TABLE IF NOT EXISTS {args.catalog}.{args.schema}.cet_counterfactual_runs_v5 (
  run_id STRING, created_at TIMESTAMP, lookback_days INT,
  base_count BIGINT, candidate_count BIGINT, added_count BIGINT, removed_count BIGINT
) USING DELTA
""")
spark.createDataFrame([("manual-run-required", int(args.lookback_days), 0, 0, 0, 0)],
                      ["run_id", "lookback_days", "base_count", "candidate_count", "added_count", "removed_count"]) \
    .withColumn("created_at", F.current_timestamp()) \
    .select("run_id", "created_at", "lookback_days", "base_count", "candidate_count", "added_count", "removed_count") \
    .write.mode("append").saveAsTable(f"{args.catalog}.{args.schema}.cet_counterfactual_runs_v5")
print("counterfactual replay contract emitted")
