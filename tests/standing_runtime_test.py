from bindings.python.query_registry import QueryRegistry
from bindings.python.standing import StandingTrendRuntime, MemoryTrendSink


def _event(eid, etype, ts, user="u1", host="h1"):
    return {
        "event_id": eid,
        "partition_key": user,
        "event_type": etype,
        "event_time_ms": ts,
        "event_time": "2026-05-31T00:00:00Z",
        "attributes": {"user_id": user, "host_id": host, "asset_criticality": 5},
    }


def test_standing_runtime_positive_and_cancel():
    registry = QueryRegistry.from_iterable([
        {
            "query_id": "q_attack",
            "version": "v1",
            "pattern": "AuthFail+,PrivEsc,DataAccess",
            "within_ms": 60000,
            "slide_ms": 10000,
            "severity": "high",
            "relations": [{"field": "user_id", "op": "same"}],
        }
    ])
    sink = MemoryTrendSink()
    runtime = StandingTrendRuntime(registry, sinks=[sink])
    assert runtime.process_event(_event(1, "AuthFail", 1)) == []
    assert runtime.process_event(_event(2, "PrivEsc", 2)) == []
    emitted = runtime.process_event(_event(3, "DataAccess", 3))
    positives = [e for e in emitted if e.event_type == "positive_match"]
    assert positives
    assert positives[0].trend["severity"] in {"high", "critical"}
    cancelled = runtime.remove_event(3)
    assert any(e.event_type == "cancel_match" for e in cancelled)
    assert len(sink.events) >= 2


if __name__ == "__main__":
    test_standing_runtime_positive_and_cancel()
    print("standing runtime ok")
