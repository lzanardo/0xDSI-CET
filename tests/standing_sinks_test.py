import json
from pathlib import Path
from bindings.python.standing import StandingTrendEvent, MemoryTrendSink, JsonlTrendSink


def sample_event():
    return StandingTrendEvent(
        event_type="positive_match",
        trend_id="t1",
        query_id="q1",
        query_version="v1",
        partition_key="p1",
        path=(1, 2, 3),
        revision=7,
        trend={"risk_score": 80.0},
    )


def test_memory_and_jsonl_sinks(tmp_path=None):
    event = sample_event()
    mem = MemoryTrendSink()
    mem.emit(event)
    assert mem.as_dicts()[0]["trend_id"] == "t1"
    path = Path(tmp_path or "/tmp") / "standing_sink_test.jsonl"
    if path.exists():
        path.unlink()
    js = JsonlTrendSink(str(path))
    js.emit(event)
    row = json.loads(path.read_text().strip())
    assert row["path"] == [1, 2, 3]


if __name__ == "__main__":
    test_memory_and_jsonl_sinks()
    print("standing sinks ok")
