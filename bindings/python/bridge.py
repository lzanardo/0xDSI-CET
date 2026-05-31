"""ctypes bridge for liboxdsi_cet.so with production diagnostics and native runtime v3.

The bridge remains backward compatible with the original C API, while exposing:
- checksum validation,
- explicit graph input bounds checks,
- execution stats / overflow reporting,
- optional pthread-backed parallel H-CET,
- optional mmap-backed runtime workspace.
"""
from __future__ import annotations

import ctypes
import hashlib
import os
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

EXPECTED_SHA256 = os.getenv("OXDSI_CET_SHA256", "")

CET_MAX_EVENTS = 200_000
CET_MAX_EDGES = 1_000_000
CET_MAX_PATHS = 100_000
CET_MAX_PATH_LEN = 64
CET_MAX_ERROR_LEN = 256
CET_RUNTIME_CONFIG_VERSION = 1


def _verify_lib(path: str) -> None:
    if not EXPECTED_SHA256:
        return
    h = hashlib.sha256(Path(path).read_bytes()).hexdigest()
    if h != EXPECTED_SHA256:
        raise RuntimeError("Library checksum verification failed")


def _env_bool(name: str, default: bool = False) -> bool:
    raw = os.getenv(name)
    if raw is None:
        return default
    return raw.strip().lower() in {"1", "true", "yes", "on"}


class CETEventType(ctypes.Structure):
    _fields_ = [
        ("name", ctypes.c_char * 32),
        ("kleene_plus", ctypes.c_int),
        ("predicate", ctypes.c_void_p),
        ("predicate_ctx", ctypes.c_void_p),
    ]


class CETQuery(ctypes.Structure):
    _fields_ = [
        ("name", ctypes.c_char * 64),
        ("seq", CETEventType * 16),
        ("seq_len", ctypes.c_size_t),
        ("within_ms", ctypes.c_int64),
        ("slide_ms", ctypes.c_int64),
        ("skip_till_any_match", ctypes.c_int),
    ]


class CETVertex(ctypes.Structure):
    _fields_ = [
        ("id", ctypes.c_int),
        ("partition_key", ctypes.c_char * 64),
        ("event_type", ctypes.c_char * 32),
        ("event_time_ms", ctypes.c_int64),
    ]


class CETEdge(ctypes.Structure):
    _fields_ = [
        ("src", ctypes.c_int),
        ("dst", ctypes.c_int),
        ("window_start_ms", ctypes.c_int64),
        ("window_end_ms", ctypes.c_int64),
    ]


class CETGraph(ctypes.Structure):
    _fields_ = [
        ("vertices", CETVertex * CET_MAX_EVENTS),
        ("edges", CETEdge * CET_MAX_EDGES),
        ("vcount", ctypes.c_size_t),
        ("ecount", ctypes.c_size_t),
    ]


class CETResult(ctypes.Structure):
    _fields_ = [
        ("paths", (ctypes.c_int * CET_MAX_PATH_LEN) * CET_MAX_PATHS),
        ("path_len", ctypes.c_size_t * CET_MAX_PATHS),
        ("count", ctypes.c_size_t),
    ]


class CETExecStats(ctypes.Structure):
    _fields_ = [
        ("paths_emitted", ctypes.c_size_t),
        ("paths_truncated", ctypes.c_size_t),
        ("states_enqueued", ctypes.c_size_t),
        ("states_truncated", ctypes.c_size_t),
        ("seed_paths", ctypes.c_size_t),
        ("max_depth_seen", ctypes.c_size_t),
        ("temporal_rejects", ctypes.c_size_t),
        ("edge_window_rejects", ctypes.c_size_t),
        ("predicate_rejects", ctypes.c_size_t),
        ("overflow", ctypes.c_int),
        ("error", ctypes.c_char * CET_MAX_ERROR_LEN),
    ]

    def as_dict(self) -> dict[str, Any]:
        return {
            "paths_emitted": int(self.paths_emitted),
            "paths_truncated": int(self.paths_truncated),
            "states_enqueued": int(self.states_enqueued),
            "states_truncated": int(self.states_truncated),
            "seed_paths": int(self.seed_paths),
            "max_depth_seen": int(self.max_depth_seen),
            "temporal_rejects": int(self.temporal_rejects),
            "edge_window_rejects": int(self.edge_window_rejects),
            "predicate_rejects": int(self.predicate_rejects),
            "overflow": bool(self.overflow),
            "error": self.error.split(b"\0", 1)[0].decode("utf-8", errors="replace"),
        }


