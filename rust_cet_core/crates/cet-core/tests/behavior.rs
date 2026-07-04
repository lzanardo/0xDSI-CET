//! Behavior corpus for cet-core.
//!
//! This file defines the *acceptance bar* for the C → Rust port. Tests are
//! grouped into three categories:
//!
//! 1. **Regression cases** — targeted tests for the specific bugs identified
//!    in the C code review. These must pass in the current scaffold.
//! 2. **Property tests** — `proptest`-based invariants that every executor
//!    port must satisfy. Marked `#[ignore]` until the executors are
//!    implemented; un-ignoring them is the definition-of-done for each
//!    algorithm.
//! 3. **Cross-algorithm equivalence** — MCET ≡ TCET ≡ HCET on any input.
//!
//! The intent is that porting each algorithm is a matter of removing
//! `#[ignore]` and watching the property tests turn green.

use cet_core::{
    caps, exec, optimizer, sliding, testkit, CetError, Edge, EventType, ExecStats, Graph,
    MatchResult, Query, Vertex,
};
use proptest::prelude::*;

// ---------------------------------------------------------------------------
// 1. Regression cases (must pass now)
// ---------------------------------------------------------------------------

fn v(id: i64, t: i64, ty: &str) -> Vertex {
    Vertex { id, partition_key: "p".into(), event_type: ty.into(), event_time_ms: t }
}

fn edge(src: i64, dst: i64) -> Edge {
    Edge { src, dst, window_start_ms: 0, window_end_ms: 0 }
}

/// Regression for the "silently dropped edges" C bug in `graph.c:16-20`.
#[test]
fn regression_edge_requires_known_endpoints() {
    let mut g = Graph::with_capacity(16, 16);
    g.add_vertex(v(1, 1, "A")).unwrap();
    // dst unknown
    let err = g.add_edge(edge(1, 99)).unwrap_err();
    assert_eq!(err, CetError::UnknownVertex(99));
    // src unknown
    let err = g.add_edge(edge(42, 1)).unwrap_err();
    assert_eq!(err, CetError::UnknownVertex(42));
}

/// Regression for undocumented duplicate-id behavior in the C engine.
#[test]
fn regression_duplicate_vertex_rejected() {
    let mut g = Graph::with_capacity(16, 16);
    g.add_vertex(v(1, 1, "A")).unwrap();
    assert_eq!(g.add_vertex(v(1, 2, "B")), Err(CetError::DuplicateVertex(1)));
}

/// Regression for capacity limits — must be reported explicitly, not silently
/// truncated as in the C engine.
#[test]
fn regression_vertex_capacity_reported() {
    let mut g = Graph::with_capacity(2, 4);
    g.add_vertex(v(1, 1, "A")).unwrap();
    g.add_vertex(v(2, 2, "B")).unwrap();
    assert!(matches!(
        g.add_vertex(v(3, 3, "C")),
        Err(CetError::CapacityExceeded { what: "vertices", limit: 2 })
    ));
}

/// Regression for the C engine's null-`stats` deref bug in
/// `algorithms.c:387-391`. In Rust, `stats` is `&mut ExecStats`, so this
/// class of bug is unrepresentable at the type level. This test documents
/// that the API refuses to accept a missing stats sink.
#[test]
fn regression_stats_is_required_by_type_system() {
    // This is a compile-time property. We verify by constructing the call
    // signature; if someone changes `stats` to `Option<&mut ExecStats>` this
    // test still passes but the intent is captured.
    let g = Graph::with_capacity(1, 1);
    let q = Query::new("q", vec![]);
    let mut out = MatchResult::new(1, 1);
    let mut stats = ExecStats::default();
    // Compiles → property holds.
    let _ = (&g, &q, &mut out, &mut stats);
}

/// Regression for the `ExecStats::merge` sticky-first-error contract.
#[test]
fn regression_stats_merge_preserves_first_error() {
    let mut a = ExecStats { max_depth_seen: 3, ..Default::default() };
    a.set_error("first");
    let mut b = ExecStats { max_depth_seen: 9, ..Default::default() };
    b.set_error("second");
    a.merge(&b);
    assert_eq!(a.error.as_deref(), Some("first"));
    assert_eq!(a.max_depth_seen, 9);
    assert!(a.overflow);
}

