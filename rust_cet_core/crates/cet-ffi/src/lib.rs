//! # cet-ffi
//!
//! C-ABI shim over `cet-core`. Exposes an **opaque handle** API so C callers
//! can drive the Rust engine without depending on Rust struct layouts.
//!
//! ## Handle lifecycle
//!
//! Every `_new` function returns a non-null `*mut Handle` on success, or
//! `NULL` on allocation failure. Every handle **must** be freed exactly once
//! via the corresponding `_free` function. Freeing `NULL` is a no-op.
//!
//! ## Thread safety
//!
//! Handles are not `Send` across FFI. Each handle must be used from a single
//! thread. The `cet_execute_hcet_parallel_ffi` function is internally
//! multi-threaded via Rayon and is safe to call.
//!
//! ## Errors
//!
//! Functions that can fail return an `int` status code (`0 = ok`,
//! `< 0 = error`). Detailed diagnostics are available via the stats
//! handles.
//!
//! ## Symbol naming
//!
//! All symbols are prefixed `cet_ffi_` to avoid conflicts with the C
//! reference implementation (`libc_engine`). Downstream Python/Java bindings
//! can `#define` shim the old names when they cut over.

#![warn(missing_docs)]

use std::ffi::{c_char, c_int, CStr};
use std::ptr;

use cet_core::{exec, Edge, EventType, ExecStats, Graph, MatchResult, Query, Vertex};
use cet_parallel::{execute_hcet_parallel, RuntimeConfig, RuntimeStats};

// ---------------------------------------------------------------------------
// Handles
// ---------------------------------------------------------------------------

/// Opaque handle to a `Graph`.
pub struct CetGraph {
    inner: Graph,
}

/// Opaque handle to a `Query`.
pub struct CetQuery {
    inner: Query,
}

/// Opaque handle to a `MatchResult`.
pub struct CetResult {
    inner: MatchResult,
}

/// Opaque handle to an `ExecStats`.
pub struct CetStats {
    inner: ExecStats,
}

/// Opaque handle to a `RuntimeConfig`.
pub struct CetRuntimeConfig {
    inner: RuntimeConfig,
}

/// Opaque handle to a `RuntimeStats`.
pub struct CetRuntimeStats {
    inner: RuntimeStats,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Box + leak: caller owns the pointer and must free via the matching `_free`.
#[inline]
fn into_handle<T>(t: T) -> *mut T {
    Box::into_raw(Box::new(t))
}

/// Safely reconstruct and drop a handle. `NULL` is a no-op.
///
/// # Safety
///
/// - `p` must be either null or a pointer previously returned from
///   [`into_handle`] and not yet freed.
#[inline]
unsafe fn drop_handle<T>(p: *mut T) {
    if !p.is_null() {
        drop(unsafe { Box::from_raw(p) });
    }
}

/// Read a required C string, returning None on null.
///
/// # Safety
///
/// - `s` must be null or a valid, null-terminated C string with a lifetime
///   at least as long as the returned borrow.
unsafe fn cstr(s: *const c_char) -> Option<&'static str> {
    if s.is_null() {
        return None;
    }
    unsafe { CStr::from_ptr(s) }.to_str().ok()
}

// ---------------------------------------------------------------------------
// Graph
// ---------------------------------------------------------------------------

/// Create an empty graph with the given capacity limits.
///
/// # Safety
///
/// The returned pointer must be freed with [`cet_ffi_graph_free`].
#[unsafe(no_mangle)]
pub extern "C" fn cet_ffi_graph_new(max_vertices: usize, max_edges: usize) -> *mut CetGraph {
    into_handle(CetGraph { inner: Graph::with_capacity(max_vertices, max_edges) })
}

/// Free a graph handle. `NULL` is a no-op.
///
/// # Safety
///
/// - `g` must be null or a pointer previously returned by
///   [`cet_ffi_graph_new`], not yet freed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cet_ffi_graph_free(g: *mut CetGraph) {
    unsafe { drop_handle(g) }
}

