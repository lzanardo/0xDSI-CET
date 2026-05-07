"""Persist and hydrate partial cache metadata for T-CET/H-CET across batches."""
from pyspark.sql import functions as F
CATALOG='main'; SCHEMA='cet'

spark.sql(f"CREATE TABLE IF NOT EXISTS {CATALOG}.{SCHEMA}.cet_partial_cache (partition_key STRING, key_vertex BIGINT, seq_idx INT, hits BIGINT, updated_at TIMESTAMP) USING DELTA")


def upsert_cache(df):
    df.createOrReplaceTempView('cache_tmp')
    spark.sql(f"""
      MERGE INTO {CATALOG}.{SCHEMA}.cet_partial_cache t
      USING cache_tmp s
      ON t.partition_key=s.partition_key AND t.key_vertex=s.key_vertex AND t.seq_idx=s.seq_idx
      WHEN MATCHED THEN UPDATE SET t.hits=s.hits, t.updated_at=current_timestamp()
      WHEN NOT MATCHED THEN INSERT (partition_key,key_vertex,seq_idx,hits,updated_at)
      VALUES (s.partition_key,s.key_vertex,s.seq_idx,s.hits,current_timestamp())
    """)

print('partial cache persistence utilities loaded')
