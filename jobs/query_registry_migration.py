"""Materialize a JSON query registry into Delta for governed rollout."""
from __future__ import annotations

import argparse, json
from pathlib import Path
from pyspark.sql import functions as F  # type: ignore


def parse_args() -> argparse.Namespace:
    p = argparse.ArgumentParser()
    p.add_argument("--catalog", default="main")
    p.add_argument("--schema", default="cet")
    p.add_argument("--registry", default="contracts/query_registry_v4.example.json")
    return p.parse_args()


args = parse_args()
data = json.loads(Path(args.registry).read_text())
rows = []
for q in data.get("queries", []):
    rows.append((q["query_id"], q.get("version", "v1"), q.get("name", q["query_id"]), q["pattern"], int(q["within_ms"]), int(q.get("slide_ms", q["within_ms"])), bool(q.get("enabled", True)), q.get("severity", "medium"), json.dumps(q, sort_keys=True)))

df = spark.createDataFrame(rows, "query_id string, query_version string, name string, pattern string, within_ms long, slide_ms long, enabled boolean, severity string, query_json string")
df.withColumn("updated_at", F.current_timestamp()).write.mode("overwrite").saveAsTable(f"{args.catalog}.{args.schema}.cet_query_registry_v4")
print(f"wrote {len(rows)} queries")
