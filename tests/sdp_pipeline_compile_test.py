from pathlib import Path


def test_sdp_pipeline_source_uses_declarative_pipelines():
    src = Path("notebooks/0xDSI_CET_SDP.py").read_text(encoding="utf-8")
    assert "from pyspark import pipelines as dp" in src
    assert "@dp.table" in src
    assert "dp.create_streaming_table" in src
    assert "@dp.append_flow" in src
    assert "@dp.materialized_view" in src
    assert "@dp.expect_or_drop" in src
