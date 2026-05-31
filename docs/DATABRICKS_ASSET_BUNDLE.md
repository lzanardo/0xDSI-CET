# Databricks Asset Bundle

v4 adds `databricks.yml` and `resources/0xdsi_cet_v4_jobs.yml` so the project can
be deployed through Databricks Asset Bundles.

Typical flow:

```bash
python -m build
databricks bundle validate -t dev
databricks bundle deploy -t dev
databricks bundle run cet_v4_streaming_runtime -t dev
```

Production settings should use Unity Catalog volumes or cloud object storage for
checkpoints, never `/tmp`.
