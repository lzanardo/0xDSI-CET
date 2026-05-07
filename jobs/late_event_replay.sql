-- Recompute impacted partitions/windows for late events and upsert repaired trends.
-- Schedule this periodically in Databricks Workflows.

CREATE OR REPLACE TEMP VIEW late_events AS
SELECT partition_key, min(event_time) AS min_late_time, max(event_time) AS max_late_time
FROM main.cet.cet_events
WHERE event_time < current_timestamp() - INTERVAL 5 MINUTES
GROUP BY partition_key;

-- Placeholder: invoke notebook task to recompute CET for affected partitions.
SELECT * FROM late_events;