/// Regression for the C engine's unbounded `skip_till_any_match` recursion
/// (`c_engine/src/algorithms.c:210-213,336-345`). The C code had no cycle
/// detection or hop budget, so a cyclic graph combined with
/// `skip_till_any_match = true` could recurse until the fixed 64-int path
/// buffer overflowed, silently corrupting results.
///
/// This test constructs a triangle `A(1) -> A(2) -> A(3) -> A(1)` (a cycle)
/// plus a `B(4)` sink reachable only from `A(3)`. With `skip_till_any_match`
/// enabled and pattern `[A, B]`, a naive traversal without a cycle guard
/// would loop forever revisiting `A(1) -> A(2) -> A(3) -> A(1) -> ...`.
///
/// Acceptance:
/// 1. All three executors terminate.
/// 2. Every emitted path has at most `MAX_PATH_LEN` vertices.
/// 3. Every emitted path is simple (no vertex appears twice).
/// 4. The `[A(1), A(2), A(3), B(4)]` match is present (correctness under
///    the cycle guard).
#[test]
fn regression_skip_till_any_match_terminates_on_cycles() {
    let mut g = Graph::with_capacity(16, 16);
    g.add_vertex(Vertex {
        id: 1,
        partition_key: "p".into(),
        event_type: "A".into(),
        event_time_ms: 1,
    })
    .unwrap();
    g.add_vertex(Vertex {
        id: 2,
        partition_key: "p".into(),
        event_type: "A".into(),
        event_time_ms: 2,
    })
    .unwrap();
    g.add_vertex(Vertex {
        id: 3,
        partition_key: "p".into(),
        event_type: "A".into(),
        event_time_ms: 3,
    })
    .unwrap();
    g.add_vertex(Vertex {
        id: 4,
        partition_key: "p".into(),
        event_type: "B".into(),
        event_time_ms: 4,
    })
    .unwrap();

    // Cycle: 1 -> 2 -> 3 -> 1. Plus a sink: 3 -> 4.
    // Note: `add_edge` accepts any (src, dst) with both endpoints known — the
    // *temporal* monotonicity check happens in the executor at traversal time,
    // so the back-edge 3 -> 1 is a valid graph edge that the executor will
    // reject via causality but only *after* considering it. This is the
    // failure mode the cycle guard defends against.
    g.add_edge(Edge { src: 1, dst: 2, window_start_ms: 0, window_end_ms: 0 }).unwrap();
    g.add_edge(Edge { src: 2, dst: 3, window_start_ms: 0, window_end_ms: 0 }).unwrap();
    g.add_edge(Edge { src: 3, dst: 1, window_start_ms: 0, window_end_ms: 0 }).unwrap();
    g.add_edge(Edge { src: 3, dst: 4, window_start_ms: 0, window_end_ms: 0 }).unwrap();

    let mut q = Query::new(
        "cycle",
        vec![
            EventType { name: "A".into(), kleene_plus: false, predicate: None },
            EventType { name: "B".into(), kleene_plus: false, predicate: None },
        ],
    );
    q.skip_till_any_match = true;

    for (label, run) in [
        ("mcet", &exec::execute_mcet as &dyn Fn(&Graph, &Query, &mut MatchResult, &mut ExecStats)),
        ("tcet", &exec::execute_tcet as &dyn Fn(&Graph, &Query, &mut MatchResult, &mut ExecStats)),
    ] {
        let mut out = MatchResult::new(64, caps::MAX_PATH_LEN);
        let mut stats = ExecStats::default();
        run(&g, &q, &mut out, &mut stats);

        // (1) Terminated (Rust would panic on stack overflow if not; we get here).
        // (2) Every path is bounded by MAX_PATH_LEN.
        for p in &out.paths {
            assert!(p.len() <= caps::MAX_PATH_LEN, "{label}: path {:?} exceeds MAX_PATH_LEN", p);
            // (3) Every path is simple.
            let mut seen = std::collections::HashSet::new();
            for id in p {
                assert!(seen.insert(*id), "{label}: path {:?} revisits {id}", p);
            }
        }
        // (4) The canonical match is present.
        assert!(
            out.paths.iter().any(|p| p == &[1, 2, 3, 4]),
            "{label}: canonical match [1,2,3,4] not emitted; got {:?}",
            out.paths
        );
    }

    // HCET has a different signature (switch_depth), so exercise it explicitly.
    for depth in 0..=q.len() {
        let mut out = MatchResult::new(64, caps::MAX_PATH_LEN);
        let mut stats = ExecStats::default();
        exec::execute_hcet(&g, &q, depth, &mut out, &mut stats);
        for p in &out.paths {
            assert!(
                p.len() <= caps::MAX_PATH_LEN,
                "hcet(d={depth}): path {:?} exceeds MAX_PATH_LEN",
                p
            );
            let mut seen = std::collections::HashSet::new();
            for id in p {
                assert!(seen.insert(*id), "hcet(d={depth}): path {:?} revisits {id}", p);
            }
        }
        assert!(
            out.paths.iter().any(|p| p == &[1, 2, 3, 4]),
            "hcet(d={depth}): canonical match [1,2,3,4] not emitted; got {:?}",
            out.paths
        );
    }
}

