"""Materialize CET Temporal Security Knowledge Graph tables from buffered events and active trends."""
from __future__ import annotations
import argparse
from pyspark.sql import functions as F  # type: ignore
from bindings.python.temporal_kg import temporal_kg_ddls

p = argparse.ArgumentParser()
p.add_argument("--catalog", default="main")
p.add_argument("--schema", default="cet")
args = p.parse_args()

for ddl in temporal_kg_ddls(args.catalog, args.schema):
    spark.sql(ddl)

spark.sql(f"""
INSERT INTO {args.catalog}.{args.schema}.cet_trend_evidence_v5
SELECT trend_id, query_id, event_id, ordinal, risk_score, current_timestamp()
FROM {args.catalog}.{args.schema}.cet_complete_trends_v2
LATERAL VIEW posexplode(path) p AS ordinal, event_id
WHERE state = 'active'
""")
print("temporal KG trend evidence materialized")
