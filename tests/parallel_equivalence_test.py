from bindings.python.bridge import CETBridge


def _norm(paths):
    return sorted(tuple(int(x) for x in p) for p in paths)


def test_parallel_matches_single_thread_and_reports_runtime():
    b = CETBridge('build/liboxdsi_cet.so')
    q = b.parse_query('parallel_equiv', 'A+,B,C', 60000, 10000)
    events = [
        (1, 'p', 'A', 1),
        (2, 'p', 'X', 2),
        (3, 'p', 'A', 3),
        (4, 'p', 'B', 4),
        (5, 'p', 'C', 5),
        (6, 'p', 'A', 6),
        (7, 'p', 'B', 7),
        (8, 'p', 'C', 8),
    ]
    edges = [(events[i-1][0], events[i][0], 0, 100) for i in range(1, len(events))]

    single = b.run_hcet(q, events, edges, switch_depth=2)
    parallel = b.run_hcet_parallel(
        q,
        events,
        edges,
        switch_depth=2,
        native_threads=2,
        enable_mmap_arena=True,
    )

    assert _norm(single.paths) == _norm(parallel.paths)
    assert parallel.stats.get('runtime', {}).get('native_threads_used', 0) >= 1
    assert 'runtime' in parallel.stats


if __name__ == '__main__':
    test_parallel_matches_single_thread_and_reports_runtime()
    print('parallel equivalence ok')
