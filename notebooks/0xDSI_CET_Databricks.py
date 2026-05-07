# Databricks notebook source
import hashlib
from pyspark.sql import functions as F
from pyspark.sql.types import StructType, StructField, StringType, LongType, ArrayType, DoubleType
from bindings.python.bridge import CETBridge

CATALOG = "main"
SCHEMA = "cet"
import json
REQUIRED_COLS = {c["name"] for c in json.loads(open("contracts/spark_input_schema_v1.json").read())["required_columns"]}

spark.sql(f"CREATE TABLE IF NOT EXISTS {CATALOG}.{SCHEMA}.cet_complete_trends (trend_id STRING, batch_id BIGINT, partition_key STRING, path ARRAY<BIGINT>, created_at TIMESTAMP) USING DELTA")
spark.sql(f"CREATE TABLE IF NOT EXISTS {CATALOG}.{SCHEMA}.cet_metrics (batch_id BIGINT, metric_name STRING, metric_value DOUBLE, created_at TIMESTAMP) USING DELTA")
spark.sql(f"CREATE TABLE IF NOT EXISTS {CATALOG}.{SCHEMA}.cet_dead_letter (batch_id BIGINT, partition_key STRING, error STRING, payload STRING, created_at TIMESTAMP) USING DELTA")


QUERY_VERSION = "v1"

def trend_id(partition_key: str, path: list[int]) -> str:
    return hashlib.sha256(f"{QUERY_VERSION}:{partition_key}:{','.join(map(str, path))}".encode()).hexdigest()


def load_coefficients(bridge):
    try:
        row = spark.table(f"{CATALOG}.{SCHEMA}.cet_optimizer_coefficients").orderBy(F.col("calibrated_at").desc()).first()
        if row is not None:
            bridge.set_cost_coefficients(float(row.mem_coef), float(row.mem_coef if hasattr(row, "mem_edge") else 0.3), float(row.cpu_coef), 0.2)
    except Exception:
        pass

def process_partition(rows):
    bridge = CETBridge()
    load_coefficients(bridge)
    query_obj = bridge.parse_query("security", "AuthFail+,PrivEsc,DataAccess", 30 * 60 * 1000, 5 * 60 * 1000)
    bucket = {}
    for r in rows:
        bucket.setdefault(r.partition_key, []).append((int(r.event_id), r.partition_key, r.event_type, int(r.event_time_ms)))
    for pkey, events in bucket.items():
        try:
            events = sorted(events, key=lambda x: x[3])
            edges = [(events[i-1][0], events[i][0], events[0][3], events[-1][3]) for i in range(1, len(events))]
            match = bridge.run_hcet(query_obj, events, edges)
            for p in match.paths:
                yield ("ok", trend_id(pkey, p), pkey, [int(x) for x in p], float(len(match.paths)), "")
        except Exception as e:
            yield ("err", "", pkey, [], 0.0, str(e)[:1000])


def process_batch(df, batch_id: int):
    import time
    batch_start=time.time()
    cols = set(df.columns)
    missing = REQUIRED_COLS - cols
    if missing:
        raise RuntimeError(f"Missing required columns: {sorted(missing)}")

    base = df.select("event_id", "partition_key", "event_type", "event_time_ms").rdd.mapPartitions(process_partition)
    out_schema = StructType([
        StructField("status", StringType()),
        StructField("trend_id", StringType()),
        StructField("partition_key", StringType()),
        StructField("path", ArrayType(LongType())),
        StructField("paths_found", DoubleType()),
        StructField("error", StringType()),
    ])
    out_df = spark.createDataFrame(base, out_schema).cache()
    if out_df.rdd.isEmpty():
        return

    ok_df = out_df.filter(F.col("status") == "ok").drop("status", "error")
    err_df = out_df.filter(F.col("status") == "err")

    success_count=0
    error_count=0
    if not ok_df.rdd.isEmpty():
        ok_df = ok_df.withColumn("batch_id", F.lit(batch_id)).withColumn("created_at", F.current_timestamp())
        ok_df.createOrReplaceTempView("cet_out_tmp")
        spark.sql(f"""
            MERGE INTO {CATALOG}.{SCHEMA}.cet_complete_trends t
            USING cet_out_tmp s
            ON t.trend_id = s.trend_id
            WHEN NOT MATCHED THEN INSERT (trend_id, batch_id, partition_key, path, created_at)
            VALUES (s.trend_id, s.batch_id, s.partition_key, s.path, s.created_at)
        """)
        success_count = ok_df.count()
        metrics_df = ok_df.groupBy("batch_id").agg(F.sum("paths_found").alias("metric_value")).withColumn("metric_name", F.lit("paths_found")).withColumn("created_at", F.current_timestamp())
        metrics_df.select("batch_id", "metric_name", "metric_value", "created_at").write.mode("append").saveAsTable(f"{CATALOG}.{SCHEMA}.cet_metrics")

    if not err_df.rdd.isEmpty():
        error_count = err_df.count()
        err_df.select(F.lit(batch_id).alias("batch_id"), "partition_key", "error", F.to_json(F.struct(*[F.col(c) for c in err_df.columns])).alias("payload"), F.current_timestamp().alias("created_at")).write.mode("append").saveAsTable(f"{CATALOG}.{SCHEMA}.cet_dead_letter")

    duration = float(time.time()-batch_start)
    extra = [(batch_id, "batch_duration_seconds", duration), (batch_id, "success_records", float(success_count)), (batch_id, "error_records", float(error_count))]
    spark.createDataFrame(extra,["batch_id","metric_name","metric_value"]).withColumn("created_at",F.current_timestamp()).write.mode("append").saveAsTable(f"{CATALOG}.{SCHEMA}.cet_metrics")


stream_df = spark.readStream.table(f"{CATALOG}.{SCHEMA}.cet_events").withWatermark("event_time", "30 minutes")
query = stream_df.writeStream.foreachBatch(process_batch).option("checkpointLocation", "/tmp/chk/0xdsi_cet_prod").start()
