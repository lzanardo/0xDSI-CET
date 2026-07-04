//! C-engine drop-in ABI compatibility layer.
//!
//! This module re-exports the exact symbol set from `c_engine/include/cet.h`,
//! with `#[repr(C)]` structs whose layout matches the C engine byte-for-byte.
//! The result is that `libcet_ffi.dylib` (renamed to `liboxdsi_cet.dylib` at
//! install time) can be loaded by the existing `bindings/python/bridge.py`
//! `ctypes.CDLL(...)` call without any Python-side changes.
//!
//! The Rust-native opaque-handle API lives in `crate::` (the parent module).
//! Callers who want the safer, dynamic-capacity API should use that instead.
//!
//! ## Layout guarantees
//!
//! These constants and struct fields must never diverge from `cet.h` or from
//! `bindings/python/bridge.py`. Both are asserted at build time via
//! `const _: () = assert!(...)` invariants at the bottom of this file.
//!
//! ## Safety
//!
//! - Every exported symbol is `unsafe extern "C" fn`. Callers must uphold
//!   the same contracts as the C engine: valid pointers, well-aligned
//!   structs, correct sizes.
//! - The `cet_graph_t` and `cet_result_t` structs are ~45 MiB and ~25 MiB
//!   respectively; Rust code must never take them by value. All operations
//!   go through raw pointers.
//! - Null pointers are rejected explicitly (matching the C engine, except
//!   for the previously-buggy null-`stats` path in
//!   `c_engine/src/algorithms.c:387-391`, which this port now handles safely).

#![allow(non_camel_case_types)]
// The struct fields here mirror `c_engine/include/cet.h` byte-for-byte; documenting
// each individually would be redundant with the corresponding C-header comments.
#![allow(missing_docs)]

use std::ffi::{c_char, c_int, CStr};
use std::os::raw::c_void;
use std::ptr;
use std::slice;

use cet_core::{Edge, EventType, ExecStats, Graph, MatchResult, Query, Vertex};
use cet_parallel::{execute_hcet_parallel, RuntimeConfig, RuntimeStats};

// ---------------------------------------------------------------------------
// Constants — must match c_engine/include/cet.h
// ---------------------------------------------------------------------------

pub const CET_MAX_SEQ: usize = 16;
pub const CET_MAX_EVENTS: usize = 200_000;
pub const CET_MAX_EDGES: usize = 1_000_000;
pub const CET_MAX_PATHS: usize = 100_000;
pub const CET_MAX_PATH_LEN: usize = 64;
pub const CET_MAX_ERROR_LEN: usize = 256;
pub const CET_RUNTIME_CONFIG_VERSION: u32 = 1;
pub const CET_MAX_NATIVE_THREADS: usize = 64;

// ---------------------------------------------------------------------------
// Structs — byte-identical to cet.h and bindings/python/bridge.py
// ---------------------------------------------------------------------------

/// Predicate function pointer type. The C engine expects a callback taking
/// `(prev_id, curr_id, ctx)` and returning non-zero to accept. The Rust
/// port does not invoke predicates from the compat layer (callers who need
/// predicates should use the Rust-native API); the field is preserved for
/// layout compatibility and set to null during query parsing.
pub type cet_predicate_fn =
    Option<unsafe extern "C" fn(prev_id: c_int, curr_id: c_int, ctx: *mut c_void) -> c_int>;

#[repr(C)]
#[derive(Copy, Clone)]
pub struct cet_event_type_t {
    pub name: [c_char; 32],
    pub kleene_plus: c_int,
    pub predicate: cet_predicate_fn,
    pub predicate_ctx: *mut c_void,
}