/// Insert a vertex. Returns 0 on success, negative on error.
///
/// # Safety
///
/// - `g` must be a valid handle previously returned by [`cet_ffi_graph_new`].
/// - `partition_key` and `event_type` must be null-terminated UTF-8 strings.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cet_ffi_graph_add_vertex(
    g: *mut CetGraph,
    id: i64,
    partition_key: *const c_char,
    event_type: *const c_char,
    event_time_ms: i64,
) -> c_int {
    if g.is_null() {
        return -1;
    }
    let g = unsafe { &mut *g };
    let pkey = match unsafe { cstr(partition_key) } {
        Some(s) => s,
        None => return -2,
    };
    let etype = match unsafe { cstr(event_type) } {
        Some(s) => s,
        None => return -3,
    };
    let v = Vertex {
        id,
        partition_key: pkey.to_string(),
        event_type: etype.to_string(),
        event_time_ms,
    };
    match g.inner.add_vertex(v) {
        Ok(()) => 0,
        Err(_) => -4,
    }
}

/// Insert an edge. Returns 0 on success, negative on error.
///
/// # Safety
///
/// - `g` must be a valid handle previously returned by [`cet_ffi_graph_new`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cet_ffi_graph_add_edge(
    g: *mut CetGraph,
    src: i64,
    dst: i64,
    window_start_ms: i64,
    window_end_ms: i64,
) -> c_int {
    if g.is_null() {
        return -1;
    }
    let g = unsafe { &mut *g };
    let e = Edge { src, dst, window_start_ms, window_end_ms };
    match g.inner.add_edge(e) {
        Ok(()) => 0,
        Err(_) => -2,
    }
}

/// Number of vertices in the graph.
///
/// # Safety
///
/// - `g` must be a valid handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cet_ffi_graph_vertex_count(g: *const CetGraph) -> usize {
    if g.is_null() {
        return 0;
    }
    unsafe { (*g).inner.vertex_count() }
}

/// Number of edges in the graph.
///
/// # Safety
///
/// - `g` must be a valid handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cet_ffi_graph_edge_count(g: *const CetGraph) -> usize {
    if g.is_null() {
        return 0;
    }
    unsafe { (*g).inner.edge_count() }
}

// ---------------------------------------------------------------------------
// Query
// ---------------------------------------------------------------------------

/// Create a query by parsing a CSV pattern. Returns NULL on parse error.
///
/// # Safety
///
/// - `name` and `pattern_csv` must be null-terminated UTF-8 strings.
/// - The returned pointer must be freed with [`cet_ffi_query_free`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cet_ffi_query_parse(
    name: *const c_char,
    pattern_csv: *const c_char,
    within_ms: i64,
    slide_ms: i64,
) -> *mut CetQuery {
    let name = match unsafe { cstr(name) } {
        Some(s) => s,
        None => return ptr::null_mut(),
    };
    let pattern = match unsafe { cstr(pattern_csv) } {
        Some(s) => s,
        None => return ptr::null_mut(),
    };
    match cet_dsl::parse_query(name, pattern, within_ms, slide_ms) {
        Ok(q) => into_handle(CetQuery { inner: q }),
        Err(_) => ptr::null_mut(),
    }
}

/// Create an empty query for programmatic construction. The returned query
/// has no steps; use [`cet_ffi_query_push_step`] to add them.
///
/// # Safety
///
/// - `name` must be a null-terminated UTF-8 string.
/// - The returned pointer must be freed with [`cet_ffi_query_free`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cet_ffi_query_new(name: *const c_char) -> *mut CetQuery {
    let name = match unsafe { cstr(name) } {
        Some(s) => s,
        None => return ptr::null_mut(),
    };
    into_handle(CetQuery { inner: Query::new(name, Vec::new()) })
}

/// Append an event-type step. Returns 0 on success, negative on error.
///
/// # Safety
///
/// - `q` must be a valid handle.
/// - `event_type` must be a null-terminated UTF-8 string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cet_ffi_query_push_step(
    q: *mut CetQuery,
    event_type: *const c_char,
    kleene_plus: c_int,
) -> c_int {
    if q.is_null() {
        return -1;
    }
    let q = unsafe { &mut *q };
    let name = match unsafe { cstr(event_type) } {
        Some(s) => s.to_string(),
        None => return -2,
    };
    if q.inner.seq.len() >= cet_core::caps::MAX_SEQ {
        return -3;
    }
    q.inner.seq.push(EventType { name, kleene_plus: kleene_plus != 0, predicate: None });
    0
}

/// Set the `within_ms` temporal window.
///
/// # Safety
///
/// - `q` must be a valid handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cet_ffi_query_set_within_ms(q: *mut CetQuery, within_ms: i64) {
    if q.is_null() {
        return;
    }
    unsafe { (*q).inner.within_ms = within_ms };
}

