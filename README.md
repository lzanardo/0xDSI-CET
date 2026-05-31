<!--
  0xDSI-CET README
  Main-ready v7: Complete Event Trend + Standing Temporal Security Intelligence
-->

<pre>
 ██████╗ ██╗  ██╗██████╗ ███████╗██╗       ██████╗███████╗████████╗
██╔═████╗╚██╗██╔╝██╔══██╗██╔════╝██║      ██╔════╝██╔════╝╚══██╔══╝
██║██╔██║ ╚███╔╝ ██║  ██║███████╗██║█████╗██║     █████╗     ██║   
████╔╝██║ ██╔██╗ ██║  ██║╚════██║██║╚════╝██║     ██╔══╝     ██║   
╚██████╔╝██╔╝ ██╗██████╔╝███████║██║      ╚██████╗███████╗   ██║   
 ╚═════╝ ╚═╝  ╚═╝╚═════╝ ╚══════╝╚═╝       ╚═════╝╚══════╝   ╚═╝   

     COMPLETE EVENT TREND ENGINE · STANDING TEMPORAL SECURITY INTELLIGENCE
</pre>

<p align="center">
  <strong>0xDSI-CET turns security telemetry into live attack trends, replayable evidence graphs, and explainable investigation intelligence.</strong>
</p>

<p align="center">
  <code>Native C Runtime</code> · <code>pthread</code> · <code>mmap</code> · <code>Databricks</code> · <code>Spark Declarative Pipelines</code> · <code>Delta Lake</code> · <code>ZeroBus</code> · <code>Standing Queries</code> · <code>Temporal KG</code>
</p>

---

## Executive Summary

**0xDSI-CET** is a security-native **Complete Event Trend** runtime for detecting, explaining, replaying, and retracting temporal attack chains.

It is designed for one mission:

> Transform raw telemetry into trustworthy temporal security intelligence.

Instead of emitting isolated alerts, 0xDSI-CET follows the full trend:

```text
failed authentication → successful access → privilege escalation → sensitive data access → outbound movement
```

Then it preserves the evidence, entities, graph path, risk explanation, replay lineage, and downstream investigation context.

<pre>
┌──────────────────────────────────────────────────────────────────────────────┐
│                              0xDSI-CET MISSION                               │
├──────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│   Alert fragments are not enough.                                             │
│   Security teams need complete, temporal, causal explanations.                │
│                                                                              │
│   0xDSI-CET = events → graph → trend → evidence → replay → investigation     │
│                                                                              │
└──────────────────────────────────────────────────────────────────────────────┘
</pre>

---

## The Architecture

<pre>
                                  ┌────────────────────────────┐
                                  │        TELEMETRY           │
                                  │ SIEM · EDR · IAM · CLOUD   │
                                  │ NET · DATA · APP · ZEROBUS │
                                  └──────────────┬─────────────┘
                                                 │
                                                 ▼
┌──────────────────────────────────────────────────────────────────────────────┐
│                       SPARK DECLARATIVE PIPELINES / SDP                      │
│                                                                              │
│      bronze raw events → quality expectations → silver canonical events      │
│                                │                                             │
│                                ▼                                             │
│                         Delta Event Buffer                                   │
└───────────────────────────────┬──────────────────────────────────────────────┘
                                │
                                ▼
┌──────────────────────────────────────────────────────────────────────────────┐
│                                0xDSI-CET CORE                                │
│                                                                              │
│  Security Graph Builder → Native C H-CET Kernel → DSL Validator → Risk Model │
│          │                         │                         │               │
│          ▼                         ▼                         ▼               │
│  identity/session/host       pthread + mmap            absence / where       │
│  process/cloud/data          temporal checks           relation constraints  │
└───────────────────────────────┬──────────────────────────────────────────────┘
                                │
                                ▼
┌──────────────────────────────────────────────────────────────────────────────┐
│                         STANDING TEMPORAL GRAPH STATE                        │
│                                                                              │
│     mutable graph · mutation log · standing trend queries · cancel events    │
└───────────────────────────────┬──────────────────────────────────────────────┘
                                │
                                ▼
┌──────────────────────────────────────────────────────────────────────────────┐
│                              SECURITY OUTCOMES                               │
│                                                                              │
│  trends · retractions · temporal KG · counterfactuals · investigation API    │
└──────────────────────────────────────────────────────────────────────────────┘
</pre>

---

## Why Complete Event Trend?

A traditional alert tells you that something happened.

A Complete Event Trend tells you **how a threat evolved**.