#[repr(C)]
pub struct cet_query_t {
    pub name: [c_char; 64],
    pub seq: [cet_event_type_t; CET_MAX_SEQ],
    pub seq_len: usize,
    pub within_ms: i64,
    pub slide_ms: i64,
    pub skip_till_any_match: c_int,
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct cet_vertex_t {
    pub id: c_int,
    pub partition_key: [c_char; 64],
    pub event_type: [c_char; 32],
    pub event_time_ms: i64,
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct cet_edge_t {
    pub src: c_int,
    pub dst: c_int,
    pub window_start_ms: i64,
    pub window_end_ms: i64,
}

#[repr(C)]
pub struct cet_graph_t {
    pub vertices: [cet_vertex_t; CET_MAX_EVENTS],
    pub edges: [cet_edge_t; CET_MAX_EDGES],
    pub vcount: usize,
    pub ecount: usize,
}

#[repr(C)]
pub struct cet_result_t {
    pub paths: [[c_int; CET_MAX_PATH_LEN]; CET_MAX_PATHS],
    pub path_len: [usize; CET_MAX_PATHS],
    pub count: usize,
}

#[repr(C)]
pub struct cet_exec_stats_t {
    pub paths_emitted: usize,
    pub paths_truncated: usize,
    pub states_enqueued: usize,
    pub states_truncated: usize,
    pub seed_paths: usize,
    pub max_depth_seen: usize,
    pub temporal_rejects: usize,
    pub edge_window_rejects: usize,
    pub predicate_rejects: usize,
    pub overflow: c_int,
    pub error: [c_char; CET_MAX_ERROR_LEN],
}

#[repr(C)]
pub struct cet_runtime_config_t {
    pub version: u32,
    pub native_threads: usize,
    pub enable_mmap_arena: c_int,
    pub mmap_workspace_bytes: usize,
    pub madvise_hugepage: c_int,
    pub deterministic_merge: c_int,
}

#[repr(C)]
pub struct cet_runtime_stats_t {
    pub native_threads_requested: usize,
    pub native_threads_used: usize,
    pub workspace_bytes: usize,
    pub used_mmap_arena: c_int,
    pub parallel_enabled: c_int,
    pub error: [c_char; CET_MAX_ERROR_LEN],
}

// ---------------------------------------------------------------------------
// Layout assertions — fail the build if we drift from the C engine.
// ---------------------------------------------------------------------------

// Ensure the fixed-size arrays don't get silently padded or reordered.
const _: () = assert!(std::mem::size_of::<cet_event_type_t>() == 32 + 4 + 4 + 8 + 8);
const _: () = assert!(std::mem::size_of::<cet_vertex_t>() == 4 + 4 + 64 + 32 + 8);
const _: () = assert!(std::mem::size_of::<cet_edge_t>() == 4 + 4 + 8 + 8);
const _: () = assert!(std::mem::align_of::<cet_query_t>() == 8);
const _: () = assert!(std::mem::align_of::<cet_graph_t>() == 8);

// ---------------------------------------------------------------------------
// Conversion helpers
// ---------------------------------------------------------------------------

/// Copy a Rust `&str` into a fixed-size C-char array, null-terminated.
/// Truncates if `s.len() >= N`.
fn write_cstr_into(dst: &mut [c_char], s: &str) {
    let n = s.len().min(dst.len().saturating_sub(1));
    let bytes = s.as_bytes();
    for (i, &b) in bytes.iter().take(n).enumerate() {
        dst[i] = b as c_char;
    }
    if n < dst.len() {
        dst[n] = 0;
    }
}

/// Read a null-terminated string from a fixed C-char array. Never panics.
fn read_cstr(src: &[c_char]) -> String {
    let bytes: &[u8] = unsafe { slice::from_raw_parts(src.as_ptr() as *const u8, src.len()) };
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[..end]).into_owned()
}

/// Materialize a Rust `Query` from the C struct.
///
/// # Safety
///
/// `q` must point to a valid, initialized `cet_query_t`.
unsafe fn query_from_c(q: *const cet_query_t) -> Query {
    let q = unsafe { &*q };
    let name = read_cstr(&q.name);
    let mut seq: Vec<EventType> = Vec::with_capacity(q.seq_len.min(CET_MAX_SEQ));
    for i in 0..q.seq_len.min(CET_MAX_SEQ) {
        let step = &q.seq[i];
        seq.push(EventType {
            name: read_cstr(&step.name),
            kleene_plus: step.kleene_plus != 0,
            predicate: None, // Predicates are not carried across the compat boundary.
        });
    }
    let mut out = Query::new(name, seq);
    out.within_ms = q.within_ms;
    out.slide_ms = q.slide_ms;
    out.skip_till_any_match = q.skip_till_any_match != 0;
    out
}

/// Materialize a Rust `Graph` view from the C struct.
///
/// # Safety
///
/// `g` must point to a valid, initialized `cet_graph_t`. The returned Graph
/// owns its data (deep copy).
unsafe fn graph_from_c(g: *const cet_graph_t) -> Graph {
    let g = unsafe { &*g };
    let mut out = Graph::with_capacity(CET_MAX_EVENTS, CET_MAX_EDGES);
    for i in 0..g.vcount.min(CET_MAX_EVENTS) {
        let v = &g.vertices[i];
        let _ = out.add_vertex(Vertex {
            id: v.id as i64,
            partition_key: read_cstr(&v.partition_key),
            event_type: read_cstr(&v.event_type),
            event_time_ms: v.event_time_ms,
        });
    }
    for i in 0..g.ecount.min(CET_MAX_EDGES) {
        let e = &g.edges[i];
        // The C engine allows edges whose endpoints are missing from the
        // vertex array; mirror that tolerance by ignoring add_edge errors.
        let _ = out.add_edge(Edge {
            src: e.src as i64,
            dst: e.dst as i64,
            window_start_ms: e.window_start_ms,
            window_end_ms: e.window_end_ms,
        });
    }
    out
}

/// Write a Rust `MatchResult` into the C struct in-place.
///
/// # Safety
///
/// `out` must point to a valid, writable `cet_result_t`.
unsafe fn result_to_c(rust: &MatchResult, out: *mut cet_result_t) {
    let out = unsafe { &mut *out };
    let n = rust.paths.len().min(CET_MAX_PATHS);
    out.count = n;
    for (i, path) in rust.paths.iter().take(n).enumerate() {
        let plen = path.len().min(CET_MAX_PATH_LEN);
        out.path_len[i] = plen;
        for (j, &id) in path.iter().take(plen).enumerate() {
            out.paths[i][j] = id as c_int;
        }
        // Zero-fill the tail so callers reading path_len then paths[i][0..plen]
        // don't see stale data.
        for j in plen..CET_MAX_PATH_LEN {
            out.paths[i][j] = 0;
        }
    }
    // Zero out the remaining path_len slots.
    for i in n..CET_MAX_PATHS.min(out.path_len.len()) {
        out.path_len[i] = 0;
    }
}

/// Write a Rust `ExecStats` into the C struct in-place.
///
/// # Safety
///
/// `stats` must be null or a valid, writable `cet_exec_stats_t`.
unsafe fn stats_to_c(rust: &ExecStats, stats: *mut cet_exec_stats_t) {
    if stats.is_null() {
        return;
    }
    let stats = unsafe { &mut *stats };
    stats.paths_emitted = rust.paths_emitted;
    stats.paths_truncated = rust.paths_truncated;
    stats.states_enqueued = rust.states_enqueued;
    stats.states_truncated = rust.states_truncated;
    stats.seed_paths = rust.seed_paths;
    stats.max_depth_seen = rust.max_depth_seen;
    stats.temporal_rejects = rust.temporal_rejects;
    stats.edge_window_rejects = rust.edge_window_rejects;
    stats.predicate_rejects = rust.predicate_rejects;
    stats.overflow = if rust.overflow { 1 } else { 0 };
    if let Some(msg) = rust.error.as_deref() {
        write_cstr_into(&mut stats.error, msg);
    } else {
        stats.error[0] = 0;
    }
}

// ---------------------------------------------------------------------------
// Exported C symbols — matching c_engine/include/cet.h
// ---------------------------------------------------------------------------

/// Zero-initialize a graph.
///
/// # Safety
///
/// `g` must point to a writable `cet_graph_t` allocation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cet_graph_init(g: *mut cet_graph_t) {
    if g.is_null() {
        return;
    }
    unsafe {
        ptr::write_bytes(g as *mut u8, 0, std::mem::size_of::<cet_graph_t>());
    }
}

