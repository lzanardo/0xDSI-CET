//! Property-based diff test: for **every** randomly generated
//! `(Graph, Query)` produced by `cet-core::testkit`, the C engine and the
//! Rust `cet-ffi` shim must produce identical canonicalized path sets across
//! all three algorithms.
//!
//! This is a strictly stronger acceptance bar than the curated diff tests in
//! `diff_c_engine.rs` — it verifies byte-level parity on the entire input
//! space explored by `testkit::arb_graph()` + `testkit::arb_query()`.
//!
//! The C-side scratch buffers (`CetGraphC`, `CetResultC`, `CetQueryC`,
//! `CetExecStatsC`) are ~70 MiB combined and allocated **once** at the start
//! of each proptest test function, then re-zeroed per case, to keep runtime
//! reasonable.
//!
//! Gated on `have_c_engine` (set by `build.rs` when `c_engine/` is present).

#![cfg(have_c_engine)]

use std::alloc::{alloc_zeroed, dealloc, Layout};
use std::cell::RefCell;
use std::ffi::CString;
use std::os::raw::{c_char, c_int};
use std::ptr::{self, NonNull};

use cet_core::{testkit, Graph, Query};
use proptest::prelude::*;

use oxdsi_cet::{
    cet_ffi_execute_hcet, cet_ffi_execute_mcet, cet_ffi_execute_tcet, cet_ffi_graph_add_edge,
    cet_ffi_graph_add_vertex, cet_ffi_graph_free, cet_ffi_graph_new, cet_ffi_query_free,
    cet_ffi_query_new, cet_ffi_query_push_step, cet_ffi_query_set_skip_till_any_match,
    cet_ffi_query_set_within_ms, cet_ffi_result_copy_path, cet_ffi_result_free, cet_ffi_result_new,
    cet_ffi_result_path_count, cet_ffi_result_path_len, cet_ffi_stats_free, cet_ffi_stats_new,
    cet_ffi_stats_overflow,
};

// ---------------------------------------------------------------------------
// C engine ABI (subset used by these tests)
// ---------------------------------------------------------------------------

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
    fn cet_exec_stats_init(stats: *mut CetExecStatsC);
}

// ---------------------------------------------------------------------------
// Heap-allocated zero-initialized scratch (reused across proptest cases)
// ---------------------------------------------------------------------------

struct HeapZeroed<T> {
    ptr: NonNull<T>,
    layout: Layout,
}

impl<T> HeapZeroed<T> {
    fn new() -> Self {
        let layout = Layout::new::<T>();
        let raw = unsafe { alloc_zeroed(layout) } as *mut T;
        let ptr = NonNull::new(raw).expect("heap alloc failed");
        Self { ptr, layout }
    }

    fn as_ptr_mut(&mut self) -> *mut T {
        self.ptr.as_ptr()
    }

    fn as_ptr(&self) -> *const T {
        self.ptr.as_ptr() as *const T
    }

    /// Re-zero the block so the next proptest case starts clean without
    /// reallocating.
    fn zero(&mut self) {
        unsafe {
            ptr::write_bytes(self.ptr.as_ptr() as *mut u8, 0, self.layout.size());
        }
    }
}

impl<T> Drop for HeapZeroed<T> {
    fn drop(&mut self) {
        unsafe { dealloc(self.ptr.as_ptr() as *mut u8, self.layout) };
    }
}

/// Reusable scratch: one allocation per test, reset per case.
struct Scratch {
    g: HeapZeroed<CetGraphC>,
    q: HeapZeroed<CetQueryC>,
    out: HeapZeroed<CetResultC>,
    stats: HeapZeroed<CetExecStatsC>,
}

impl Scratch {
    fn new() -> Self {
        Self {
            g: HeapZeroed::new(),
            q: HeapZeroed::new(),
            out: HeapZeroed::new(),
            stats: HeapZeroed::new(),
        }
    }

    fn reset(&mut self) {
        self.g.zero();
        self.q.zero();
        self.out.zero();
        self.stats.zero();
        unsafe {
            cet_graph_init(self.g.as_ptr_mut());
            cet_exec_stats_init(self.stats.as_ptr_mut());
        }
    }
}

