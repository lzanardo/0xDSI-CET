from bindings.python.bridge import CETMatch
from bindings.python.multi_query_runtime import CETRuntimeV4, RuntimeOptions
from bindings.python.query_registry import QueryRegistry


class FakeBridge:
    def parse_query(self, name, seq_csv, within_ms, slide_ms):
        assert seq_csv == "AuthFail+,PrivEsc,DataAccess"
        return seq_csv
    def run_hcet(self, query, events, edges, **kwargs):
        return CETMatch(paths=[[1,2,3]], stats={"overflow": False})


def test_multi_query_runtime_emits_enriched_trend():
    registry = QueryRegistry.from_iterable([{
        "query_id":"security_escalation",
        "version":"v4",
        "pattern":"AuthFail+,PrivEsc,DataAccess",
        "within_ms":10000,
        "slide_ms":1000,
        "severity":"high",
        "relations":[{"field":"user_id","op":"same"}],
    }])
    events = [
        {"event_id":1,"partition_key":"u1","event_type":"AuthFail","event_time_ms":1,"attributes":{"user_id":"u1","asset_criticality":5}},
        {"event_id":2,"partition_key":"u1","event_type":"PrivEsc","event_time_ms":2,"attributes":{"user_id":"u1"}},
        {"event_id":3,"partition_key":"u1","event_type":"DataAccess","event_time_ms":3,"attributes":{"user_id":"u1"}},
    ]
    trends = CETRuntimeV4(registry, bridge=FakeBridge(), options=RuntimeOptions()).run(events)
    assert len(trends) == 1
    assert trends[0]["query_id"] == "security_escalation"
    assert trends[0]["risk_score"] >= 75


if __name__ == "__main__":
    test_multi_query_runtime_emits_enriched_trend(); print("multi query runtime ok")
