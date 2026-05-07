"""Late event replay job scaffold for Databricks Workflows."""
from pyspark.sql import functions as F

CATALOG='main'; SCHEMA='cet'
late = spark.table(f"{CATALOG}.{SCHEMA}.cet_events").where(F.col('event_time') < F.current_timestamp() - F.expr('INTERVAL 5 MINUTES'))
impacted = late.groupBy('partition_key').agg(F.min('event_time').alias('start_ts'), F.max('event_time').alias('end_ts'))
impacted.write.mode('overwrite').saveAsTable(f"{CATALOG}.{SCHEMA}.cet_impacted_partitions")
print('Impacted partitions materialized; trigger recompute notebook next.')
