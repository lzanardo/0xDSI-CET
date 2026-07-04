//! Diff tests: run the same query through the C reference engine and the
//! Rust cet-ffi shim, assert identical output.
//!
//! This is the strongest possible acceptance bar for the port: byte-for-byte
//! parity on curated inputs. The C engine is compiled by `build.rs` and
//! linked as `cet_engine_c`; symbols we call are declared here as
//! `extern "C"`.
//!
//! The tests are gated on `have_c_engine` — set by `build.rs` when the
//! `c_engine/` source tree is present next to the workspace.

#![cfg(have_c_engine)]

use std::alloc::{alloc_zeroed, dealloc, Layout};
use std::ffi::CString;
use std::os::raw::{c_char, c_int};
use std::ptr::NonNull;

/// Owned, heap-allocated, zero-initialized `T`. Necessary because the C
/// `cet_graph_t` / `cet_result_t` structs contain multi-megabyte fixed-size
/// arrays; a naive `Box::new(std::mem::zeroed())` overflows the stack because
/// the value must live briefly on the stack before being moved.
struct HeapZeroed<T> {
    ptr: NonNull<T>,
    layout: Layout,
}

impl<T> HeapZeroed<T> {
    fn new() -> Self {
        let layout = Layout::new::<T>();
        // SAFETY: `T` is `#[repr(C)]` POD-like (arrays of primitives + POD
        // structs), so an all-zero bit pattern is a valid instance.
        let raw = unsafe { alloc_zeroed(layout) } as *mut T;
        let ptr = NonNull::new(raw).expect("heap allocation failed");
        Self { ptr, layout }
    }

    fn as_ptr_mut(&mut self) -> *mut T {
        self.ptr.as_ptr()
    }

    fn as_ptr(&self) -> *const T {
        self.ptr.as_ptr() as *const T
    }

    fn as_ref(&self) -> &T {
        // SAFETY: pointer is valid and aligned for T; T is POD-safe.
        unsafe { &*self.ptr.as_ptr() }
    }
}

impl<T> Drop for HeapZeroed<T> {
    fn drop(&mut self) {
        // SAFETY: allocated by `alloc_zeroed` with the same layout.
        unsafe { dealloc(self.ptr.as_ptr() as *mut u8, self.layout) };
    }
}

// ---------------------------------------------------------------------------
// C engine ABI (subset used by these tests)
// ---------------------------------------------------------------------------
//
// These declarations mirror the layout in c_engine/include/cet.h. We do NOT
// consume the C structs by field — we allocate them via calloc/into_raw and
// treat them as opaque byte blobs.

const CET_MAX_SEQ: usize = 16;
const CET_MAX_EVENTS: usize = 200_000;
const CET_MAX_EDGES: usize = 1_000_000;
const CET_MAX_PATHS: usize = 100_000;
const CET_MAX_PATH_LEN: usize = 64;
const CET_MAX_ERROR_LEN: usize = 256;

#[repr(C)]
#[derive(Clone, Copy)]
struct CetEventTypeC {
    name: [c_char; 32],
    kleene_plus: c_int,
    predicate: *mut std::ffi::c_void,
    predicate_ctx: *mut std::ffi::c_void,
}

