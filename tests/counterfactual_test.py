from bindings.python.counterfactual import diff_trend_sets


def test_counterfactual_diff():
    diff = diff_trend_sets([{"trend_id":"a"}], [{"trend_id":"a"},{"trend_id":"b"}])
    assert diff.summary == {"added":1,"removed":0,"common":1}


if __name__ == "__main__":
    test_counterfactual_diff(); print("counterfactual ok")
