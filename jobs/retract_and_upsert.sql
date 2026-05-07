-- Retract stale trends and upsert repaired trends after recompute.
MERGE INTO main.cet.cet_complete_trends t
USING main.cet.cet_recomputed_trends r
ON t.trend_id = r.trend_id
WHEN MATCHED THEN UPDATE SET t.path = r.path, t.created_at = current_timestamp()
WHEN NOT MATCHED THEN INSERT (trend_id, batch_id, partition_key, path, created_at)
VALUES (r.trend_id, r.batch_id, r.partition_key, r.path, current_timestamp());