/// Set the `skip_till_any_match` flag.
///
/// # Safety
///
/// - `q` must be a valid handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cet_ffi_query_set_skip_till_any_match(q: *mut CetQuery, flag: c_int) {
    if q.is_null() {
        return;
    }
    unsafe { (*q).inner.skip_till_any_match = flag != 0 };
}

/// Free a query handle.
///
/// # Safety
///
/// - `q` must be null or previously returned by a query constructor.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cet_ffi_query_free(q: *mut CetQuery) {
    unsafe { drop_handle(q) }
}

// ---------------------------------------------------------------------------
// Result
// ---------------------------------------------------------------------------

/// Create a result buffer with the given caps.
///
/// # Safety
///
/// - The returned pointer must be freed with [`cet_ffi_result_free`].
#[unsafe(no_mangle)]
pub extern "C" fn cet_ffi_result_new(max_paths: usize, max_path_len: usize) -> *mut CetResult {
    into_handle(CetResult { inner: MatchResult::new(max_paths, max_path_len) })
}

/// Free a result handle.
///
/// # Safety
///
/// - `r` must be null or a valid handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cet_ffi_result_free(r: *mut CetResult) {
    unsafe { drop_handle(r) }
}

/// Number of emitted paths.
///
/// # Safety
///
/// - `r` must be a valid handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cet_ffi_result_path_count(r: *const CetResult) -> usize {
    if r.is_null() {
        return 0;
    }
    unsafe { (*r).inner.paths.len() }
}

/// Length of the path at index `idx`. Returns 0 for invalid `idx`.
///
/// # Safety
///
/// - `r` must be a valid handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cet_ffi_result_path_len(r: *const CetResult, idx: usize) -> usize {
    if r.is_null() {
        return 0;
    }
    let r = unsafe { &*r };
    r.inner.paths.get(idx).map(|p| p.len()).unwrap_or(0)
}

/// Copy up to `out_len` vertex ids from the path at `path_idx` into `out`.
/// Returns the number of ids copied (which may be less than the full path
/// length if `out_len` is smaller).
///
/// # Safety
///
/// - `r` must be a valid handle.
/// - `out` must point to at least `out_len` writable `i64` slots.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cet_ffi_result_copy_path(
    r: *const CetResult,
    path_idx: usize,
    out: *mut i64,
    out_len: usize,
) -> usize {
    if r.is_null() || out.is_null() || out_len == 0 {
        return 0;
    }
    let r = unsafe { &*r };
    let path = match r.inner.paths.get(path_idx) {
        Some(p) => p,
        None => return 0,
    };
    let n = path.len().min(out_len);
    unsafe {
        ptr::copy_nonoverlapping(path.as_ptr(), out, n);
    }
    n
}

// ---------------------------------------------------------------------------
// Stats
// ---------------------------------------------------------------------------

/// Allocate a fresh, zero-initialized stats handle.
///
/// # Safety
///
/// - The returned pointer must be freed with [`cet_ffi_stats_free`].
#[unsafe(no_mangle)]
pub extern "C" fn cet_ffi_stats_new() -> *mut CetStats {
    into_handle(CetStats { inner: ExecStats::default() })
}

/// Free a stats handle.
///
/// # Safety
///
/// - `s` must be null or a valid handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cet_ffi_stats_free(s: *mut CetStats) {
    unsafe { drop_handle(s) }
}

/// Getter: paths_emitted.
///
/// # Safety
///
/// - `s` must be a valid handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cet_ffi_stats_paths_emitted(s: *const CetStats) -> usize {
    if s.is_null() {
        0
    } else {
        unsafe { (*s).inner.paths_emitted }
    }
}

/// Getter: paths_truncated.
///
/// # Safety
///
/// - `s` must be a valid handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cet_ffi_stats_paths_truncated(s: *const CetStats) -> usize {
    if s.is_null() {
        0
    } else {
        unsafe { (*s).inner.paths_truncated }
    }
}

/// Getter: overflow flag (0 or 1).
///
/// # Safety
///
/// - `s` must be a valid handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cet_ffi_stats_overflow(s: *const CetStats) -> c_int {
    if s.is_null() {
        0
    } else {
        if unsafe { (*s).inner.overflow } {
            1
        } else {
            0
        }
    }
}

/// Getter: max_depth_seen.
///
/// # Safety
///
/// - `s` must be a valid handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cet_ffi_stats_max_depth_seen(s: *const CetStats) -> usize {
    if s.is_null() {
        0
    } else {
        unsafe { (*s).inner.max_depth_seen }
    }
}

