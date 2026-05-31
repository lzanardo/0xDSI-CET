from pathlib import Path
import json
import tempfile

from bindings.python.connectors.zerobus import ZeroBusConfig, ZeroBusClient, zero_bus_events


def test_zerobus_file_transport_normalizes_events():
    with tempfile.TemporaryDirectory() as td:
        path = Path(td) / "events.jsonl"
        path.write_text(json.dumps({"event_id": 1, "event_type": "AuthFail", "event_time_ms": 10, "user_id": "u1"}) + "\n", encoding="utf-8")
        cfg = ZeroBusConfig(source_name="zerobus-test", transport="file", file_path=str(path), target_table="main.cet.cet_events")
        events = list(ZeroBusClient(cfg).poll_normalized())
        assert events[0]["event_id"] == 1
        assert events[0]["event_type"] == "AuthFail"
        assert events[0]["partition_key"] == "u1"
        assert events[0]["source_name"] == "zerobus-test"


def test_zerobus_mock_helper():
    events = zero_bus_events(
        {"transport": "mock", "source_name": "zerobus", "target_table": "main.cet.cet_events"},
        mock_events=[{"id": 2, "type": "PrivEsc", "timestamp_ms": 20, "host_id": "h1"}],
    )
    assert len(events) == 1
    assert events[0]["event_id"] == 2
    assert events[0]["partition_key"] == "h1"
