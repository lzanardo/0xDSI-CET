from bindings.python.investigation import build_investigation_package


def test_investigation_package_contains_questions():
    trend = {"trend_id":"t","query_id":"q","path":[1],"risk_score":80,"severity":"high"}
    events = {1:{"event_id":1,"event_type":"PrivEsc","event_time_ms":10,"partition_key":"p"}}
    pkg = build_investigation_package(trend, events, mitre_map={"q":["T1078"]})
    assert pkg["timeline"][0]["event_type"] == "PrivEsc"
    assert pkg["mitre_techniques"] == ["T1078"]
    assert pkg["next_best_questions"]


if __name__ == "__main__":
    test_investigation_package_contains_questions(); print("investigation api ok")