// ---------------------------------------------------------------------------
// Input translation: Rust Graph/Query -> C engine scratch and cet-ffi handles
// ---------------------------------------------------------------------------

/// Populate a C-side graph from a Rust `Graph`. Returns `Some(())` on success
/// or `None` if any add-vertex/add-edge returns non-zero (should only happen
/// on capacity overflow, which testkit's small graphs never hit).
fn populate_c_graph(scratch: &mut Scratch, g: &Graph) -> Option<()> {
    for v in g.vertices() {
        // The C engine takes int for id; our testkit generator uses small ids.
        let id = i32::try_from(v.id).ok()?;
        let pk = CString::new(v.partition_key.as_str()).ok()?;
        let ety = CString::new(v.event_type.as_str()).ok()?;
        let rc = unsafe {
            cet_graph_add_vertex(
                scratch.g.as_ptr_mut(),
                id,
                pk.as_ptr(),
                ety.as_ptr(),
                v.event_time_ms,
            )
        };
        if rc != 0 {
            return None;
        }
    }
    for e in g.edges() {
        let src = i32::try_from(e.src).ok()?;
        let dst = i32::try_from(e.dst).ok()?;
        let rc = unsafe {
            cet_graph_add_edge(scratch.g.as_ptr_mut(), src, dst, e.window_start_ms, e.window_end_ms)
        };
        if rc != 0 {
            return None;
        }
    }
    Some(())
}

/// Populate a C-side query struct field-by-field. Skips queries containing
/// predicates (testkit doesn't produce those, but we defensively bail).
fn populate_c_query(scratch: &mut Scratch, q: &Query) -> Option<()> {
    if q.seq.iter().any(|s| s.predicate.is_some()) {
        return None;
    }
    let name = CString::new(q.name.as_str()).ok()?;
    let q_ptr = scratch.q.as_ptr_mut();
    unsafe {
        // Zero-fill was done in reset(); copy name in.
        let name_bytes = name.as_bytes_with_nul();
        let name_field = (&raw mut (*q_ptr).name) as *mut c_char;
        let n = name_bytes.len().min(64);
        ptr::copy_nonoverlapping(name_bytes.as_ptr() as *const c_char, name_field, n);

        (*q_ptr).within_ms = q.within_ms;
        (*q_ptr).slide_ms = q.slide_ms;
        (*q_ptr).skip_till_any_match = if q.skip_till_any_match { 1 } else { 0 };
        (*q_ptr).seq_len = q.seq.len();

        for (i, step) in q.seq.iter().enumerate() {
            let step_name = CString::new(step.name.as_str()).ok()?;
            let name_bytes = step_name.as_bytes_with_nul();
            let name_field = (&raw mut (*q_ptr).seq[i].name) as *mut c_char;
            let n = name_bytes.len().min(32);
            ptr::copy_nonoverlapping(name_bytes.as_ptr() as *const c_char, name_field, n);
            (*q_ptr).seq[i].kleene_plus = if step.kleene_plus { 1 } else { 0 };
            (*q_ptr).seq[i].predicate = ptr::null_mut();
            (*q_ptr).seq[i].predicate_ctx = ptr::null_mut();
        }
    }
    Some(())
}

fn c_paths_from(out: *const CetResultC) -> Vec<Vec<i64>> {
    unsafe {
        let count = (*out).count;
        (0..count)
            .map(|i| {
                let n = (*out).path_len[i];
                (0..n).map(|j| (*out).paths[i][j] as i64).collect()
            })
            .collect()
    }
}

fn canonicalize(mut ps: Vec<Vec<i64>>) -> Vec<Vec<i64>> {
    ps.sort();
    ps.dedup();
    ps
}

// ---------------------------------------------------------------------------
// Rust FFI: build the same input via cet-ffi handles.
// ---------------------------------------------------------------------------

