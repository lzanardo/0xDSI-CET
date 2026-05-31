"""Ingest ZeroBus events into the canonical CET event table.

Usage in Databricks job:
  spark_python_task parameters:
    --config-json '{"transport":"http","endpoint":"https://...","target_table":"main.cet.cet_events"}'

For deterministic replay/testing:
    --config-json '{"transport":"file","file_path":"/dbfs/tmp/zerobus.jsonl"}'
"""
from __future__ import annotations

import argparse
import json
from pyspark.sql import functions as F  # type: ignore

from bindings.python.connectors.zerobus import ZeroBusClient, ZeroBusConfig


def parse_args() -> argparse.Namespace:
    p = argparse.ArgumentParser()
    p.add_argument("--config-json", default="{}")
    p.add_argument("--config-path", default="")
    return p.parse_args()


def load_config(args: argparse.Namespace) -> dict:
    if args.config_path:
        with open(args.config_path, "r", encoding="utf-8") as fh:
            return json.load(fh)
    return json.loads(args.config_json or "{}")


args = parse_args()
config = ZeroBusConfig.from_dict(load_config(args))
client = ZeroBusClient(config)

total = 0
for batch in client.batches():
    if not batch:
        continue
    df = spark.createDataFrame(batch)  # type: ignore[name-defined]
    df = df.withColumn("event_time", F.coalesce(F.col("event_time").cast("timestamp"), (F.col("event_time_ms") / 1000).cast("timestamp")))
    df.write.mode("append").saveAsTable(config.target_table)
    total += len(batch)

print(f"zerobus ingest completed: {total} events -> {config.target_table}")
