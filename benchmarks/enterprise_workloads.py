from __future__ import annotations

import argparse
import time

from bindings.python.query_registry import QueryRegistry
from bindings.python.multi_query_runtime import CETRuntimeV4, RuntimeOptions
from bindings.python.bridge import CETMatch


class FakeBridge:
    def parse_query(self, name, seq_csv, within_ms, slide_ms):
        return {"name": name, "seq_csv": seq_csv, "within_ms": within_ms, "slide_ms": slide_ms}

    def run_hcet(self, query, events, edges, **kwargs):
        by_partition = {}
        for eid, pkey, etype, ts in sorted(events, key=lambda x: (x[1], x[3], x[0])):
            by_partition.setdefault(pkey, {}).setdefault(etype, []).append(eid)
        paths = []
        for typed in by_partition.values():
            if {"AuthFail", "PrivEsc", "DataAccess"}.issubset(typed):
                paths.append([typed["AuthFail"][0], typed["PrivEsc"][0], typed["DataAccess"][0]])
                if len(paths) >= 100:
                    break
        return CETMatch(paths=paths, stats={"fake": True})


def make_events(n: int):
    types = ["AuthFail", "PrivEsc", "DataAccess", "Noise", "MFAChallengeFailed"]
    users = 100
    for i in range(1, n + 1):
        user = i % users
        phase = (i // users) % len(types)
        t = types[phase]
        yield {
            "event_id": i,
            "partition_key": f"user-{user}",
            "event_type": t,
            "event_time_ms": i * 1000,
            "attributes": {
                "user_id": f"user-{user}",
                "host_id": f"host-{user % 25}",
                "session_id": f"sess-{i // 10}",
                "asset_criticality": float(i % 10),
            },
        }


def main():
    p = argparse.ArgumentParser()
    p.add_argument("--events", type=int, default=5000)
    p.add_argument("--queries", type=int, default=2)
    p.add_argument("--offline-only", action="store_true")
    args = p.parse_args()
    queries = []
    for i in range(args.queries):
        queries.append({
            "query_id": f"q{i}",
            "version": "v4",
            "pattern": "AuthFail+,PrivEsc,DataAccess",
            "within_ms": 60 * 60 * 1000,
            "slide_ms": 5 * 60 * 1000,
            "severity": "high",
            "relations": [{"field": "user_id", "op": "same"}],
        })
    runtime = CETRuntimeV4(QueryRegistry.from_iterable(queries), bridge=FakeBridge(), options=RuntimeOptions())
    events = list(make_events(args.events))
    t0 = time.perf_counter()
    trends = runtime.run(events)
    dt = time.perf_counter() - t0
    print(f"events={args.events} queries={args.queries} trends={len(trends)} seconds={dt:.6f}")


if __name__ == "__main__":
    main()