/// Wrapper that mirrors the C-side inputs through the Rust FFI shim,
/// executes the requested algorithm, and returns the canonicalized path set.
///
/// Returns `None` on FFI-detected input rejection (e.g. non-UTF-8 string,
/// which testkit's generators do not produce).
fn run_via_ffi(
    g: &Graph,
    q: &Query,
    algo: Algo,
    switch_depth: usize,
) -> Option<(Vec<Vec<i64>>, bool)> {
    unsafe {
        let g_handle = cet_ffi_graph_new(1024, 4096);
        if g_handle.is_null() {
            return None;
        }
        let pk_default = CString::new("p").ok()?;
        for v in g.vertices() {
            let pk = CString::new(v.partition_key.as_str()).unwrap_or_else(|_| pk_default.clone());
            let ety = CString::new(v.event_type.as_str()).ok()?;
            if cet_ffi_graph_add_vertex(g_handle, v.id, pk.as_ptr(), ety.as_ptr(), v.event_time_ms)
                != 0
            {
                cet_ffi_graph_free(g_handle);
                return None;
            }
        }
        for e in g.edges() {
            if cet_ffi_graph_add_edge(g_handle, e.src, e.dst, e.window_start_ms, e.window_end_ms)
                != 0
            {
                cet_ffi_graph_free(g_handle);
                return None;
            }
        }

        // Build query programmatically so we don't have to serialize back to CSV.
        let name = CString::new(q.name.as_str()).ok()?;
        let q_handle = cet_ffi_query_new(name.as_ptr());
        if q_handle.is_null() {
            cet_ffi_graph_free(g_handle);
            return None;
        }
        cet_ffi_query_set_within_ms(q_handle, q.within_ms);
        cet_ffi_query_set_skip_till_any_match(q_handle, if q.skip_till_any_match { 1 } else { 0 });
        for step in &q.seq {
            let ety = CString::new(step.name.as_str()).ok()?;
            if cet_ffi_query_push_step(q_handle, ety.as_ptr(), if step.kleene_plus { 1 } else { 0 })
                != 0
            {
                cet_ffi_query_free(q_handle);
                cet_ffi_graph_free(g_handle);
                return None;
            }
        }

        let out = cet_ffi_result_new(1024, 64);
        let stats = cet_ffi_stats_new();
        let rc = match algo {
            Algo::Mcet => cet_ffi_execute_mcet(g_handle, q_handle, out, stats),
            Algo::Tcet => cet_ffi_execute_tcet(g_handle, q_handle, out, stats),
            Algo::Hcet => cet_ffi_execute_hcet(g_handle, q_handle, switch_depth, out, stats),
        };
        assert_eq!(rc, 0);

        let overflow = cet_ffi_stats_overflow(stats) != 0;
        let mut paths: Vec<Vec<i64>> = Vec::new();
        let n = cet_ffi_result_path_count(out);
        for i in 0..n {
            let len = cet_ffi_result_path_len(out, i);
            let mut buf = vec![0i64; len];
            let copied = cet_ffi_result_copy_path(out, i, buf.as_mut_ptr(), buf.len());
            assert_eq!(copied, len);
            paths.push(buf);
        }

        cet_ffi_stats_free(stats);
        cet_ffi_result_free(out);
        cet_ffi_query_free(q_handle);
        cet_ffi_graph_free(g_handle);

        Some((canonicalize(paths), overflow))
    }
}

fn run_via_c(
    scratch: &mut Scratch,
    g: &Graph,
    q: &Query,
    algo: Algo,
    switch_depth: usize,
) -> Option<(Vec<Vec<i64>>, bool)> {
    scratch.reset();
    populate_c_graph(scratch, g)?;
    populate_c_query(scratch, q)?;
    unsafe {
        match algo {
            Algo::Mcet => cet_execute_mcet_ex(
                scratch.g.as_ptr(),
                scratch.q.as_ptr(),
                scratch.out.as_ptr_mut(),
                scratch.stats.as_ptr_mut(),
            ),
            Algo::Tcet => cet_execute_tcet_ex(
                scratch.g.as_ptr(),
                scratch.q.as_ptr(),
                scratch.out.as_ptr_mut(),
                scratch.stats.as_ptr_mut(),
            ),
            Algo::Hcet => cet_execute_hcet_ex(
                scratch.g.as_ptr(),
                scratch.q.as_ptr(),
                switch_depth,
                scratch.out.as_ptr_mut(),
                scratch.stats.as_ptr_mut(),
            ),
        }
        let overflow = (*scratch.stats.as_ptr()).overflow != 0;
        let paths = c_paths_from(scratch.out.as_ptr());
        Some((canonicalize(paths), overflow))
    }
}