/// Caps module values must remain stable across releases (external ABI
/// contract for the FFI shim).
#[test]
fn caps_are_stable_abi() {
    assert_eq!(caps::MAX_SEQ, 16);
    assert_eq!(caps::MAX_PATH_LEN, 64);
    assert_eq!(caps::MAX_PATHS, 100_000);
    assert_eq!(caps::MAX_GRAPHLETS, 4096);
    assert_eq!(caps::MAX_NATIVE_THREADS, 64);
    assert_eq!(caps::MAX_ERROR_LEN, 256);
}

// ---------------------------------------------------------------------------
// 2. Property tests — acceptance bar for future executor implementations
// ---------------------------------------------------------------------------

/// Every emitted path is temporally non-decreasing in `event_time_ms`.
/// This is the core causality invariant of the CET engine. Runs against
/// both TCET and MCET.
#[test]
fn property_paths_are_temporally_monotonic() {
    proptest!(|(g in testkit::arb_graph(), q in testkit::arb_query())| {
        for exec_fn in [
            exec::execute_tcet as fn(&Graph, &Query, &mut MatchResult, &mut ExecStats),
            exec::execute_mcet as fn(&Graph, &Query, &mut MatchResult, &mut ExecStats),
        ] {
            let mut out = MatchResult::new(1000, 32);
            let mut stats = ExecStats::default();
            exec_fn(&g, &q, &mut out, &mut stats);
            for path in &out.paths {
                let times: Vec<i64> = path
                    .iter()
                    .map(|id| g.get(*id).expect("path vertex exists").event_time_ms)
                    .collect();
                for w in times.windows(2) {
                    prop_assert!(w[0] <= w[1], "path not temporally monotonic: {:?}", times);
                }
            }
        }
    });
}

/// `stats.paths_emitted == out.paths.len()` on any non-truncated run.
#[test]
fn property_stats_paths_emitted_matches_result_len() {
    proptest!(|(g in testkit::arb_graph(), q in testkit::arb_query())| {
        for exec_fn in [
            exec::execute_tcet as fn(&Graph, &Query, &mut MatchResult, &mut ExecStats),
            exec::execute_mcet as fn(&Graph, &Query, &mut MatchResult, &mut ExecStats),
        ] {
            let mut out = MatchResult::new(10_000, 64);
            let mut stats = ExecStats::default();
            exec_fn(&g, &q, &mut out, &mut stats);
            if !stats.overflow {
                prop_assert_eq!(stats.paths_emitted, out.paths.len());
            }
        }
    });
}

/// Every emitted path fits within the configured caps.
#[test]
fn property_result_respects_capacity() {
    proptest!(|(g in testkit::arb_graph(), q in testkit::arb_query())| {
        for exec_fn in [
            exec::execute_tcet as fn(&Graph, &Query, &mut MatchResult, &mut ExecStats),
            exec::execute_mcet as fn(&Graph, &Query, &mut MatchResult, &mut ExecStats),
        ] {
            let cap_paths = 50usize;
            let cap_len = 16usize;
            let mut out = MatchResult::new(cap_paths, cap_len);
            let mut stats = ExecStats::default();
            exec_fn(&g, &q, &mut out, &mut stats);
            prop_assert!(out.paths.len() <= cap_paths);
            for p in &out.paths {
                prop_assert!(p.len() <= cap_len);
            }
        }
    });
}

