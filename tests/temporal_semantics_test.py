from bindings.python.bridge import CETBridge


def test_out_of_order_edges_are_rejected():
    b = CETBridge("build/liboxdsi_cet.so")
    q = b.parse_query("q", "A,B", 1000, 100)
    events = [(1, "p", "A", 100), (2, "p", "B", 50)]
    edges = [(1, 2, 0, 200)]
    out = b.run_hcet(q, events, edges)
    assert out.paths == []
    assert out.stats.get("temporal_rejects", 0) >= 1


def test_edge_window_is_enforced_when_present():
    b = CETBridge("build/liboxdsi_cet.so")
    q = b.parse_query("q", "A,B", 1000, 100)
    events = [(1, "p", "A", 10), (2, "p", "B", 20)]
    edges = [(1, 2, 100, 200)]
    out = b.run_hcet(q, events, edges)
    assert out.paths == []
    assert out.stats.get("edge_window_rejects", 0) >= 1


if __name__ == "__main__":
    test_out_of_order_edges_are_rejected()
    test_edge_window_is_enforced_when_present()
    print("temporal semantics ok")
