from bindings.python.security_graph import SecurityGraphBuilder


def test_security_graph_builder_adds_temporal_and_entity_edges():
    events = [
        {"event_id":1,"partition_key":"u1","event_type":"AuthFail","event_time_ms":1,"attributes":{"user_id":"u1","host_id":"h1"}},
        {"event_id":2,"partition_key":"u1","event_type":"PrivEsc","event_time_ms":2,"attributes":{"user_id":"u1","host_id":"h1"}},
        {"event_id":3,"partition_key":"u1","event_type":"DataAccess","event_time_ms":3,"attributes":{"user_id":"u1","host_id":"h1"}},
    ]
    graph = SecurityGraphBuilder().build(events)
    relations = {m.relation for m in graph.edge_metadata}
    assert "temporal_next" in relations
    assert "same_user" in relations
    assert len(graph.native_events) == 3
    assert graph.native_edges


if __name__ == "__main__":
    test_security_graph_builder_adds_temporal_and_entity_edges(); print("security graph builder ok")
