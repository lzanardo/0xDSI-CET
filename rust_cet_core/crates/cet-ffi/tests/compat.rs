//! Tests for the C-engine drop-in compat layer.
//!
//! These validate that the un-prefixed `cet_*` symbols in `crate::compat`
//! produce results equivalent to the safer opaque-handle API. They are
//! deliberately kept off the `#[cfg(have_c_engine)]` gate because they
//! don't link the C engine — they call our own `#[repr(C)]` functions in
//! the same test binary.

use std::alloc::{alloc_zeroed, dealloc, Layout};
use std::ffi::CString;
use std::ptr::{self, NonNull};

use oxdsi_cet::compat::{
    cet_exec_stats_t, cet_execute_hcet, cet_execute_hcet_ex, cet_execute_hcet_parallel_ex,
    cet_execute_mcet, cet_execute_mcet_ex, cet_execute_tcet, cet_execute_tcet_ex,
    cet_graph_add_edge, cet_graph_add_vertex, cet_graph_init, cet_graph_t, cet_parse_query,
    cet_query_t, cet_result_t, cet_runtime_config_default, cet_runtime_config_t,
    cet_runtime_stats_t, cet_set_cost_coefficients,
};

/// Heap-allocated, zero-initialized `T`. Same pattern as the C-vs-Rust diff
/// test — the compat `cet_graph_t` / `cet_result_t` are multi-MiB structs
/// that must not live on the stack.
struct HeapZeroed<T> {
    ptr: NonNull<T>,
    layout: Layout,
}

impl<T> HeapZeroed<T> {
    fn new() -> Self {
        let layout = Layout::new::<T>();
        let raw = unsafe { alloc_zeroed(layout) } as *mut T;
        let ptr = NonNull::new(raw).expect("alloc_zeroed failed");
        Self { ptr, layout }
    }
    fn as_ptr_mut(&mut self) -> *mut T {
        self.ptr.as_ptr()
    }
    fn as_ptr(&self) -> *const T {
        self.ptr.as_ptr() as *const T
    }
}

impl<T> Drop for HeapZeroed<T> {
    fn drop(&mut self) {
        unsafe { dealloc(self.ptr.as_ptr() as *mut u8, self.layout) };
    }
}

fn build_curated_graph_and_query() -> (HeapZeroed<cet_graph_t>, HeapZeroed<cet_query_t>) {
    let mut g = HeapZeroed::<cet_graph_t>::new();
    let mut q = HeapZeroed::<cet_query_t>::new();
    let pk = CString::new("p").unwrap();
    unsafe {
        cet_graph_init(g.as_ptr_mut());
        for &(id, ty, t) in
            &[(1i32, "A", 1i64), (2, "B", 2), (3, "C", 3), (4, "A", 4), (5, "B", 5), (6, "C", 6)]
        {
            let ty = CString::new(ty).unwrap();
            let rc = cet_graph_add_vertex(g.as_ptr_mut(), id, pk.as_ptr(), ty.as_ptr(), t);
            assert_eq!(rc, 0, "add_vertex rc for id={id}");
        }
        for &(s, d) in &[(1i32, 2i32), (2, 3), (3, 4), (4, 5), (5, 6)] {
            let rc = cet_graph_add_edge(g.as_ptr_mut(), s, d, 0, 0);
            assert_eq!(rc, 0);
        }
        let name = CString::new("q").unwrap();
        let pat = CString::new("A,B,C").unwrap();
        let rc = cet_parse_query(name.as_ptr(), pat.as_ptr(), -1, 0, q.as_ptr_mut());
        assert_eq!(rc, 0);
    }
    (g, q)
}

fn count_paths(r: &HeapZeroed<cet_result_t>) -> usize {
    unsafe { (*r.as_ptr()).count }
}

#[test]
fn compat_mcet_emits_expected_paths() {
    let (g, q) = build_curated_graph_and_query();
    let mut out = HeapZeroed::<cet_result_t>::new();
    unsafe { cet_execute_mcet(g.as_ptr(), q.as_ptr(), out.as_ptr_mut()) };
    let n = count_paths(&out);
    assert!(n > 0, "MCET produced no paths");
}

#[test]
fn compat_tcet_emits_expected_paths() {
    let (g, q) = build_curated_graph_and_query();
    let mut out = HeapZeroed::<cet_result_t>::new();
    unsafe { cet_execute_tcet(g.as_ptr(), q.as_ptr(), out.as_ptr_mut()) };
    assert!(count_paths(&out) > 0);
}

#[test]
fn compat_hcet_emits_expected_paths() {
    let (g, q) = build_curated_graph_and_query();
    let mut out = HeapZeroed::<cet_result_t>::new();
    unsafe { cet_execute_hcet(g.as_ptr(), q.as_ptr(), 2, out.as_ptr_mut()) };
    assert!(count_paths(&out) > 0);
}

