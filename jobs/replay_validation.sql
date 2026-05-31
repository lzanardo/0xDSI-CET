-- Databricks SQL replay validation checks.
-- Requires:
--   CREATE OR REPLACE TEMP VIEW cet_replay_target AS SELECT 'replay-...' AS replay_id;

SELECT
  'recomputed_count' AS check_name,
  COUNT(*) AS value
FROM main.cet.cet_recomputed_trends_v2 r
JOIN cet_replay_target t ON r.replay_id = t.replay_id

UNION ALL

SELECT
  'retracted_count' AS check_name,
  COUNT(*) AS value
FROM main.cet.cet_complete_trends_v2 tr
JOIN cet_replay_target t ON tr.replay_id = t.replay_id
WHERE tr.status = 'retracted'

UNION ALL

SELECT
  'active_after_replay_count' AS check_name,
  COUNT(*) AS value
FROM main.cet.cet_complete_trends_v2 tr
JOIN cet_replay_target t ON tr.replay_id = t.replay_id
WHERE tr.status = 'active';
