# Spark Declarative Pipelines integration

`notebooks/0xDSI_CET_SDP.py` is a Spark Declarative Pipelines / Lakeflow source
file. It creates:

- `cet_sdp_bronze_events`: raw streaming table with required field expectations;
- `cet_sdp_silver_events`: canonical security event stream;
- `cet_sdp_event_buffer`: streaming table targeted by an append flow;
- `cet_sdp_event_quality_metrics`: materialized view for operational checks;
- `cet_sdp_query_registry`: materialized view over enabled CET queries.

The design keeps SDP responsible for declarative ingestion, data quality,
incremental planning, and lineage. The native CET runtime continues to run in
main-ready jobs because graph matching is a custom native operation and does not
fit naturally into pure relational SDP expressions yet.

## Apply

```bash
./ci/v6_sdp_zerobus_regression.sh
```

## Deploy

The patch adds `resources/0xdsi_cet_sdp_zerobus_jobs.yml`, which is picked up by
the existing Databricks Asset Bundle include pattern.

```bash
databricks bundle validate -t dev
databricks bundle deploy -t dev
```
