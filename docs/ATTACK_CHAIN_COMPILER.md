# Attack Chain Compiler

The compiler maps a high-level attack-chain document to CET query registry JSON.
This lets 0xDSI model detections as temporal/causal chains rather than isolated
rules.

Example:

```json
{
  "chain_id": "priv_esc_exfil",
  "version": "v1",
  "steps": [
    {"event_type": "AuthFail", "max_repeats": null},
    {"event_type": "PrivEsc"},
    {"event_type": "DataAccess"}
  ],
  "within_ms": 1800000,
  "relations": [{"field": "user_id"}],
  "absence": [{"event_type": "MFAChallenge"}],
  "mitre": ["T1078"]
}
```