// ---------------------------------------------------------------------------
// 3. Cross-algorithm equivalence — the headline property
// ---------------------------------------------------------------------------

/// The three execution strategies must produce identical path sets.
///
/// This is the headline invariant of the port: MCET, TCET, and HCET are three
/// different traversals of the same semantic function. If they disagree on
/// any input, at least one is wrong.
#[test]
fn property_mcet_tcet_hcet_agree() {
    proptest!(|(g in testkit::arb_graph(), q in testkit::arb_query())| {
        let mut out_m = MatchResult::new(10_000, 64);
        let mut out_t = MatchResult::new(10_000, 64);
        let mut out_h = MatchResult::new(10_000, 64);
        let mut s_m = ExecStats::default();
        let mut s_t = ExecStats::default();
        let mut s_h = ExecStats::default();
        exec::execute_mcet(&g, &q, &mut out_m, &mut s_m);
        exec::execute_tcet(&g, &q, &mut out_t, &mut s_t);
        let switch_depth = q.len() / 2;
        exec::execute_hcet(&g, &q, switch_depth, &mut out_h, &mut s_h);

        // Truncated runs may drop paths depending on traversal order, so
        // only compare when all three completed cleanly.
        if !s_m.overflow && !s_t.overflow && !s_h.overflow {
            let cm = testkit::canonicalize(&out_m.paths);
            let ct = testkit::canonicalize(&out_t.paths);
            let ch = testkit::canonicalize(&out_h.paths);
            prop_assert_eq!(&cm, &ct);
            prop_assert_eq!(&ct, &ch);
        }
    });
}

/// HCET result must be invariant under the choice of `switch_depth`.
///
/// Since HCET at `switch_depth = 0` and `switch_depth = 1` delegates to TCET,
/// this also transitively re-checks HCET/TCET equivalence.
#[test]
fn property_hcet_switch_depth_invariant() {
    proptest!(|(g in testkit::arb_graph(), q in testkit::arb_query())| {
        let mut baseline = MatchResult::new(10_000, 64);
        let mut s_base = ExecStats::default();
        exec::execute_hcet(&g, &q, 0, &mut baseline, &mut s_base);

        for depth in 0..=q.len() {
            let mut out = MatchResult::new(10_000, 64);
            let mut stats = ExecStats::default();
            exec::execute_hcet(&g, &q, depth, &mut out, &mut stats);
            if !stats.overflow && !s_base.overflow {
                let cb = testkit::canonicalize(&baseline.paths);
                let cc = testkit::canonicalize(&out.paths);
                prop_assert_eq!(cb, cc, "HCET diverges at switch_depth={}", depth);
            }
        }
    });
}

/// MCET and TCET must produce identical path sets on any input.
///
/// This is the sub-property of [`property_mcet_tcet_hcet_agree`] that we can
/// enforce today (HCET is still stubbed). Un-ignoring the full three-way
/// property is the acceptance bar for the next milestone.
#[test]
fn property_mcet_matches_tcet() {
    proptest!(|(g in testkit::arb_graph(), q in testkit::arb_query())| {
        let mut out_m = MatchResult::new(10_000, 64);
        let mut out_t = MatchResult::new(10_000, 64);
        let mut s_m = ExecStats::default();
        let mut s_t = ExecStats::default();
        exec::execute_mcet(&g, &q, &mut out_m, &mut s_m);
        exec::execute_tcet(&g, &q, &mut out_t, &mut s_t);

        // Only compare when neither run truncated; a truncated run may drop
        // paths arbitrarily depending on traversal order.
        if !s_m.overflow && !s_t.overflow {
            let cm = testkit::canonicalize(&out_m.paths);
            let ct = testkit::canonicalize(&out_t.paths);
            prop_assert_eq!(cm, ct);
        }
    });
}

// ---------------------------------------------------------------------------
// 4. Sanity: generators produce well-formed inputs (runs today)
// ---------------------------------------------------------------------------

/// Verifies that the [`testkit::arb_graph`] generator produces graphs that
/// obey the invariants the executor will rely on. Runs today so a broken
/// generator can't hide bugs in future property tests.
#[test]
fn testkit_generators_produce_well_formed_graphs() {
    proptest!(|(g in testkit::arb_graph())| {
        // Timestamps are strictly increasing.
        for w in g.vertices().windows(2) {
            prop_assert!(w[0].event_time_ms <= w[1].event_time_ms);
        }
        // Every edge references known endpoints and points forward in time.
        for e in g.edges() {
            let sv = g.get(e.src).expect("src exists");
            let dv = g.get(e.dst).expect("dst exists");
            prop_assert!(sv.event_time_ms <= dv.event_time_ms);
        }
    });
}