// ---------------------------------------------------------------------------
// Runtime config / stats
// ---------------------------------------------------------------------------

/// Create a runtime config with defaults (`native_threads = 1`).
///
/// # Safety
///
/// - The returned pointer must be freed with [`cet_ffi_runtime_config_free`].
#[unsafe(no_mangle)]
pub extern "C" fn cet_ffi_runtime_config_new() -> *mut CetRuntimeConfig {
    into_handle(CetRuntimeConfig { inner: RuntimeConfig::default() })
}

/// Free a runtime config handle.
///
/// # Safety
///
/// - `c` must be null or a valid handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cet_ffi_runtime_config_free(c: *mut CetRuntimeConfig) {
    unsafe { drop_handle(c) }
}

/// Set `native_threads`.
///
/// # Safety
///
/// - `c` must be a valid handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cet_ffi_runtime_config_set_native_threads(
    c: *mut CetRuntimeConfig,
    n: usize,
) {
    if c.is_null() {
        return;
    }
    unsafe { (*c).inner.native_threads = n };
}

/// Allocate a runtime-stats handle.
///
/// # Safety
///
/// - The returned pointer must be freed with [`cet_ffi_runtime_stats_free`].
#[unsafe(no_mangle)]
pub extern "C" fn cet_ffi_runtime_stats_new() -> *mut CetRuntimeStats {
    into_handle(CetRuntimeStats { inner: RuntimeStats::default() })
}

/// Free a runtime-stats handle.
///
/// # Safety
///
/// - `s` must be null or a valid handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cet_ffi_runtime_stats_free(s: *mut CetRuntimeStats) {
    unsafe { drop_handle(s) }
}

/// Getter: native_threads_used.
///
/// # Safety
///
/// - `s` must be a valid handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cet_ffi_runtime_stats_native_threads_used(
    s: *const CetRuntimeStats,
) -> usize {
    if s.is_null() {
        0
    } else {
        unsafe { (*s).inner.native_threads_used }
    }
}

/// Getter: parallel_enabled (0 or 1).
///
/// # Safety
///
/// - `s` must be a valid handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cet_ffi_runtime_stats_parallel_enabled(
    s: *const CetRuntimeStats,
) -> c_int {
    if s.is_null() {
        0
    } else {
        if unsafe { (*s).inner.parallel_enabled } {
            1
        } else {
            0
        }
    }
}

// ---------------------------------------------------------------------------
// Execute
// ---------------------------------------------------------------------------

/// Run MCET. Returns 0 on success, negative on error.
///
/// # Safety
///
/// - All handles must be valid and non-null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cet_ffi_execute_mcet(
    g: *const CetGraph,
    q: *const CetQuery,
    out: *mut CetResult,
    stats: *mut CetStats,
) -> c_int {
    if g.is_null() || q.is_null() || out.is_null() || stats.is_null() {
        return -1;
    }
    let g = unsafe { &(*g).inner };
    let q = unsafe { &(*q).inner };
    let out = unsafe { &mut (*out).inner };
    let stats = unsafe { &mut (*stats).inner };
    exec::execute_mcet(g, q, out, stats);
    0
}

/// Run TCET. Returns 0 on success, negative on error.
///
/// # Safety
///
/// - All handles must be valid and non-null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cet_ffi_execute_tcet(
    g: *const CetGraph,
    q: *const CetQuery,
    out: *mut CetResult,
    stats: *mut CetStats,
) -> c_int {
    if g.is_null() || q.is_null() || out.is_null() || stats.is_null() {
        return -1;
    }
    let g = unsafe { &(*g).inner };
    let q = unsafe { &(*q).inner };
    let out = unsafe { &mut (*out).inner };
    let stats = unsafe { &mut (*stats).inner };
    exec::execute_tcet(g, q, out, stats);
    0
}

/// Run HCET. Returns 0 on success, negative on error.
///
/// # Safety
///
/// - All handles must be valid and non-null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cet_ffi_execute_hcet(
    g: *const CetGraph,
    q: *const CetQuery,
    switch_depth: usize,
    out: *mut CetResult,
    stats: *mut CetStats,
) -> c_int {
    if g.is_null() || q.is_null() || out.is_null() || stats.is_null() {
        return -1;
    }
    let g = unsafe { &(*g).inner };
    let q = unsafe { &(*q).inner };
    let out = unsafe { &mut (*out).inner };
    let stats = unsafe { &mut (*stats).inner };
    exec::execute_hcet(g, q, switch_depth, out, stats);
    0
}

