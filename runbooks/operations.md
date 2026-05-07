# 0xDSI-CET Ops Runbook

## Incident: native bridge failures
1. Check `cet_dead_letter` growth.
2. Validate shared library checksum env `OXDSI_CET_SHA256`.
3. Restart stream from checkpoint.

## Incident: late-event surge
1. Run `jobs/late_event_replay.sql` workflow task.
2. Verify repaired trends upsert count.

## SLOs
- p95 batch processing under 30s
- replay job completion under 15m
- dead-letter ratio < 0.5%