```text
┌────────────┐   ┌────────────┐   ┌────────────┐   ┌────────────┐
│ AuthFail+  │ → │  PrivEsc   │ → │ DataAccess │ → │  Transfer  │
└────────────┘   └────────────┘   └────────────┘   └────────────┘
       │                │                │                │
       ▼                ▼                ▼                ▼
   same_user        same_host       sensitive_data     external_dest
```

CET adds the missing layer between event streaming, graph analytics, and security operations:

| Capability | Traditional alerting | 0xDSI-CET |
|---|---:|---:|
| Event sequence detection | Partial | Native |
| Entity relationship graph | Partial | Native |
| Absence / negative evidence | Rare | Native DSL |
| Replay and retraction | Hard | First-class |
| Historical what-if analysis | Manual | Built-in |
| Agent-ready evidence | Rare | Structured output |
| Lakehouse-native deployment | Usually external | Databricks + Delta |
| Standing trend detection | Rare | v7 runtime |

---

## Why 0xDSI Uses CET

0xDSI needs a detection layer that is not just fast, but **defensible**.

CET helps 0xDSI:

1. Detect complete attack chains rather than isolated events.
2. Preserve context across identities, hosts, sessions, processes, cloud resources, and data assets.
3. Explain why a trend exists using evidence graphs and risk factors.
4. Retract stale conclusions when late or corrected events change the truth.
5. Replay detection logic against historical data before deploying new rules.
6. Feed investigation agents with structured context instead of raw logs.
7. Run natively on the lakehouse using Spark, Delta, Databricks, and SDP.
8. Integrate with ZeroBus and downstream event-driven security workflows.

<pre>
        ┌─────────────┐       ┌─────────────┐       ┌─────────────┐
        │   EVENTS    │ ───▶  │   TRENDS    │ ───▶  │  EVIDENCE   │
        └─────────────┘       └─────────────┘       └─────────────┘
               │                     │                     │
               ▼                     ▼                     ▼
        ┌─────────────┐       ┌─────────────┐       ┌─────────────┐
        │   REPLAY    │ ◀───  │    RISK     │ ───▶  │   AGENTS    │
        └─────────────┘       └─────────────┘       └─────────────┘
</pre>

---

## Core Capabilities

### 1. Native C Runtime

The native engine provides the fast path for graph trend matching:

- M-CET, T-CET, and H-CET execution modes;
- temporal correctness diagnostics;
- overflow and truncation statistics;
- optional `pthread` parallel execution;
- optional `mmap` runtime workspace;
- Python bridge for Databricks, local tests, and batch replay.

```text
C kernel = deterministic matching + low overhead + explicit runtime stats
```

### 2. CET DSL

The DSL expresses attack-chain logic directly:

```text
AuthFail+, PrivEsc, DataAccess
```

Supported query concepts include:

```text
A+          one or more events
A?          optional event
A{m,n}      bounded repetition
(A|B)       alternatives
WHERE       payload predicates
ABSENT      negative / absence conditions
RELATIONS   same_user, same_host, same_session, parent_process, same_asset
```

Example query:

```json
{
  "query_id": "credential_access_to_data_exposure",
  "version": "v1",
  "pattern": "AuthFail+,PrivEsc,DataAccess",
  "within_ms": 1800000,
  "slide_ms": 300000,
  "relations": [
    {"field": "user_id", "op": "same"},
    {"field": "host_id", "op": "same"}
  ],
  "absence": [
    {"event_type": "MFAChallenge", "within_ms": 600000}
  ],
  "severity": "high"
}
```

### 3. Security Graph Builder

The graph builder converts normalized events into temporal and semantic edges:

```text
same_user
same_host
same_session
same_source_ip
same_cloud_account
same_process
same_asset
parent_process
```

This lets the runtime detect trends that are not merely sequential, but connected through security meaning.

### 4. Spark Declarative Pipelines / SDP

The SDP integration defines declarative tables and flows for canonical event preparation:

```text
source table → bronze events → expectations → silver events → CET event buffer
```

This provides a clean data plane for quality gates, lineage, replay readiness, and downstream CET execution.

### 5. ZeroBus Connector

The connector layer supports transport-pluggable ZeroBus ingestion:

```text
ZeroBus → connector → canonical security event → Delta / SDP → CET runtime
```

Initial transports include:

- local file / JSONL replay;
- HTTP / NDJSON-style polling;
- mock transport for tests;
- extension points for production ZeroBus protocols.

### 6. Standing Trend Runtime