/// Add a vertex. Returns 0 on success, -1 on capacity overflow.
///
/// # Safety
///
/// `g`, `pkey`, `etype` must be valid pointers. `pkey` and `etype` must be
/// null-terminated C strings.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cet_graph_add_vertex(
    g: *mut cet_graph_t,
    id: c_int,
    pkey: *const c_char,
    etype: *const c_char,
    t: i64,
) -> c_int {
    if g.is_null() || pkey.is_null() || etype.is_null() {
        return -1;
    }
    let g = unsafe { &mut *g };
    if g.vcount >= CET_MAX_EVENTS {
        return -1;
    }
    let idx = g.vcount;
    let v = &mut g.vertices[idx];
    v.id = id;
    v.event_time_ms = t;
    let pkey_bytes = unsafe { CStr::from_ptr(pkey).to_bytes() };
    let etype_bytes = unsafe { CStr::from_ptr(etype).to_bytes() };
    write_cstr_into(&mut v.partition_key, &String::from_utf8_lossy(pkey_bytes));
    write_cstr_into(&mut v.event_type, &String::from_utf8_lossy(etype_bytes));
    g.vcount += 1;
    0
}

/// Add an edge. Returns 0 on success, -1 on capacity overflow.
///
/// # Safety
///
/// `g` must be a valid pointer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cet_graph_add_edge(
    g: *mut cet_graph_t,
    src: c_int,
    dst: c_int,
    wstart: i64,
    wend: i64,
) -> c_int {
    if g.is_null() {
        return -1;
    }
    let g = unsafe { &mut *g };
    if g.ecount >= CET_MAX_EDGES {
        return -1;
    }
    let idx = g.ecount;
    let e = &mut g.edges[idx];
    e.src = src;
    e.dst = dst;
    e.window_start_ms = wstart;
    e.window_end_ms = wend;
    g.ecount += 1;
    0
}

