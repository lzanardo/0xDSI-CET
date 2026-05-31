from __future__ import annotations

import argparse
from time import perf_counter
from bindings.python.bridge import CETBridge


def build_workload(n: int):
    events = []
    edges = []
    types = ['A', 'B', 'C', 'X']
    for i in range(n):
        events.append((i + 1, 'acct', types[i % len(types)], i + 1))
        if i > 0:
            edges.append((i, i + 1, 0, n + 1))
    return events, edges


def run(n: int, threads: int, mmap_workspace_bytes: int):
    b = CETBridge('build/liboxdsi_cet.so')
    q = b.parse_query('parallel_bench', 'A,B,C', max(60000, n + 1), 10000)
    events, edges = build_workload(n)
    t0 = perf_counter()
    out = b.run_hcet_parallel(
        q,
        events,
        edges,
        switch_depth=2,
        native_threads=threads,
        enable_mmap_arena=True,
        mmap_workspace_bytes=mmap_workspace_bytes,
    )
    dt = perf_counter() - t0
    return len(out.paths), dt, out.stats


if __name__ == '__main__':
    ap = argparse.ArgumentParser()
    ap.add_argument('--events', type=int, default=20000)
    ap.add_argument('--threads', type=int, default=1)
    ap.add_argument('--mmap-workspace-bytes', type=int, default=64 * 1024 * 1024)
    args = ap.parse_args()
    paths, seconds, stats = run(args.events, args.threads, args.mmap_workspace_bytes)
    runtime = stats.get('runtime', {})
    print(
        f"events={args.events} threads={args.threads} paths={paths} seconds={seconds:.6f} "
        f"threads_used={runtime.get('native_threads_used', 0)} "
        f"parallel={runtime.get('parallel_enabled', False)} "
        f"mmap={runtime.get('used_mmap_arena', False)} "
        f"workspace_bytes={runtime.get('workspace_bytes', 0)}"
    )