The v7 runtime introduces mutable temporal graph state and standing trend queries.

<pre>
             ┌──────────────────────┐
 event ─────▶│ TemporalGraphState    │
             └──────────┬───────────┘
                        │ mutation log
                        ▼
             ┌──────────────────────┐
             │ StandingTrendRuntime  │
             └──────────┬───────────┘
                        │
          ┌─────────────┴─────────────┐
          ▼                           ▼
 positive_match                cancel_match
 trend emitted                 trend retracted
</pre>

This bridges historical lakehouse replay and live temporal graph intelligence.

### 7. Temporal Security Knowledge Graph

CET emits structured intelligence, not just alerts:

```text
entities
relations
trends
evidence paths
risk explanations
investigation prompts
```

The Temporal Security KG supports hunting, response, governance, executive reporting, and agentic investigation.

### 8. Counterfactual Replay

CET supports what-if detection engineering:

```text
“If this query existed last month, what would it have detected?”
“If I change the window from 30 minutes to 2 hours, what changes?”
“If this false positive is suppressed, which trends disappear?”
```

This makes detection development measurable instead of speculative.

---

## 0xDSI Use Cases

### Privilege Escalation to Sensitive Data Access

```text
AuthFail+ → PrivEsc → DataAccess
same_user + same_host
ABSENT MFAChallenge
```

Identify credential attacks that escalate into real access risk.

### Cloud Control Plane Abuse

```text
AssumeRole → PolicyChange → ObjectRead+ → ExternalTransfer
same_cloud_account + same_principal
```

Detect identity-to-cloud-resource abuse paths.

### Endpoint Process Chain Investigation

```text
ScriptExecution → ChildProcess → CredentialDump → NetworkConnection
parent_process + same_host
```

Convert endpoint telemetry into causal process graphs.

### Data Exfiltration Trend

```text
DataAccess+ → Compression → Upload
same_user + same_asset
WHERE data_sensitivity in ["pii", "secret", "source_code"]
```

Prioritize data movement that carries business impact.

### Insider Risk Trend

```text
AfterHoursLogin → BulkRead+ → PermissionChange? → ExternalShare
same_user
```

Detect behavior that only becomes suspicious as a full temporal trend.

### Detection Engineering Replay

```text
new_query_version → replay 30 days → compare old/new trend sets
```

Deploy detection logic with evidence, not guesswork.

---

## What Makes 0xDSI-CET Different

<pre>
                       ┌────────────────────────────┐
                       │       SECURITY-FIRST        │
                       └─────────────┬──────────────┘
                                     │
         ┌───────────────────────────┼───────────────────────────┐
         ▼                           ▼                           ▼
┌─────────────────┐        ┌─────────────────┐        ┌─────────────────┐
│  TEMPORAL GRAPH │        │  LAKEHOUSE CORE │        │  NATIVE ENGINE  │
│ standing trends │        │ Delta + replay  │        │ C + pthread     │
│ positive/cancel │        │ SDP + DAB       │        │ mmap workspace  │
└─────────────────┘        └─────────────────┘        └─────────────────┘
         │                           │                           │
         └───────────────────────────┼───────────────────────────┘
                                     ▼
                         ┌───────────────────────┐
                         │ AGENTIC INVESTIGATION │
                         │ why / evidence / next │
                         └───────────────────────┘
</pre>

0xDSI-CET is not a generic graph database. It is a domain-specific temporal security intelligence system for:

- real-time trend detection;
- historical replay;
- detection engineering governance;
- causal evidence generation;
- graph-based investigation;
- lakehouse-native security operations;
- agentic investigation workflows.

---

## The 0xDSI-CET Flywheel

<pre>
       ┌──────────────────┐
       │  DETECT TRENDS   │
       └────────┬─────────┘
                ▼
       ┌──────────────────┐
       │ EXPLAIN EVIDENCE │
       └────────┬─────────┘
                ▼
       ┌──────────────────┐
       │  REPLAY HISTORY  │
       └────────┬─────────┘
                ▼
       ┌──────────────────┐
       │ IMPROVE QUERIES  │
       └────────┬─────────┘
                ▼
       ┌──────────────────┐
       │  TRAIN AGENTS    │
       └────────┬─────────┘
                └─────────────── back to detection
</pre>

Every trend improves the next investigation. Every replay improves the next query. Every query version improves the security graph.

---

## Repository Map