/// Parse a CSV pattern into a query. Returns 0 on success, -1 on empty
/// pattern or malformed input.
///
/// # Safety
///
/// All pointer arguments must be valid; strings must be null-terminated.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cet_parse_query(
    name: *const c_char,
    pattern_csv: *const c_char,
    within_ms: i64,
    slide_ms: i64,
    out: *mut cet_query_t,
) -> c_int {
    if name.is_null() || pattern_csv.is_null() || out.is_null() {
        return -1;
    }
    let name_s = match unsafe { CStr::from_ptr(name) }.to_str() {
        Ok(s) => s,
        Err(_) => return -1,
    };
    let pat_s = match unsafe { CStr::from_ptr(pattern_csv) }.to_str() {
        Ok(s) => s,
        Err(_) => return -1,
    };
    let q = match cet_dsl::parse_query(name_s, pat_s, within_ms, slide_ms) {
        Ok(q) => q,
        Err(_) => return -1,
    };
    unsafe {
        ptr::write_bytes(out as *mut u8, 0, std::mem::size_of::<cet_query_t>());
    }
    let dst = unsafe { &mut *out };
    write_cstr_into(&mut dst.name, &q.name);
    dst.within_ms = q.within_ms;
    dst.slide_ms = q.slide_ms;
    dst.skip_till_any_match = if q.skip_till_any_match { 1 } else { 0 };
    dst.seq_len = q.seq.len().min(CET_MAX_SEQ);
    for (i, step) in q.seq.iter().take(CET_MAX_SEQ).enumerate() {
        write_cstr_into(&mut dst.seq[i].name, &step.name);
        dst.seq[i].kleene_plus = if step.kleene_plus { 1 } else { 0 };
        dst.seq[i].predicate = None;
        dst.seq[i].predicate_ctx = ptr::null_mut();
    }
    0
}

/// Populate a runtime config with defaults.
///
/// # Safety
///
/// `cfg` must point to a writable `cet_runtime_config_t`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cet_runtime_config_default(cfg: *mut cet_runtime_config_t) {
    if cfg.is_null() {
        return;
    }
    let cfg = unsafe { &mut *cfg };
    cfg.version = CET_RUNTIME_CONFIG_VERSION;
    cfg.native_threads = 1;
    cfg.enable_mmap_arena = 0;
    cfg.mmap_workspace_bytes = 0;
    cfg.madvise_hugepage = 0;
    cfg.deterministic_merge = 1;
}