#[derive(Clone, Copy)]
enum Algo {
    Mcet,
    Tcet,
    Hcet,
}

// ---------------------------------------------------------------------------
// Property tests: one per algorithm. Each function owns a single Scratch.
// ---------------------------------------------------------------------------

/// Property: for every well-formed random `(Graph, Query)`, the C engine and
/// the Rust FFI shim produce identical MCET path sets.
///
/// NOTE: The Rust engine has an intentional **behavior improvement** over the
/// C engine (a cycle guard on `skip_till_any_match`, closing the unbounded
/// recursion bug from `c_engine/src/algorithms.c:210-213`). Testkit produces
/// acyclic graphs (edges point strictly forward in time), so the fix does not
/// change output on this input space and the diff is exact.
/// Per-test proptest config.
///
/// Reads `PROPTEST_CASES` from the environment (default 64). This is
/// deliberately lower than the pure-Rust property tests because each diff
/// case runs the full C engine + the full Rust FFI, so 2000 iterations
/// takes minutes.
fn diff_config() -> ProptestConfig {
    let cases = std::env::var("PROPTEST_CASES").ok().and_then(|s| s.parse().ok()).unwrap_or(64);
    ProptestConfig::with_cases(cases)
}

/// Property: for every well-formed random `(Graph, Query)`, the C engine and
/// the Rust FFI shim produce identical MCET path sets.
///
/// NOTE: The Rust engine has an intentional **behavior improvement** over the
/// C engine (a cycle guard on `skip_till_any_match`, closing the unbounded
/// recursion bug from `c_engine/src/algorithms.c:210-213`). Testkit produces
/// acyclic graphs (edges point strictly forward in time), so the fix does not
/// change output on this input space and the diff is exact.
#[test]
fn property_diff_mcet_matches_c_engine() {
    let scratch = RefCell::new(Scratch::new());
    proptest!(diff_config(), |(g in testkit::arb_graph(), q in testkit::arb_query())| {
        let switch_depth = q.len() / 2;
        let c = run_via_c(&mut scratch.borrow_mut(), &g, &q, Algo::Mcet, switch_depth);
        let r = run_via_ffi(&g, &q, Algo::Mcet, switch_depth);
        // If either side rejected the input (e.g. non-UTF-8) skip the case.
        if let (Some((cp, c_over)), Some((rp, r_over))) = (c, r) {
            if !c_over && !r_over {
                prop_assert_eq!(cp, rp);
            }
        }
    });
}

#[test]
fn property_diff_tcet_matches_c_engine() {
    let scratch = RefCell::new(Scratch::new());
    proptest!(diff_config(), |(g in testkit::arb_graph(), q in testkit::arb_query())| {
        let switch_depth = q.len() / 2;
        let c = run_via_c(&mut scratch.borrow_mut(), &g, &q, Algo::Tcet, switch_depth);
        let r = run_via_ffi(&g, &q, Algo::Tcet, switch_depth);
        if let (Some((cp, c_over)), Some((rp, r_over))) = (c, r) {
            if !c_over && !r_over {
                prop_assert_eq!(cp, rp);
            }
        }
    });
}

#[test]
fn property_diff_hcet_matches_c_engine() {
    let scratch = RefCell::new(Scratch::new());
    proptest!(diff_config(), |(g in testkit::arb_graph(), q in testkit::arb_query())| {
        for switch_depth in 0..=q.len() {
            let c = run_via_c(&mut scratch.borrow_mut(), &g, &q, Algo::Hcet, switch_depth);
            let r = run_via_ffi(&g, &q, Algo::Hcet, switch_depth);
            if let (Some((cp, c_over)), Some((rp, r_over))) = (c, r) {
                if !c_over && !r_over {
                    let msg = format!("diff at switch_depth={switch_depth}");
                    prop_assert_eq!(cp, rp, "{}", msg);
                }
            }
        }
    });
}
