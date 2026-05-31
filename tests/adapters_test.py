from bindings.python.adapters.ecs import normalize_ecs_event
from bindings.python.adapters.ocsf import normalize_ocsf_event
from bindings.python.adapters.sigma import sigma_like_to_query


def test_ecs_adapter():
    e = normalize_ecs_event({"event":{"id":1,"action":"AuthFail"},"@timestamp":"2026-01-01T00:00:00Z","user":{"name":"u"}})
    assert e["event_type"] == "AuthFail"
    assert e["partition_key"] == "u"


def test_ocsf_adapter():
    e = normalize_ocsf_event({"event_id":2,"class_name":"PrivEsc","time":3,"actor":{"user":{"uid":"u"}}})
    assert e["event_type"] == "PrivEsc"


def test_sigma_adapter():
    q = sigma_like_to_query({"id":"r1","sequence":["A","B"],"timeframe_ms":1000})
    assert q["pattern"] == "A,B"


if __name__ == "__main__":
    test_ecs_adapter(); test_ocsf_adapter(); test_sigma_adapter(); print("adapters ok")
