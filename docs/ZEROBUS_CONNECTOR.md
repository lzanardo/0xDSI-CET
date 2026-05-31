# ZeroBus connector

The ZeroBus connector is transport-pluggable because ZeroBus protocol details are
private to 0xDSI. The initial implementation supports:

- `file`: deterministic JSONL replay;
- `http`: one-shot polling endpoint that returns JSON array or NDJSON;
- `mock`: unit tests and embedded development.

All transports normalize events into the canonical CET event shape:

```json
{
  "event_id": 1,
  "partition_key": "u1",
  "event_type": "AuthFail",
  "event_time_ms": 1710000000000,
  "event_time": "2026-05-31T18:00:00Z",
  "attributes": {},
  "source_name": "zerobus",
  "raw_json": "..."
}
```

A production native ZeroBus protocol can be added by extending
`ZeroBusClient.poll()` with a new `transport` value while preserving downstream
Spark/SDP/CET contracts.