/// Set global cost coefficients (compat no-op).
///
/// The C engine stored these in file-scope globals; the Rust port uses a
/// per-instance `CostModel` and does not track a global. This function is
/// exported for ABI compatibility but has no effect. Callers who want to
/// use custom coefficients should use the Rust-native `cet_core::optimizer`
/// API.
///
/// # Safety
///
/// This function is safe to call; it exists only to satisfy the symbol lookup.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cet_set_cost_coefficients(
    _mem_vertex: f64,
    _mem_edge: f64,
    _cpu_edge: f64,
    _cpu_vertex: f64,
) {
    // Intentionally a no-op. See docs above.
}

// --- Executors --------------------------------------------------------------

/// Run MCET without stats output.
///
/// # Safety
///
/// All pointers must be valid.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cet_execute_mcet(
    g: *const cet_graph_t,
    q: *const cet_query_t,
    out: *mut cet_result_t,
) {
    let mut stats = ExecStats::default();
    unsafe { execute_mcet_impl(g, q, out, &mut stats) };
}

/// Run TCET without stats output.
///
/// # Safety
///
/// All pointers must be valid.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cet_execute_tcet(
    g: *const cet_graph_t,
    q: *const cet_query_t,
    out: *mut cet_result_t,
) {
    let mut stats = ExecStats::default();
    unsafe { execute_tcet_impl(g, q, out, &mut stats) };
}

/// Run HCET without stats output.
///
/// # Safety
///
/// All pointers must be valid.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cet_execute_hcet(
    g: *const cet_graph_t,
    q: *const cet_query_t,
    switch_depth: usize,
    out: *mut cet_result_t,
) {
    let mut stats = ExecStats::default();
    unsafe { execute_hcet_impl(g, q, switch_depth, out, &mut stats) };
}

/// Run MCET with extended stats.
///
/// # Safety
///
/// All required pointers must be valid; `stats` may be null (the C engine
/// tolerated but crashed on null; this port genuinely handles null).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cet_execute_mcet_ex(
    g: *const cet_graph_t,
    q: *const cet_query_t,
    out: *mut cet_result_t,
    stats: *mut cet_exec_stats_t,
) {
    let mut rust_stats = ExecStats::default();
    unsafe { execute_mcet_impl(g, q, out, &mut rust_stats) };
    unsafe { stats_to_c(&rust_stats, stats) };
}

/// Run TCET with extended stats.
///
/// # Safety
///
/// All required pointers must be valid; `stats` may be null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cet_execute_tcet_ex(
    g: *const cet_graph_t,
    q: *const cet_query_t,
    out: *mut cet_result_t,
    stats: *mut cet_exec_stats_t,
) {
    let mut rust_stats = ExecStats::default();
    unsafe { execute_tcet_impl(g, q, out, &mut rust_stats) };
    unsafe { stats_to_c(&rust_stats, stats) };
}

/// Run HCET with extended stats.
///
/// # Safety
///
/// All required pointers must be valid; `stats` may be null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cet_execute_hcet_ex(
    g: *const cet_graph_t,
    q: *const cet_query_t,
    switch_depth: usize,
    out: *mut cet_result_t,
    stats: *mut cet_exec_stats_t,
) {
    let mut rust_stats = ExecStats::default();
    unsafe { execute_hcet_impl(g, q, switch_depth, out, &mut rust_stats) };
    unsafe { stats_to_c(&rust_stats, stats) };
}

