"""Workflow-level replay flow smoke (logical assertions only)."""
from pathlib import Path

def test_replay_assets_exist():
    assert Path('jobs/late_event_replay.py').exists()
    assert Path('jobs/recompute_trends.py').exists()
    assert Path('jobs/retract_and_upsert.sql').exists()

if __name__=='__main__':
    test_replay_assets_exist()
    print('e2e replay asset smoke ok')
