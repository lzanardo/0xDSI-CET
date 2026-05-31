from bindings.python.query_registry import QueryRegistry
from bindings.python.standing import StandingTrendRuntime


def ev(eid, typ, ts, user):
    return {"event_id": eid, "partition_key": user, "event_type": typ, "event_time_ms": ts, "attributes": {"user_id": user}}


def test_relation_constraint_same_user():
    registry = QueryRegistry.from_iterable([
        {"query_id": "same_user", "version": "v1", "pattern": "AuthFail,PrivEsc", "within_ms": 1000, "slide_ms": 100, "relations": [{"field": "user_id", "op": "same"}]}
    ])
    rt = StandingTrendRuntime(registry)
    assert rt.process_event(ev(1, "AuthFail", 1, "u1")) == []
    # Different partition/user should not complete the same-user query.
    assert rt.process_event(ev(2, "PrivEsc", 2, "u2")) == []
    emitted = rt.process_event(ev(3, "PrivEsc", 3, "u1"))
    assert any(e.event_type == "positive_match" and e.path == (1, 3) for e in emitted)


if __name__ == "__main__":
    test_relation_constraint_same_user()
    print("relation-aware standing runtime ok")