```text
c_engine/                  Native C runtime: graph, DSL subset, H-CET, pthread/mmap
bindings/python/           Python bridge, DSL, graph builder, runtime, adapters
bindings/python/standing/  Standing Trend Runtime and sinks
bindings/python/connectors/SDP and ZeroBus connector interfaces
contracts/                 JSON contracts for events, trends, graph state, connectors
notebooks/                 Databricks, SDP, and standing runtime notebooks
jobs/                      Replay, recompute, state TTL, KG materialization, ZeroBus ingest
resources/                 Databricks Asset Bundle job definitions
ci/                        Regression, sanitizer, fuzz, v4/v5/v6/v7 validation
scripts/                   Wheel, SBOM, Databricks validate, standing runtime validation
docs/                      Production, SDP, ZeroBus, standing runtime, and operational docs
```

---

## Quick Start

### Build the native engine

```bash
cmake -S . -B build -DCET_ENABLE_PTHREADS=ON -DCET_ENABLE_MMAP_ARENA=ON
cmake --build build
ctest --test-dir build --output-on-failure
```

### Run regression checks

```bash
./ci/production_regression.sh
./ci/v4_offline_regression.sh
./ci/v5_main_ready_regression.sh
./ci/v6_sdp_zerobus_regression.sh
./ci/v7_standing_runtime_regression.sh
./ci/all_in_one_v7_regression.sh
```

### Build package artifacts

```bash
make wheel
make sbom
make main-ready
```

### Validate Databricks bundle

```bash
./scripts/databricks_validate.sh
```

### Deploy to Databricks

```bash
databricks bundle deploy -t dev
databricks bundle deploy -t prod
```

---

## Example: Standing Trend Runtime

```python
from bindings.python.dsl import parse_query_spec
from bindings.python.standing.runtime import StandingTrendRuntime, StandingTrendQuery

query = StandingTrendQuery.from_query_spec(parse_query_spec({
    "query_id": "privilege_escalation_to_data_access",
    "version": "v1",
    "pattern": "AuthFail+,PrivEsc,DataAccess",
    "within_ms": 1800000,
    "slide_ms": 300000,
    "relations": [{"field": "user_id", "op": "same"}],
    "severity": "high"
}))

runtime = StandingTrendRuntime([query])

for event in events:
    for result in runtime.ingest(event):
        print(result.event_type, result.trend_id, result.severity)
```

---

## Example: Attack Chain Query

```text
ATTACK_CHAIN privilege_escalation_to_exfiltration
MATCH
  AuthFail+ AS failed_auth
  THEN PrivEsc AS escalation
  THEN DataAccess+ AS sensitive_access
  THEN ExternalTransfer AS transfer
WHERE
  same_user(failed_auth, escalation, sensitive_access, transfer)
  same_host(escalation, sensitive_access)
ABSENT
  MFAChallenge BETWEEN failed_auth AND escalation
SCORE
  base 50
  + asset_criticality
  + data_sensitivity
  + abnormal_hour
```

This is the direction of 0xDSI-CET: attack-chain intelligence that can be compiled, replayed, scored, explained, and operationalized.

---

## Production Posture

The v7 stack includes the code paths needed for serious production readiness:

- deterministic trend IDs;
- replay and retraction paths;
- graph mutation logs;
- standing positive/cancel match events;
- Delta event buffers;
- Databricks Asset Bundles;
- Spark Declarative Pipelines integration;
- ZeroBus connector layer;
- native runtime diagnostics;
- metrics contracts;
- SBOM generation script;
- CI regression layers.

Before declaring a specific deployment production-ready, validate:

```text
[ ] Databricks workspace deployment
[ ] Unity Catalog grants and Volumes
[ ] production ZeroBus protocol adapter
[ ] SDP pipeline execution with production data
[ ] benchmark at 1M / 10M / 100M events
[ ] replay-storm drill
[ ] alert backend integration
[ ] signed artifacts and SBOM publication
[ ] disaster recovery runbook
```

---

## North Star

<pre>
┌──────────────────────────────────────────────────────────────────────────────┐
│                          TEMPORAL SECURITY INTELLIGENCE                      │
├──────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│  Events become graph state.                                                   │
│  Graph state becomes trends.                                                  │
│  Trends become evidence.                                                      │
│  Evidence becomes investigation.                                               │
│  Investigation becomes better detection.                                      │
│                                                                              │
└──────────────────────────────────────────────────────────────────────────────┘
</pre>

0xDSI-CET is built to become the temporal intelligence layer between telemetry, lakehouse analytics, graph reasoning, replay, and autonomous security investigation.