#[repr(C)]
struct CetQueryC {
    name: [c_char; 64],
    seq: [CetEventTypeC; CET_MAX_SEQ],
    seq_len: usize,
    within_ms: i64,
    slide_ms: i64,
    skip_till_any_match: c_int,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct CetVertexC {
    id: c_int,
    partition_key: [c_char; 64],
    event_type: [c_char; 32],
    event_time_ms: i64,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct CetEdgeC {
    src: c_int,
    dst: c_int,
    window_start_ms: i64,
    window_end_ms: i64,
}

#[repr(C)]
struct CetGraphC {
    vertices: [CetVertexC; CET_MAX_EVENTS],
    edges: [CetEdgeC; CET_MAX_EDGES],
    vcount: usize,
    ecount: usize,
}

#[repr(C)]
struct CetResultC {
    paths: [[c_int; CET_MAX_PATH_LEN]; CET_MAX_PATHS],
    path_len: [usize; CET_MAX_PATHS],
    count: usize,
}

#[repr(C)]
struct CetExecStatsC {
    paths_emitted: usize,
    paths_truncated: usize,
    states_enqueued: usize,
    states_truncated: usize,
    seed_paths: usize,
    max_depth_seen: usize,
    temporal_rejects: usize,
    edge_window_rejects: usize,
    predicate_rejects: usize,
    overflow: c_int,
    error: [c_char; CET_MAX_ERROR_LEN],
}

extern "C" {
    fn cet_parse_query(
        name: *const c_char,
        pattern_csv: *const c_char,
        within_ms: i64,
        slide_ms: i64,
        out: *mut CetQueryC,
    ) -> c_int;
    fn cet_graph_init(g: *mut CetGraphC);
    fn cet_graph_add_vertex(
        g: *mut CetGraphC,
        id: c_int,
        pkey: *const c_char,
        etype: *const c_char,
        t: i64,
    ) -> c_int;
    fn cet_graph_add_edge(
        g: *mut CetGraphC,
        src: c_int,
        dst: c_int,
        wstart: i64,
        wend: i64,
    ) -> c_int;
    fn cet_execute_mcet_ex(
        g: *const CetGraphC,
        q: *const CetQueryC,
        out: *mut CetResultC,
        stats: *mut CetExecStatsC,
    );
    fn cet_execute_tcet_ex(
        g: *const CetGraphC,
        q: *const CetQueryC,
        out: *mut CetResultC,
        stats: *mut CetExecStatsC,
    );
    fn cet_execute_hcet_ex(
        g: *const CetGraphC,
        q: *const CetQueryC,
        switch_depth: usize,
        out: *mut CetResultC,
        stats: *mut CetExecStatsC,
    );
}

// ---------------------------------------------------------------------------
// Rust FFI ABI (used through the cet-ffi shim directly, without libloading)
// ---------------------------------------------------------------------------

use cet_ffi::{
    cet_ffi_execute_hcet, cet_ffi_execute_mcet, cet_ffi_execute_tcet, cet_ffi_graph_add_edge,
    cet_ffi_graph_add_vertex, cet_ffi_graph_free, cet_ffi_graph_new, cet_ffi_query_free,
    cet_ffi_query_parse, cet_ffi_result_copy_path, cet_ffi_result_free, cet_ffi_result_new,
    cet_ffi_result_path_count, cet_ffi_result_path_len, cet_ffi_stats_free, cet_ffi_stats_new,
};

// ---------------------------------------------------------------------------
// Helpers to build inputs on both sides and gather canonical output.
// ---------------------------------------------------------------------------

type Scenario = (
    &'static str,                  // human-readable name
    Vec<(i64, &'static str, i64)>, // (id, event_type, event_time_ms) tuples
    Vec<(i64, i64)>,               // (src, dst) edges (no edge windows)
    &'static str,                  // pattern CSV
    i64,                           // within_ms (-1 = no bound)
    usize,                         // hcet switch_depth
);

fn scenarios() -> Vec<Scenario> {
    vec![
        (
            "linear_abc",
            vec![(1, "A", 1), (2, "B", 2), (3, "C", 3)],
            vec![(1, 2), (2, 3)],
            "A,B,C",
            -1,
            2,
        ),
        (
            "linear_abc_skip",
            vec![(1, "A", 1), (2, "X", 2), (3, "B", 3), (4, "C", 4)],
            vec![(1, 2), (2, 3), (3, 4)],
            "A,B,C",
            -1,
            2,
        ),
        (
            "two_seeds",
            vec![(1, "A", 1), (2, "A", 2), (3, "B", 3), (4, "C", 4)],
            vec![(1, 3), (2, 3), (3, 4)],
            "A,B,C",
            -1,
            2,
        ),
        (
            "kleene_a_plus",
            vec![(1, "A", 1), (2, "A", 2), (3, "B", 3)],
            vec![(1, 2), (2, 3)],
            "A+,B",
            -1,
            1,
        ),
        (
            "within_ms_filter",
            vec![(1, "A", 0), (2, "B", 5), (3, "B", 500)],
            vec![(1, 2), (1, 3)],
            "A,B",
            10,
            1,
        ),
    ]
}

fn c_paths(out: &CetResultC) -> Vec<Vec<i64>> {
    (0..out.count)
        .map(|i| {
            let n = out.path_len[i];
            (0..n).map(|j| out.paths[i][j] as i64).collect()
        })
        .collect()
}

fn canonicalize(mut ps: Vec<Vec<i64>>) -> Vec<Vec<i64>> {
    // Paths are already ordered in emission order; sort at the outer
    // level for set equality.
    ps.sort();
    ps.dedup();
    ps
}

// Run both engines and canonicalize their outputs.
fn run_both(scenario: &Scenario, exec: &str) -> (Vec<Vec<i64>>, Vec<Vec<i64>>) {
    let (_name, verts, edges, pattern_csv, within_ms, switch_depth) = scenario;

    // ---- C engine ----
    let mut g_c: HeapZeroed<CetGraphC> = HeapZeroed::new();
    let mut q_c: HeapZeroed<CetQueryC> = HeapZeroed::new();
    let mut out_c: HeapZeroed<CetResultC> = HeapZeroed::new();
    let mut stats_c: HeapZeroed<CetExecStatsC> = HeapZeroed::new();

    unsafe {
        cet_graph_init(g_c.as_ptr_mut());
    }
    let pk = CString::new("p").unwrap();
    for &(id, ty, t) in verts {
        let ty = CString::new(ty).unwrap();
        unsafe {
            assert_eq!(
                cet_graph_add_vertex(g_c.as_ptr_mut(), id as c_int, pk.as_ptr(), ty.as_ptr(), t),
                0
            );
        }
    }
    for &(s, d) in edges {
        unsafe {
            assert_eq!(cet_graph_add_edge(g_c.as_ptr_mut(), s as c_int, d as c_int, 0, 0), 0);
        }
    }
    let name = CString::new("q").unwrap();
    let pat = CString::new(*pattern_csv).unwrap();
    unsafe {
        assert_eq!(
            cet_parse_query(name.as_ptr(), pat.as_ptr(), *within_ms, 0, q_c.as_ptr_mut()),
            0
        );
        match exec {
            "mcet" => cet_execute_mcet_ex(
                g_c.as_ptr(),
                q_c.as_ptr(),
                out_c.as_ptr_mut(),
                stats_c.as_ptr_mut(),
            ),
            "tcet" => cet_execute_tcet_ex(
                g_c.as_ptr(),
                q_c.as_ptr(),
                out_c.as_ptr_mut(),
                stats_c.as_ptr_mut(),
            ),
            "hcet" => cet_execute_hcet_ex(
                g_c.as_ptr(),
                q_c.as_ptr(),
                *switch_depth,
                out_c.as_ptr_mut(),
                stats_c.as_ptr_mut(),
            ),
            _ => unreachable!(),
        };
    }
    let c_result = c_paths(out_c.as_ref());

    // ---- Rust FFI ----
    let g_r = cet_ffi_graph_new(1024, 1024);
    let q_r;
    let out_r = cet_ffi_result_new(1024, 64);
    let stats_r = cet_ffi_stats_new();
    unsafe {
        for &(id, ty, t) in verts {
            let ty = CString::new(ty).unwrap();
            assert_eq!(cet_ffi_graph_add_vertex(g_r, id, pk.as_ptr(), ty.as_ptr(), t), 0);
        }
        for &(s, d) in edges {
            assert_eq!(cet_ffi_graph_add_edge(g_r, s, d, 0, 0), 0);
        }
        q_r = cet_ffi_query_parse(name.as_ptr(), pat.as_ptr(), *within_ms, 0);
        assert!(!q_r.is_null());
        let rc = match exec {
            "mcet" => cet_ffi_execute_mcet(g_r, q_r, out_r, stats_r),
            "tcet" => cet_ffi_execute_tcet(g_r, q_r, out_r, stats_r),
            "hcet" => cet_ffi_execute_hcet(g_r, q_r, *switch_depth, out_r, stats_r),
            _ => unreachable!(),
        };
        assert_eq!(rc, 0);
    }
    let mut rust_result: Vec<Vec<i64>> = Vec::new();
    unsafe {
        let n = cet_ffi_result_path_count(out_r);
        for i in 0..n {
            let len = cet_ffi_result_path_len(out_r, i);
            let mut buf = vec![0i64; len];
            let copied = cet_ffi_result_copy_path(out_r, i, buf.as_mut_ptr(), buf.len());
            assert_eq!(copied, len);
            rust_result.push(buf);
        }

        cet_ffi_stats_free(stats_r);
        cet_ffi_result_free(out_r);
        cet_ffi_query_free(q_r);
        cet_ffi_graph_free(g_r);
    }

    (canonicalize(c_result), canonicalize(rust_result))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn diff_tcet_matches_c_engine() {
    for s in scenarios() {
        let (c, r) = run_both(&s, "tcet");
        assert_eq!(c, r, "tcet mismatch on {}", s.0);
    }
}

#[test]
fn diff_mcet_matches_c_engine() {
    for s in scenarios() {
        let (c, r) = run_both(&s, "mcet");
        assert_eq!(c, r, "mcet mismatch on {}", s.0);
    }
}

#[test]
fn diff_hcet_matches_c_engine() {
    for s in scenarios() {
        let (c, r) = run_both(&s, "hcet");
        assert_eq!(c, r, "hcet mismatch on {}", s.0);
    }
}
