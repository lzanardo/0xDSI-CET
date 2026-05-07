from time import perf_counter
from bindings.python.bridge import CETBridge


def run(mode="h-cet", n=5000):
    bridge = CETBridge("build/liboxdsi_cet.so")
    q = bridge.parse_query("bench", "A,B,C", 60000, 10000)
    events = []
    edges = []
    for i in range(n):
        t = ["A", "B", "C"][i % 3]
        events.append((i + 1, "acct", t, i + 1))
        if i > 0:
            edges.append((i, i + 1, 0, n + 1))
    t0 = perf_counter()
    out = bridge.run_hcet(q, events, edges)
    dt = perf_counter() - t0
    return len(out.paths), dt


if __name__ == "__main__":
    paths, seconds = run()
    print(f"paths={paths} seconds={seconds:.6f}")
