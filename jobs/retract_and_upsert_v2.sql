-- Databricks SQL
-- Complete replay closure for CET v2.
--
-- Before running, set the target replay id:
--   CREATE OR REPLACE TEMP VIEW cet_replay_target AS SELECT 'replay-...' AS replay_id;
--
-- This script:
--   1. retracts active trends in impacted windows that disappeared after recompute,
--   2. upserts recomputed trends,
--   3. preserves trend lineage via replay_id/status/retracted_at.

CREATE OR REPLACE TEMP VIEW cet_replay_impacted AS
SELECT w.*
FROM main.cet.cet_impacted_windows_v2 w
JOIN cet_replay_target t
  ON w.replay_id = t.replay_id;

CREATE OR REPLACE TEMP VIEW cet_replay_stale_trends AS
SELECT DISTINCT old.trend_id
FROM main.cet.cet_complete_trends_v2 old
JOIN cet_replay_impacted w
  ON old.query_id = w.query_id
 AND old.query_version = w.query_version
 AND old.partition_key = w.partition_key
 AND old.status = 'active'
 AND old.trend_end_ms >= w.window_start_ms
 AND old.trend_start_ms <= w.window_end_ms
LEFT ANTI JOIN main.cet.cet_recomputed_trends_v2 r
  ON r.replay_id = w.replay_id
 AND r.trend_id = old.trend_id;

MERGE INTO main.cet.cet_complete_trends_v2 t
USING cet_replay_stale_trends s
ON t.trend_id = s.trend_id
WHEN MATCHED THEN UPDATE SET
  t.status = 'retracted',
  t.retracted_at = current_timestamp(),
  t.updated_at = current_timestamp(),
  t.replay_id = (SELECT replay_id FROM cet_replay_target LIMIT 1);

MERGE INTO main.cet.cet_complete_trends_v2 t
USING (
  SELECT r.*
  FROM main.cet.cet_recomputed_trends_v2 r
  JOIN cet_replay_target target
    ON r.replay_id = target.replay_id
) s
ON t.trend_id = s.trend_id
WHEN MATCHED THEN UPDATE SET
  t.batch_id = s.batch_id,
  t.path = s.path,
  t.trend_start_ms = s.trend_start_ms,
  t.trend_end_ms = s.trend_end_ms,
  t.event_count = s.event_count,
  t.score = s.score,
  t.severity = s.severity,
  t.status = 'active',
  t.updated_at = current_timestamp(),
  t.retracted_at = NULL,
  t.replay_id = s.replay_id,
  t.engine_stats = s.engine_stats
WHEN NOT MATCHED THEN INSERT (
  trend_id,
  query_id,
  query_version,
  batch_id,
  partition_key,
  path,
  trend_start_ms,
  trend_end_ms,
  event_count,
  score,
  severity,
  status,
  created_at,
  updated_at,
  retracted_at,
  replay_id,
  engine_stats
) VALUES (
  s.trend_id,
  s.query_id,
  s.query_version,
  s.batch_id,
  s.partition_key,
  s.path,
  s.trend_start_ms,
  s.trend_end_ms,
  s.event_count,
  s.score,
  s.severity,
  'active',
  current_timestamp(),
  current_timestamp(),
  NULL,
  s.replay_id,
  s.engine_stats
);
