"""Compute before/after replay diff for CET trends."""
from __future__ import annotations

import argparse
from pyspark.sql import functions as F  # type: ignore


def parse_args() -> argparse.Namespace:
    p = argparse.ArgumentParser()
    p.add_argument("--catalog", default="main")
    p.add_argument("--schema", default="cet")
    return p.parse_args()


args = parse_args()
current = spark.table(f"{args.catalog}.{args.schema}.cet_complete_trends_v2")
recomputed = spark.table(f"{args.catalog}.{args.schema}.cet_recomputed_trends")

stale = current.alias("c").join(recomputed.alias("r"), "trend_id", "left_anti").withColumn("diff_type", F.lit("stale_retract"))
new = recomputed.alias("r").join(current.alias("c"), "trend_id", "left_anti").withColumn("diff_type", F.lit("new_insert"))
changed = recomputed.alias("r").join(current.alias("c"), "trend_id", "inner").where(F.to_json("r.path") != F.to_json("c.path")).select("r.*").withColumn("diff_type", F.lit("changed_update"))

diff = stale.unionByName(new, allowMissingColumns=True).unionByName(changed, allowMissingColumns=True)
diff.write.mode("overwrite").saveAsTable(f"{args.catalog}.{args.schema}.cet_replay_diff")
print("replay diff written")