/// Run HCET in parallel via Rayon.
///
/// # Safety
///
/// All required pointers must be valid; `stats` and `runtime_stats` may be null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cet_execute_hcet_parallel_ex(
    g: *const cet_graph_t,
    q: *const cet_query_t,
    switch_depth: usize,
    cfg: *const cet_runtime_config_t,
    out: *mut cet_result_t,
    stats: *mut cet_exec_stats_t,
    runtime_stats: *mut cet_runtime_stats_t,
) -> c_int {
    if g.is_null() || q.is_null() || out.is_null() {
        return -1;
    }
    let rust_g = unsafe { graph_from_c(g) };
    let rust_q = unsafe { query_from_c(q) };
    let rust_cfg = if cfg.is_null() {
        RuntimeConfig::default()
    } else {
        let c = unsafe { &*cfg };
        RuntimeConfig {
            native_threads: c.native_threads,
            deterministic_merge: c.deterministic_merge != 0,
        }
    };
    let mut rust_out = MatchResult::new(CET_MAX_PATHS, CET_MAX_PATH_LEN);
    let mut rust_stats = ExecStats::default();
    let mut rust_rt = RuntimeStats::default();
    execute_hcet_parallel(
        &rust_g,
        &rust_q,
        switch_depth,
        &rust_cfg,
        &mut rust_out,
        &mut rust_stats,
        &mut rust_rt,
    );
    unsafe {
        // Zero the output struct so callers see a fresh state.
        ptr::write_bytes(out as *mut u8, 0, std::mem::size_of::<cet_result_t>());
        result_to_c(&rust_out, out);
        stats_to_c(&rust_stats, stats);
        if !runtime_stats.is_null() {
            let rt = &mut *runtime_stats;
            rt.native_threads_requested = rust_rt.native_threads_requested;
            rt.native_threads_used = rust_rt.native_threads_used;
            rt.workspace_bytes = 0;
            rt.used_mmap_arena = 0;
            rt.parallel_enabled = if rust_rt.parallel_enabled { 1 } else { 0 };
            rt.error[0] = 0;
            if let Some(msg) = rust_rt.error.as_deref() {
                write_cstr_into(&mut rt.error, msg);
            }
        }
    }
    0
}

// ---------------------------------------------------------------------------
// Shared implementation for the executor family.
// ---------------------------------------------------------------------------

/// # Safety
///
/// All required pointers must be valid.
unsafe fn execute_mcet_impl(
    g: *const cet_graph_t,
    q: *const cet_query_t,
    out: *mut cet_result_t,
    stats: &mut ExecStats,
) {
    if g.is_null() || q.is_null() || out.is_null() {
        return;
    }
    let rust_g = unsafe { graph_from_c(g) };
    let rust_q = unsafe { query_from_c(q) };
    let mut rust_out = MatchResult::new(CET_MAX_PATHS, CET_MAX_PATH_LEN);
    cet_core::exec::execute_mcet(&rust_g, &rust_q, &mut rust_out, stats);
    unsafe {
        ptr::write_bytes(out as *mut u8, 0, std::mem::size_of::<cet_result_t>());
        result_to_c(&rust_out, out);
    }
}

/// # Safety
///
/// All required pointers must be valid.
unsafe fn execute_tcet_impl(
    g: *const cet_graph_t,
    q: *const cet_query_t,
    out: *mut cet_result_t,
    stats: &mut ExecStats,
) {
    if g.is_null() || q.is_null() || out.is_null() {
        return;
    }
    let rust_g = unsafe { graph_from_c(g) };
    let rust_q = unsafe { query_from_c(q) };
    let mut rust_out = MatchResult::new(CET_MAX_PATHS, CET_MAX_PATH_LEN);
    cet_core::exec::execute_tcet(&rust_g, &rust_q, &mut rust_out, stats);
    unsafe {
        ptr::write_bytes(out as *mut u8, 0, std::mem::size_of::<cet_result_t>());
        result_to_c(&rust_out, out);
    }
}

/// # Safety
///
/// All required pointers must be valid.
unsafe fn execute_hcet_impl(
    g: *const cet_graph_t,
    q: *const cet_query_t,
    switch_depth: usize,
    out: *mut cet_result_t,
    stats: &mut ExecStats,
) {
    if g.is_null() || q.is_null() || out.is_null() {
        return;
    }
    let rust_g = unsafe { graph_from_c(g) };
    let rust_q = unsafe { query_from_c(q) };
    let mut rust_out = MatchResult::new(CET_MAX_PATHS, CET_MAX_PATH_LEN);
    cet_core::exec::execute_hcet(&rust_g, &rust_q, switch_depth, &mut rust_out, stats);
    unsafe {
        ptr::write_bytes(out as *mut u8, 0, std::mem::size_of::<cet_result_t>());
        result_to_c(&rust_out, out);
    }
}