/// Run HCET in parallel. Returns 0 on success, negative on error.
///
/// # Safety
///
/// - All handles must be valid and non-null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cet_ffi_execute_hcet_parallel(
    g: *const CetGraph,
    q: *const CetQuery,
    switch_depth: usize,
    cfg: *const CetRuntimeConfig,
    out: *mut CetResult,
    stats: *mut CetStats,
    rt: *mut CetRuntimeStats,
) -> c_int {
    if g.is_null()
        || q.is_null()
        || cfg.is_null()
        || out.is_null()
        || stats.is_null()
        || rt.is_null()
    {
        return -1;
    }
    let g = unsafe { &(*g).inner };
    let q = unsafe { &(*q).inner };
    let cfg = unsafe { &(*cfg).inner };
    let out = unsafe { &mut (*out).inner };
    let stats = unsafe { &mut (*stats).inner };
    let rt = unsafe { &mut (*rt).inner };
    execute_hcet_parallel(g, q, switch_depth, cfg, out, stats, rt);
    0
}

// ---------------------------------------------------------------------------
// Round-trip smoke tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;

    #[test]
    fn round_trip_smoke_test() {
        unsafe {
            let g = cet_ffi_graph_new(16, 16);
            assert!(!g.is_null());
            let pk = CString::new("p").unwrap();
            let a = CString::new("A").unwrap();
            let b = CString::new("B").unwrap();
            let c = CString::new("C").unwrap();
            assert_eq!(cet_ffi_graph_add_vertex(g, 1, pk.as_ptr(), a.as_ptr(), 1), 0);
            assert_eq!(cet_ffi_graph_add_vertex(g, 2, pk.as_ptr(), b.as_ptr(), 2), 0);
            assert_eq!(cet_ffi_graph_add_vertex(g, 3, pk.as_ptr(), c.as_ptr(), 3), 0);
            assert_eq!(cet_ffi_graph_add_edge(g, 1, 2, 0, 0), 0);
            assert_eq!(cet_ffi_graph_add_edge(g, 2, 3, 0, 0), 0);
            assert_eq!(cet_ffi_graph_vertex_count(g), 3);

            let name = CString::new("q").unwrap();
            let pat = CString::new("A,B,C").unwrap();
            let q = cet_ffi_query_parse(name.as_ptr(), pat.as_ptr(), -1, 0);
            assert!(!q.is_null());

            let out = cet_ffi_result_new(64, 16);
            let stats = cet_ffi_stats_new();
            assert_eq!(cet_ffi_execute_tcet(g, q, out, stats), 0);

            assert_eq!(cet_ffi_result_path_count(out), 1);
            assert_eq!(cet_ffi_result_path_len(out, 0), 3);
            let mut buf = [0i64; 3];
            let copied = cet_ffi_result_copy_path(out, 0, buf.as_mut_ptr(), buf.len());
            assert_eq!(copied, 3);
            assert_eq!(buf, [1, 2, 3]);

            cet_ffi_stats_free(stats);
            cet_ffi_result_free(out);
            cet_ffi_query_free(q);
            cet_ffi_graph_free(g);
        }
    }

    #[test]
    fn null_handles_are_safe() {
        unsafe {
            cet_ffi_graph_free(ptr::null_mut());
            cet_ffi_query_free(ptr::null_mut());
            cet_ffi_result_free(ptr::null_mut());
            cet_ffi_stats_free(ptr::null_mut());
            assert_eq!(cet_ffi_graph_vertex_count(ptr::null()), 0);
            assert_eq!(cet_ffi_result_path_count(ptr::null()), 0);
        }
    }

    #[test]
    fn programmatic_query_construction() {
        unsafe {
            let name = CString::new("q").unwrap();
            let q = cet_ffi_query_new(name.as_ptr());
            let a = CString::new("A").unwrap();
            let b = CString::new("B").unwrap();
            assert_eq!(cet_ffi_query_push_step(q, a.as_ptr(), 0), 0);
            assert_eq!(cet_ffi_query_push_step(q, b.as_ptr(), 1), 0);
            cet_ffi_query_set_within_ms(q, 60_000);
            cet_ffi_query_set_skip_till_any_match(q, 0);
            cet_ffi_query_free(q);
        }
    }
}
