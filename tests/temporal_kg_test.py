from bindings.python.temporal_kg import build_temporal_kg


def test_temporal_kg_materializes_entities_and_evidence():
    events = [
        {"event_id":1,"event_type":"AuthFail","event_time_ms":1,"attributes":{"user_id":"u1","host_id":"h1"}},
        {"event_id":2,"event_type":"PrivEsc","event_time_ms":2,"attributes":{"user_id":"u1","host_id":"h1"}},
    ]
    trends = [{"trend_id":"t1","query_id":"q","path":[1,2],"risk_score":90}]
    kg = build_temporal_kg(events, trends)
    assert len(kg.entities) >= 2
    assert len(kg.trend_evidence) == 2


if __name__ == "__main__":
    test_temporal_kg_materializes_entities_and_evidence(); print("temporal kg ok")
