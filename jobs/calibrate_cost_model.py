"""Calibrate optimizer coefficients from historical metrics (offline step)."""
from pyspark.sql import functions as F
CATALOG='main'; SCHEMA='cet'

m = spark.table(f"{CATALOG}.{SCHEMA}.cet_metrics")
# Example calibration proxy (placeholder): compute average latency and emit coefficient suggestions.
agg = m.groupBy().agg(F.avg(F.when(F.col('metric_name')=='batch_duration_seconds',F.col('metric_value'))).alias('avg_batch_sec'))
coef = agg.withColumn('mem_coef', F.lit(0.7)).withColumn('cpu_coef', F.lit(0.8)).withColumn('calibrated_at', F.current_timestamp())
coef.write.mode('overwrite').saveAsTable(f"{CATALOG}.{SCHEMA}.cet_optimizer_coefficients")
print('optimizer coefficients table updated')