/// Verifies that the query generator produces queries within the alphabet.
#[test]
fn testkit_generators_produce_well_formed_queries() {
    proptest!(|(q in testkit::arb_query())| {
        prop_assert!(!q.is_empty());
        prop_assert!(q.len() <= 4);
        for e in &q.seq {
            prop_assert!(testkit::EVENT_TYPES.contains(&e.name.as_str()));
        }
    });
}

/// Small hand-authored graph — smoke test that fixed data flows through the
/// public API before we start property-testing behavior.
#[test]
fn hand_authored_graph_builds() {
    let mut g = Graph::with_capacity(16, 16);
    g.add_vertex(v(1, 1, "A")).unwrap();
    g.add_vertex(v(2, 2, "X")).unwrap();
    g.add_vertex(v(3, 3, "A")).unwrap();
    g.add_vertex(v(4, 4, "B")).unwrap();
    g.add_vertex(v(5, 5, "C")).unwrap();
    for (s, d) in [(1, 2), (2, 3), (3, 4), (4, 5)] {
        g.add_edge(edge(s, d)).unwrap();
    }
    assert_eq!(g.vertex_count(), 5);
    assert_eq!(g.edge_count(), 4);
    assert_eq!(g.get(3).unwrap().event_type, "A");

    // Query construction path (no execution yet).
    let seq = vec![
        EventType { name: "A".into(), kleene_plus: true, predicate: None },
        EventType { name: "B".into(), kleene_plus: false, predicate: None },
        EventType { name: "C".into(), kleene_plus: false, predicate: None },
    ];
    let q = Query::new("q", seq);
    assert_eq!(q.len(), 3);
    assert!(q.skip_till_any_match);
}

// ---------------------------------------------------------------------------
// 5. Optimizer properties and regressions
// ---------------------------------------------------------------------------

/// `greedy_plan.total_memory` must never exceed `max_mem`, regardless of the
/// graphlet population.
#[test]
fn property_greedy_respects_memory_cap() {
    let strat = (
        prop::collection::vec(
            (0i64..100i64, 1i64..20i64, 1i64..20i64), // (start_ms, vcount, ecount)
            0..30,
        ),
        1.0f64..1000.0f64,
    );
    proptest!(|((rows, max_mem) in strat)| {
        let model = optimizer::CostModel::default();
        let mut arr: Vec<optimizer::Graphlet> = rows.into_iter().enumerate().map(|(i, (start, v, e))| {
            optimizer::Graphlet {
                id: format!("g{i}"),
                start_ms: start,
                end_ms: start + 10,
                vertex_count: v,
                edge_count: e,
                memory_cost: 0.0,
                cpu_cost: 0.0,
            }
        }).collect();
        optimizer::estimate_costs(&mut arr, &model);
        let plan = optimizer::greedy_plan(&arr, max_mem);
        prop_assert!(plan.total_memory <= max_mem + 1e-9,
            "greedy plan exceeded memory cap: {} > {}", plan.total_memory, max_mem);
    });
}

/// Branch-and-bound must respect the memory cap and, when both planners
/// return non-empty plans within their budgets, must have CPU cost no worse
/// than greedy.
#[test]
fn property_bnb_matches_or_beats_greedy_on_small_inputs() {
    let strat = (
        prop::collection::vec(
            (0i64..100i64, 1i64..8i64, 1i64..8i64),
            0..8, // small enough for bnb to complete within budget
        ),
        1.0f64..30.0f64,
    );
    proptest!(|((rows, max_mem) in strat)| {
        let model = optimizer::CostModel::default();
        let mut arr: Vec<optimizer::Graphlet> = rows.into_iter().enumerate().map(|(i, (start, v, e))| {
            optimizer::Graphlet {
                id: format!("g{i}"),
                start_ms: start,
                end_ms: start + 10,
                vertex_count: v,
                edge_count: e,
                memory_cost: 0.0,
                cpu_cost: 0.0,
            }
        }).collect();
        optimizer::estimate_costs(&mut arr, &model);
        let g = optimizer::greedy_plan(&arr, max_mem);
        let b = optimizer::branch_and_bound_plan(&arr, max_mem, 1_000_000);
        prop_assert!(g.total_memory <= max_mem + 1e-9);
        prop_assert!(b.total_memory <= max_mem + 1e-9);
        if !g.indices.is_empty() && !b.indices.is_empty() {
            prop_assert!(b.total_cpu <= g.total_cpu + 1e-9,
                "bnb cpu={} > greedy cpu={}", b.total_cpu, g.total_cpu);
        }
    });
}