#[test]
fn compat_algorithms_agree() {
    let (g, q) = build_curated_graph_and_query();
    let mut m = HeapZeroed::<cet_result_t>::new();
    let mut t = HeapZeroed::<cet_result_t>::new();
    let mut h = HeapZeroed::<cet_result_t>::new();
    unsafe {
        cet_execute_mcet(g.as_ptr(), q.as_ptr(), m.as_ptr_mut());
        cet_execute_tcet(g.as_ptr(), q.as_ptr(), t.as_ptr_mut());
        cet_execute_hcet(g.as_ptr(), q.as_ptr(), 2, h.as_ptr_mut());
    }

    fn to_set(r: &HeapZeroed<cet_result_t>) -> Vec<Vec<i32>> {
        unsafe {
            let count = (*r.as_ptr()).count;
            (0..count)
                .map(|i| {
                    let n = (*r.as_ptr()).path_len[i];
                    (0..n).map(|j| (*r.as_ptr()).paths[i][j]).collect::<Vec<_>>()
                })
                .collect()
        }
    }

    let mut sm = to_set(&m);
    let mut st = to_set(&t);
    let mut sh = to_set(&h);
    sm.sort();
    st.sort();
    sh.sort();
    sm.dedup();
    st.dedup();
    sh.dedup();
    assert_eq!(sm, st);
    assert_eq!(st, sh);
}

#[test]
fn compat_ex_variants_populate_stats() {
    let (g, q) = build_curated_graph_and_query();
    let mut out = HeapZeroed::<cet_result_t>::new();
    let mut stats = HeapZeroed::<cet_exec_stats_t>::new();
    unsafe { cet_execute_hcet_ex(g.as_ptr(), q.as_ptr(), 2, out.as_ptr_mut(), stats.as_ptr_mut()) };
    let s = unsafe { &*stats.as_ptr() };
    assert!(s.paths_emitted > 0);
    assert!(s.overflow == 0);
}

#[test]
fn compat_ex_variants_accept_null_stats() {
    let (g, q) = build_curated_graph_and_query();
    let mut out = HeapZeroed::<cet_result_t>::new();
    // Null stats must not crash — this closes the bug from
    // c_engine/src/algorithms.c:387-391 for the compat layer too.
    unsafe {
        cet_execute_mcet_ex(g.as_ptr(), q.as_ptr(), out.as_ptr_mut(), ptr::null_mut());
        cet_execute_tcet_ex(g.as_ptr(), q.as_ptr(), out.as_ptr_mut(), ptr::null_mut());
        cet_execute_hcet_ex(g.as_ptr(), q.as_ptr(), 2, out.as_ptr_mut(), ptr::null_mut());
    }
}

#[test]
fn compat_runtime_config_default_populates() {
    let mut cfg = HeapZeroed::<cet_runtime_config_t>::new();
    unsafe { cet_runtime_config_default(cfg.as_ptr_mut()) };
    let c = unsafe { &*cfg.as_ptr() };
    assert_eq!(c.version, 1);
    assert_eq!(c.native_threads, 1);
    assert_eq!(c.deterministic_merge, 1);
}

#[test]
fn compat_set_cost_coefficients_is_a_noop() {
    // Compat behavior: the C engine used file-scope globals; the Rust port
    // uses per-instance CostModel. The symbol exists but has no effect.
    unsafe { cet_set_cost_coefficients(1.0, 2.0, 3.0, 4.0) };
}

#[test]
fn compat_hcet_parallel_ex_produces_output() {
    let (g, q) = build_curated_graph_and_query();
    let mut out = HeapZeroed::<cet_result_t>::new();
    let mut stats = HeapZeroed::<cet_exec_stats_t>::new();
    let mut rt = HeapZeroed::<cet_runtime_stats_t>::new();
    let mut cfg = HeapZeroed::<cet_runtime_config_t>::new();
    unsafe {
        cet_runtime_config_default(cfg.as_ptr_mut());
        (*cfg.as_ptr_mut()).native_threads = 2;
        let rc = cet_execute_hcet_parallel_ex(
            g.as_ptr(),
            q.as_ptr(),
            2,
            cfg.as_ptr(),
            out.as_ptr_mut(),
            stats.as_ptr_mut(),
            rt.as_ptr_mut(),
        );
        assert_eq!(rc, 0);
    }
    assert!(count_paths(&out) > 0);
    let rt = unsafe { &*rt.as_ptr() };
    assert_eq!(rt.native_threads_requested, 2);
    assert!(rt.native_threads_used > 0);
}

#[test]
fn compat_null_graph_pointer_is_safe() {
    let mut out = HeapZeroed::<cet_result_t>::new();
    let q = HeapZeroed::<cet_query_t>::new();
    // Bail out cleanly when g is null, don't crash.
    unsafe { cet_execute_mcet(ptr::null(), q.as_ptr(), out.as_ptr_mut()) };
    assert_eq!(count_paths(&out), 0);
}

#[test]
fn compat_layout_sizes_match_c_engine() {
    // Sanity: struct sizes match what bindings/python/bridge.py expects.
    // If these ever drift, ctypes.sizeof(CETVertex) etc. would disagree and
    // the Python bridge would crash before running. Locking them in here
    // gives us a Rust-side canary.
    assert_eq!(std::mem::size_of::<oxdsi_cet::compat::cet_vertex_t>(), 4 + 4 + 64 + 32 + 8);
    assert_eq!(std::mem::size_of::<oxdsi_cet::compat::cet_edge_t>(), 4 + 4 + 8 + 8);
    // cet_event_type_t: name(32) + kleene_plus(4) + pad(4) + predicate(8) + predicate_ctx(8) = 56
    assert_eq!(std::mem::size_of::<oxdsi_cet::compat::cet_event_type_t>(), 56);
}
