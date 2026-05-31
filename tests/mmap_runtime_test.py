from bindings.python.bridge import CETBridge


def test_parallel_mmap_workspace_reports_runtime_stats():
    b = CETBridge('build/liboxdsi_cet.so')
    q = b.parse_query('mmap_runtime', 'A,B,C', 60000, 10000)
    events = [(1, 'p', 'A', 1), (2, 'p', 'B', 2), (3, 'p', 'C', 3), (4, 'p', 'A', 4), (5, 'p', 'B', 5), (6, 'p', 'C', 6)]
    edges = [(events[i - 1][0], events[i][0], 0, 100) for i in range(1, len(events))]
    out = b.run_hcet_parallel(q, events, edges, switch_depth=2, native_threads=2, enable_mmap_arena=True, mmap_workspace_bytes=64 * 1024 * 1024)
    assert out.paths
    runtime = out.stats.get('runtime', {})
    assert runtime.get('parallel_enabled') is True
    assert runtime.get('used_mmap_arena') is True
    assert runtime.get('workspace_bytes', 0) > 0


if __name__ == '__main__':
    test_parallel_mmap_workspace_reports_runtime_stats()
    print('mmap runtime ok')