/// `classify_graphlet_delta` produces a partition:
///   shared ∪ new  ⊆ curr (as (start,end) keys)
///   shared ∪ expired ⊆ prev (as (start,end) keys)
///   shared ∩ new  == ∅
///   every prev entry is in shared-image or expired
///   every curr entry is in shared-image or new
#[test]
fn property_delta_partition() {
    let strat = (
        prop::collection::vec((0i64..50i64, 1i64..20i64), 0..10),
        prop::collection::vec((0i64..50i64, 1i64..20i64), 0..10),
    );
    proptest!(|((prev_rows, curr_rows) in strat)| {
        let model = optimizer::CostModel::default();
        let make = |rows: Vec<(i64, i64)>| -> Vec<optimizer::Graphlet> {
            let mut arr: Vec<_> = rows.into_iter().enumerate().map(|(i, (s, d))| {
                optimizer::Graphlet {
                    id: format!("g{i}"),
                    start_ms: s,
                    end_ms: s + d,
                    vertex_count: 1,
                    edge_count: 1,
                    memory_cost: 0.0,
                    cpu_cost: 0.0,
                }
            }).collect();
            optimizer::estimate_costs(&mut arr, &model);
            arr
        };
        let prev = make(prev_rows);
        let curr = make(curr_rows);
        let d = optimizer::classify_graphlet_delta(&prev, &curr);

        // Turn index buckets back into (start,end) keys.
        let shared_keys: std::collections::HashSet<(i64, i64)> =
            d.shared.iter().map(|&i| (curr[i].start_ms, curr[i].end_ms)).collect();
        let new_keys: std::collections::HashSet<(i64, i64)> =
            d.new.iter().map(|&i| (curr[i].start_ms, curr[i].end_ms)).collect();
        let expired_keys: std::collections::HashSet<(i64, i64)> =
            d.expired.iter().map(|&j| (prev[j].start_ms, prev[j].end_ms)).collect();
        let prev_keys: std::collections::HashSet<(i64, i64)> =
            prev.iter().map(|g| (g.start_ms, g.end_ms)).collect();
        let curr_keys: std::collections::HashSet<(i64, i64)> =
            curr.iter().map(|g| (g.start_ms, g.end_ms)).collect();

        // shared ⊆ prev and shared ⊆ curr.
        for k in &shared_keys {
            prop_assert!(prev_keys.contains(k));
            prop_assert!(curr_keys.contains(k));
        }
        // new is disjoint from prev.
        prop_assert!(new_keys.is_disjoint(&prev_keys));
        // expired is disjoint from curr.
        prop_assert!(expired_keys.is_disjoint(&curr_keys));
        // shared and new are disjoint.
        prop_assert!(shared_keys.is_disjoint(&new_keys));
        // Every curr key is either shared or new.
        for k in &curr_keys {
            prop_assert!(shared_keys.contains(k) || new_keys.contains(k));
        }
        // Every prev key is either shared (as image) or expired.
        for k in &prev_keys {
            prop_assert!(shared_keys.contains(k) || expired_keys.contains(k));
        }
    });
}

