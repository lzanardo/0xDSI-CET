# Security Graph Builder

The v4 graph builder converts normalized telemetry into native CET graph input.

Edges emitted:

- `temporal_next`
- `same_user`
- `same_host`
- `same_session`
- `same_source_ip`
- `same_cloud_account`
- `same_process`
- `same_asset`
- `parent_process`

The C engine receives integer event edges. Edge metadata is retained by the
Python layer for audit, relation validation, and future relation-aware native
planning.