class CETRuntimeConfig(ctypes.Structure):
    _fields_ = [
        ("version", ctypes.c_uint32),
        ("native_threads", ctypes.c_size_t),
        ("enable_mmap_arena", ctypes.c_int),
        ("mmap_workspace_bytes", ctypes.c_size_t),
        ("madvise_hugepage", ctypes.c_int),
        ("deterministic_merge", ctypes.c_int),
    ]


class CETRuntimeStats(ctypes.Structure):
    _fields_ = [
        ("native_threads_requested", ctypes.c_size_t),
        ("native_threads_used", ctypes.c_size_t),
        ("workspace_bytes", ctypes.c_size_t),
        ("used_mmap_arena", ctypes.c_int),
        ("parallel_enabled", ctypes.c_int),
        ("error", ctypes.c_char * CET_MAX_ERROR_LEN),
    ]

    def as_dict(self) -> dict[str, Any]:
        return {
            "native_threads_requested": int(self.native_threads_requested),
            "native_threads_used": int(self.native_threads_used),
            "workspace_bytes": int(self.workspace_bytes),
            "used_mmap_arena": bool(self.used_mmap_arena),
            "parallel_enabled": bool(self.parallel_enabled),
            "error": self.error.split(b"\0", 1)[0].decode("utf-8", errors="replace"),
        }


@dataclass
class CETMatch:
    paths: list[list[int]]
    stats: dict[str, Any] = field(default_factory=dict)


