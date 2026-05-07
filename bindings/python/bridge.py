"""Production-grade ctypes bridge for liboxdsi_cet.so."""
from __future__ import annotations

import ctypes
import hashlib
import os
from dataclasses import dataclass
from pathlib import Path

EXPECTED_SHA256 = os.getenv("OXDSI_CET_SHA256", "")


def _verify_lib(path: str) -> None:
    if not EXPECTED_SHA256:
        return
    h = hashlib.sha256(Path(path).read_bytes()).hexdigest()
    if h != EXPECTED_SHA256:
        raise RuntimeError("Library checksum verification failed")


class CETEventType(ctypes.Structure):
    _fields_ = [("name", ctypes.c_char * 32), ("kleene_plus", ctypes.c_int), ("predicate", ctypes.c_void_p), ("predicate_ctx", ctypes.c_void_p)]


class CETQuery(ctypes.Structure):
    _fields_ = [("name", ctypes.c_char * 64), ("seq", CETEventType * 16), ("seq_len", ctypes.c_size_t), ("within_ms", ctypes.c_int64), ("slide_ms", ctypes.c_int64), ("skip_till_any_match", ctypes.c_int)]


class CETVertex(ctypes.Structure):
    _fields_ = [("id", ctypes.c_int), ("partition_key", ctypes.c_char * 64), ("event_type", ctypes.c_char * 32), ("event_time_ms", ctypes.c_int64)]


class CETEdge(ctypes.Structure):
    _fields_ = [("src", ctypes.c_int), ("dst", ctypes.c_int), ("window_start_ms", ctypes.c_int64), ("window_end_ms", ctypes.c_int64)]


class CETGraph(ctypes.Structure):
    _fields_ = [("vertices", CETVertex * 200000), ("edges", CETEdge * 1000000), ("vcount", ctypes.c_size_t), ("ecount", ctypes.c_size_t)]


class CETResult(ctypes.Structure):
    _fields_ = [("paths", (ctypes.c_int * 64) * 100000), ("path_len", ctypes.c_size_t * 100000), ("count", ctypes.c_size_t)]


@dataclass
class CETMatch:
    paths: list[list[int]]


class CETBridge:
    def __init__(self, lib_path: str | None = None):
        if lib_path is None:
            lib_path = str(Path(__file__).resolve().parents[2] / "build" / "liboxdsi_cet.so")
        _verify_lib(lib_path)
        self.lib = ctypes.CDLL(lib_path)
        self._bind()

    def _bind(self) -> None:
        self.lib.cet_parse_query.argtypes = [ctypes.c_char_p, ctypes.c_char_p, ctypes.c_int64, ctypes.c_int64, ctypes.POINTER(CETQuery)]
        self.lib.cet_parse_query.restype = ctypes.c_int
        self.lib.cet_graph_init.argtypes = [ctypes.POINTER(CETGraph)]
        self.lib.cet_graph_add_vertex.argtypes = [ctypes.POINTER(CETGraph), ctypes.c_int, ctypes.c_char_p, ctypes.c_char_p, ctypes.c_int64]
        self.lib.cet_graph_add_edge.argtypes = [ctypes.POINTER(CETGraph), ctypes.c_int, ctypes.c_int, ctypes.c_int64, ctypes.c_int64]
        self.lib.cet_execute_hcet.argtypes = [ctypes.POINTER(CETGraph), ctypes.POINTER(CETQuery), ctypes.c_size_t, ctypes.POINTER(CETResult)]
        self.lib.cet_set_cost_coefficients.argtypes = [ctypes.c_double, ctypes.c_double, ctypes.c_double, ctypes.c_double]

    def parse_query(self, name: str, seq_csv: str, within_ms: int, slide_ms: int) -> CETQuery:
        q = CETQuery()
        rc = self.lib.cet_parse_query(name.encode(), seq_csv.encode(), within_ms, slide_ms, ctypes.byref(q))
        if rc != 0:
            raise RuntimeError("cet_parse_query failed")
        return q

    def run_hcet(self, query: CETQuery, events: list[tuple[int, str, str, int]], edges: list[tuple[int, int, int, int]], switch_depth: int = 2) -> CETMatch:
        g = CETGraph()
        self.lib.cet_graph_init(ctypes.byref(g))
        for eid, pkey, etype, ts in events:
            self.lib.cet_graph_add_vertex(ctypes.byref(g), eid, pkey.encode(), etype.encode(), ts)
        for src, dst, ws, we in edges:
            self.lib.cet_graph_add_edge(ctypes.byref(g), src, dst, ws, we)
        out = CETResult()
        self.lib.cet_execute_hcet(ctypes.byref(g), ctypes.byref(query), switch_depth, ctypes.byref(out))
        return CETMatch(paths=[[out.paths[i][j] for j in range(out.path_len[i])] for i in range(out.count)])

    def set_cost_coefficients(self, mem_vertex: float, mem_edge: float, cpu_edge: float, cpu_vertex: float) -> None:
        self.lib.cet_set_cost_coefficients(mem_vertex, mem_edge, cpu_edge, cpu_vertex)