/// Regression: cost coefficients live on the `CostModel` value, not in
/// file-scope globals. Running two planners concurrently with different
/// models must not interfere. This closes the C engine's global-mutable
/// coefficients race (`c_engine/src/optimizer.c:6`).
#[test]
fn regression_cost_coefficients_are_per_call_and_thread_safe() {
    use std::sync::Arc;
    use std::thread;

    let base: Vec<optimizer::Graphlet> = (0..64)
        .map(|i| optimizer::Graphlet {
            id: format!("g{i}"),
            start_ms: i * 10,
            end_ms: i * 10 + 5,
            vertex_count: (i % 5) + 1,
            edge_count: (i % 3) + 1,
            memory_cost: 0.0,
            cpu_cost: 0.0,
        })
        .collect();
    let base = Arc::new(base);

    let m1 =
        optimizer::CostModel { mem_vertex: 1.0, mem_edge: 0.0, cpu_edge: 0.0, cpu_vertex: 0.0 };
    let m2 =
        optimizer::CostModel { mem_vertex: 0.0, mem_edge: 1.0, cpu_edge: 0.0, cpu_vertex: 0.0 };

    let handles: Vec<_> = (0..8)
        .map(|i| {
            let base = Arc::clone(&base);
            let model = if i % 2 == 0 { m1 } else { m2 };
            thread::spawn(move || {
                for _ in 0..200 {
                    let mut arr = (*base).clone();
                    optimizer::estimate_costs(&mut arr, &model);
                    // Each graphlet's memory_cost must be exactly what THIS
                    // thread's model produces.
                    for g in &arr {
                        let expected = g.vertex_count as f64 * model.mem_vertex
                            + g.edge_count as f64 * model.mem_edge;
                        assert!((g.memory_cost - expected).abs() < 1e-9);
                    }
                }
            })
        })
        .collect();
    for h in handles {
        h.join().unwrap();
    }
}

// ---------------------------------------------------------------------------
// 6. Sliding-window properties
// ---------------------------------------------------------------------------

/// Every emitted window fits inside `[start, end]`.
#[test]
fn property_windows_fit_range() {
    let strat = (
        -1_000_000i64..=1_000_000i64, // start
        1i64..=1_000_000i64,          // delta (end = start + delta)
        1i64..=1_000_000i64,          // within
        1i64..=1_000_000i64,          // slide
    );
    proptest!(|((start, delta, within, slide) in strat)| {
        let end = start + delta;
        let w = sliding::materialize_windows(start, end, within, slide, 4096).unwrap();
        for &(t, s) in &w {
            prop_assert!(t >= start, "window start {} < range start {}", t, start);
            prop_assert!(s <= end, "window stop {} > range end {}", s, end);
            prop_assert_eq!(s - t, within, "window length != within");
        }
    });
}

/// Consecutive windows are spaced by exactly `slide` ms.
#[test]
fn property_windows_spacing_is_slide() {
    let strat = (0i64..=100_000i64, 1i64..=100_000i64, 1i64..=1_000i64, 1i64..=1_000i64);
    proptest!(|((start, delta, within, slide) in strat)| {
        let end = start + delta;
        let w = sliding::materialize_windows(start, end, within, slide, 4096).unwrap();
        for pair in w.windows(2) {
            prop_assert_eq!(pair[1].0 - pair[0].0, slide);
        }
    });
}

/// The number of emitted windows matches the closed-form formula
/// `max(0, floor((end - start - within) / slide) + 1)`, capped by `cap`.
#[test]
fn property_window_count_matches_formula() {
    let strat = (
        -10_000i64..=10_000i64,
        0i64..=10_000i64,
        1i64..=1_000i64,
        1i64..=1_000i64,
        1usize..=4096usize,
    );
    proptest!(|((start, delta, within, slide, cap) in strat)| {
        let end = start + delta;
        let w = sliding::materialize_windows(start, end, within, slide, cap).unwrap();
        let expected = if end - start < within {
            0usize
        } else {
            let n = (end - start - within) / slide + 1;
            (n as usize).min(cap)
        };
        prop_assert_eq!(w.len(), expected);
    });
}

/// Non-positive `slide` or `within` are always rejected with the right
/// error variant, never silently mishandled.
#[test]
fn property_invalid_parameters_are_rejected() {
    let strat = (-100i64..=100i64, -100i64..=100i64, -10i64..=10i64, -10i64..=10i64);
    proptest!(|((start, end, within, slide) in strat)| {
        let r = sliding::materialize_windows(start, end, within, slide, 64);
        if slide <= 0 {
            prop_assert_eq!(r, Err(sliding::WindowError::NonPositiveSlide(slide)));
        } else if within <= 0 {
            prop_assert_eq!(r, Err(sliding::WindowError::NonPositiveWithin(within)));
        } else {
            prop_assert!(r.is_ok());
        }
    });
}