class CETBridge:
    def __init__(self, lib_path: str | None = None):
        if lib_path is None:
            lib_path = str(Path(__file__).resolve().parents[2] / "build" / "liboxdsi_cet.so")
        _verify_lib(lib_path)
        self.lib_path = lib_path
        self.lib = ctypes.CDLL(lib_path)
        self.has_extended_exec = False
        self.has_parallel_exec = False
        self.has_runtime_defaults = False
        self._bind()

    def _bind(self) -> None:
        self.lib.cet_parse_query.argtypes = [
            ctypes.c_char_p,
            ctypes.c_char_p,
            ctypes.c_int64,
            ctypes.c_int64,
            ctypes.POINTER(CETQuery),
        ]
        self.lib.cet_parse_query.restype = ctypes.c_int

        self.lib.cet_graph_init.argtypes = [ctypes.POINTER(CETGraph)]
        self.lib.cet_graph_init.restype = None

        self.lib.cet_graph_add_vertex.argtypes = [
            ctypes.POINTER(CETGraph),
            ctypes.c_int,
            ctypes.c_char_p,
            ctypes.c_char_p,
            ctypes.c_int64,
        ]
        self.lib.cet_graph_add_vertex.restype = ctypes.c_int

        self.lib.cet_graph_add_edge.argtypes = [
            ctypes.POINTER(CETGraph),
            ctypes.c_int,
            ctypes.c_int,
            ctypes.c_int64,
            ctypes.c_int64,
        ]
        self.lib.cet_graph_add_edge.restype = ctypes.c_int

        self.lib.cet_execute_hcet.argtypes = [
            ctypes.POINTER(CETGraph),
            ctypes.POINTER(CETQuery),
            ctypes.c_size_t,
            ctypes.POINTER(CETResult),
        ]
        self.lib.cet_execute_hcet.restype = None

        self.lib.cet_set_cost_coefficients.argtypes = [
            ctypes.c_double,
            ctypes.c_double,
            ctypes.c_double,
            ctypes.c_double,
        ]
        self.lib.cet_set_cost_coefficients.restype = None

        try:
            self.lib.cet_execute_hcet_ex.argtypes = [
                ctypes.POINTER(CETGraph),
                ctypes.POINTER(CETQuery),
                ctypes.c_size_t,
                ctypes.POINTER(CETResult),
                ctypes.POINTER(CETExecStats),
            ]
            self.lib.cet_execute_hcet_ex.restype = None
            self.has_extended_exec = True
        except AttributeError:
            self.has_extended_exec = False

        try:
            self.lib.cet_runtime_config_default.argtypes = [ctypes.POINTER(CETRuntimeConfig)]
            self.lib.cet_runtime_config_default.restype = None
            self.has_runtime_defaults = True
        except AttributeError:
            self.has_runtime_defaults = False

        try:
            self.lib.cet_execute_hcet_parallel_ex.argtypes = [
                ctypes.POINTER(CETGraph),
                ctypes.POINTER(CETQuery),
                ctypes.c_size_t,
                ctypes.POINTER(CETRuntimeConfig),
                ctypes.POINTER(CETResult),
                ctypes.POINTER(CETExecStats),
                ctypes.POINTER(CETRuntimeStats),
            ]
            self.lib.cet_execute_hcet_parallel_ex.restype = ctypes.c_int
            self.has_parallel_exec = True
        except AttributeError:
            self.has_parallel_exec = False

    def parse_query(self, name: str, seq_csv: str, within_ms: int, slide_ms: int) -> CETQuery:
        q = CETQuery()
        rc = self.lib.cet_parse_query(
            name.encode("utf-8"),
            seq_csv.encode("utf-8"),
            int(within_ms),
            int(slide_ms),
            ctypes.byref(q),
        )
        if rc != 0:
            raise RuntimeError("cet_parse_query failed")
        return q

    def _build_graph(
        self,
        events: list[tuple[int, str, str, int]],
        edges: list[tuple[int, int, int, int]],
    ) -> CETGraph:
        if len(events) > CET_MAX_EVENTS:
            raise OverflowError(f"events exceeds CET_MAX_EVENTS={CET_MAX_EVENTS}: {len(events)}")
        if len(edges) > CET_MAX_EDGES:
            raise OverflowError(f"edges exceeds CET_MAX_EDGES={CET_MAX_EDGES}: {len(edges)}")

        g = CETGraph()
        self.lib.cet_graph_init(ctypes.byref(g))

        for eid, pkey, etype, ts in events:
            rc = self.lib.cet_graph_add_vertex(
                ctypes.byref(g),
                int(eid),
                str(pkey).encode("utf-8")[:63],
                str(etype).encode("utf-8")[:31],
                int(ts),
            )
            if rc != 0:
                raise OverflowError("cet_graph_add_vertex failed, likely CET_MAX_EVENTS reached")

        for src, dst, ws, we in edges:
            rc = self.lib.cet_graph_add_edge(
                ctypes.byref(g),
                int(src),
                int(dst),
                int(ws),
                int(we),
            )
            if rc != 0:
                raise OverflowError("cet_graph_add_edge failed, likely CET_MAX_EDGES reached")
        return g

    @staticmethod
    def _paths_from_result(out: CETResult) -> list[list[int]]:
        return [[out.paths[i][j] for j in range(out.path_len[i])] for i in range(out.count)]

    def run_hcet(
        self,
        query: CETQuery,
        events: list[tuple[int, str, str, int]],
        edges: list[tuple[int, int, int, int]],
        switch_depth: int = 2,
        *,
        raise_on_overflow: bool = False,
        native_threads: int | None = None,
        enable_mmap_arena: bool | None = None,
        mmap_workspace_bytes: int | None = None,
        madvise_hugepage: bool | None = None,
        use_mmap_arena: bool | None = None,
        mmap_arena_bytes: int | None = None,
        parallel_min_start_vertices: int | None = None,
    ) -> CETMatch:
        # Convenience compatibility: callers can use run_hcet(...) for both the
        # classic and native-runtime paths. `parallel_min_start_vertices` is
        # accepted for API symmetry with Databricks config, but the C v3 runtime
        # currently shards by vertex range when native_threads > 1.
        del parallel_min_start_vertices
        if use_mmap_arena is not None and enable_mmap_arena is None:
            enable_mmap_arena = use_mmap_arena
        if mmap_arena_bytes is not None and mmap_workspace_bytes is None:
            mmap_workspace_bytes = mmap_arena_bytes
        if native_threads is not None or enable_mmap_arena is not None or mmap_workspace_bytes is not None or madvise_hugepage is not None:
            return self.run_hcet_parallel(
                query,
                events,
                edges,
                switch_depth=switch_depth,
                native_threads=native_threads,
                enable_mmap_arena=enable_mmap_arena,
                mmap_workspace_bytes=mmap_workspace_bytes,
                madvise_hugepage=madvise_hugepage,
                raise_on_overflow=raise_on_overflow,
            )

        g = self._build_graph(events, edges)
        out = CETResult()
        stats_dict: dict[str, Any] = {}

        if self.has_extended_exec:
            stats = CETExecStats()
            self.lib.cet_execute_hcet_ex(
                ctypes.byref(g),
                ctypes.byref(query),
                int(switch_depth),
                ctypes.byref(out),
                ctypes.byref(stats),
            )
            stats_dict = stats.as_dict()
            if raise_on_overflow and stats_dict.get("overflow"):
                raise OverflowError(stats_dict.get("error") or "CET execution overflow/truncation")
        else:
            self.lib.cet_execute_hcet(
                ctypes.byref(g),
                ctypes.byref(query),
                int(switch_depth),
                ctypes.byref(out),
            )

        return CETMatch(paths=self._paths_from_result(out), stats=stats_dict)

    def runtime_config(
        self,
        *,
        native_threads: int | None = None,
        enable_mmap_arena: bool | None = None,
        mmap_workspace_bytes: int | None = None,
        madvise_hugepage: bool | None = None,
        deterministic_merge: bool = True,
    ) -> CETRuntimeConfig:
        cfg = CETRuntimeConfig()
        if self.has_runtime_defaults:
            self.lib.cet_runtime_config_default(ctypes.byref(cfg))
        else:
            cfg.version = CET_RUNTIME_CONFIG_VERSION
            cfg.native_threads = int(os.getenv("CET_NATIVE_THREADS", "1"))
            cfg.enable_mmap_arena = int(_env_bool("CET_ENABLE_MMAP_ARENA", False))
            cfg.mmap_workspace_bytes = int(os.getenv("CET_MMAP_WORKSPACE_BYTES", "0"))
            cfg.madvise_hugepage = int(_env_bool("CET_MADVISE_HUGEPAGE", False))
            cfg.deterministic_merge = 1

        if native_threads is not None:
            cfg.native_threads = max(1, int(native_threads))
        if enable_mmap_arena is not None:
            cfg.enable_mmap_arena = int(bool(enable_mmap_arena))
        if mmap_workspace_bytes is not None:
            cfg.mmap_workspace_bytes = max(0, int(mmap_workspace_bytes))
        if madvise_hugepage is not None:
            cfg.madvise_hugepage = int(bool(madvise_hugepage))
        cfg.deterministic_merge = int(bool(deterministic_merge))
        return cfg

    def run_hcet_parallel(
        self,
        query: CETQuery,
        events: list[tuple[int, str, str, int]],
        edges: list[tuple[int, int, int, int]],
        switch_depth: int = 2,
        *,
        native_threads: int | None = None,
        enable_mmap_arena: bool | None = None,
        mmap_workspace_bytes: int | None = None,
        madvise_hugepage: bool | None = None,
        raise_on_overflow: bool = False,
    ) -> CETMatch:
        if not self.has_parallel_exec:
            match = self.run_hcet(query, events, edges, switch_depth, raise_on_overflow=raise_on_overflow)
            match.stats.setdefault("runtime", {"parallel_enabled": False, "error": "parallel symbol unavailable"})
            return match

        g = self._build_graph(events, edges)
        out = CETResult()
        stats = CETExecStats()
        runtime_stats = CETRuntimeStats()
        cfg = self.runtime_config(
            native_threads=native_threads,
            enable_mmap_arena=enable_mmap_arena,
            mmap_workspace_bytes=mmap_workspace_bytes,
            madvise_hugepage=madvise_hugepage,
        )

        rc = self.lib.cet_execute_hcet_parallel_ex(
            ctypes.byref(g),
            ctypes.byref(query),
            int(switch_depth),
            ctypes.byref(cfg),
            ctypes.byref(out),
            ctypes.byref(stats),
            ctypes.byref(runtime_stats),
        )
        if rc != 0:
            raise RuntimeError(runtime_stats.as_dict().get("error") or f"parallel H-CET failed rc={rc}")

        stats_dict = stats.as_dict()
        stats_dict["runtime"] = runtime_stats.as_dict()
        if raise_on_overflow and stats_dict.get("overflow"):
            raise OverflowError(stats_dict.get("error") or "CET execution overflow/truncation")
        return CETMatch(paths=self._paths_from_result(out), stats=stats_dict)

    def set_cost_coefficients(
        self,
        mem_vertex: float,
        mem_edge: float,
        cpu_edge: float,
        cpu_vertex: float,
    ) -> None:
        self.lib.cet_set_cost_coefficients(
            float(mem_vertex),
            float(mem_edge),
            float(cpu_edge),
            float(cpu_vertex),
        )
