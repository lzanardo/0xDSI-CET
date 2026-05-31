from bindings.python.dsl import parse_pattern, parse_query_spec, validate_path


def test_parse_extended_pattern():
    steps = parse_pattern("AuthFail+,PrivEsc?,(DataAccess|Download){1,2}")
    assert steps[0].kleene_plus
    assert steps[1].optional
    assert steps[2].has_or
    assert steps[2].bounded


def test_query_native_pattern_and_features():
    q = parse_query_spec({
        "query_id": "q",
        "version": "v4",
        "pattern": "A+,B,C",
        "within_ms": 1000,
        "slide_ms": 100,
        "absence": [{"event_type": "D"}],
        "relations": [{"field": "user_id"}],
    })
    assert q.native_pattern() == "A+,B,C"
    assert {"absence", "relations"}.issubset(q.unsupported_native_features())


def test_absence_validation_rejects_path():
    q = parse_query_spec({"query_id":"q","pattern":"A,B","within_ms":1000,"absence":[{"event_type":"D"}]})
    path = [{"event_id":1,"partition_key":"p","event_type":"A","event_time_ms":1},{"event_id":2,"partition_key":"p","event_type":"B","event_time_ms":3}]
    all_events = path + [{"event_id":9,"partition_key":"p","event_type":"D","event_time_ms":2}]
    assert not validate_path(q, path, all_events)


if __name__ == "__main__":
    test_parse_extended_pattern(); test_query_native_pattern_and_features(); test_absence_validation_rejects_path(); print("dsl compiler ok")
