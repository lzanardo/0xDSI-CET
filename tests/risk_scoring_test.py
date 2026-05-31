from bindings.python.dsl import parse_query_spec
from bindings.python.risk import score_trend


def test_risk_scoring_bounds_and_severity():
    q = parse_query_spec({"query_id":"q","pattern":"A,B","within_ms":100,"severity":"critical","score":{"boost":50}})
    out = score_trend(q, [{"attributes":{"asset_criticality":20}}, {"attributes":{}}], {})
    assert out["risk_score"] == 100.0
    assert out["severity"] == "critical"


if __name__ == "__main__":
    test_risk_scoring_bounds_and_severity(); print("risk scoring ok")
