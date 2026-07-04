//! Behavior tests for the parallel HCET driver.
//!
//! These are the acceptance-bar properties for `cet-parallel`. The headline
//! invariant is [`property_parallel_matches_serial`]: for every input and every
//! thread count in `{1, 2, 4, 8}`, `execute_hcet_parallel` produces the same
//! path set as the serial `exec::execute_hcet`. This is what makes the parallel
//! driver safe to deploy on Spark/Databricks executors — the result is
//! independent of thread scheduling.

use cet_core::{exec, testkit, ExecStats, MatchResult};
use cet_parallel::{execute_hcet_parallel, RuntimeConfig, RuntimeStats};
use proptest::prelude::*;

fn canonicalize(paths: &[Vec<i64>]) -> Vec<Vec<i64>> {
    let mut owned: Vec<Vec<i64>> = paths.to_vec();
    owned.sort();
    owned.dedup();
    owned
}

/// For every input and every thread count in `{1, 2, 4, 8}`, the parallel
/// driver's path set equals the serial HCET path set.
#[test]
fn property_parallel_matches_serial() {
    proptest!(|(g in testkit::arb_graph(), q in testkit::arb_query())| {
        // Baseline: serial HCET.
        let mut serial_out = MatchResult::new(10_000, 64);
        let mut serial_stats = ExecStats::default();
        let switch_depth = q.len() / 2;
        exec::execute_hcet(&g, &q, switch_depth, &mut serial_out, &mut serial_stats);
        let baseline = canonicalize(&serial_out.paths);

        for &threads in &[1usize, 2, 4, 8] {
            let cfg = RuntimeConfig { native_threads: threads, deterministic_merge: true };
            let mut par_out = MatchResult::new(10_000, 64);
            let mut par_stats = ExecStats::default();
            let mut rt = RuntimeStats::default();
            execute_hcet_parallel(&g, &q, switch_depth, &cfg, &mut par_out, &mut par_stats, &mut rt);

            if !serial_stats.overflow && !par_stats.overflow {
                let got = canonicalize(&par_out.paths);
                prop_assert_eq!(
                    &got, &baseline,
                    "parallel diverges from serial at threads={}", threads
                );
            }
        }
    });
}

/// Determinism property: running the parallel driver twice with the same
/// thread count on the same input produces bit-identical `out.paths`.
///
/// This is a stronger property than [`property_parallel_matches_serial`]
/// because it constrains ordering, not just set equality.
#[test]
fn property_parallel_is_deterministic() {
    proptest!(|(g in testkit::arb_graph(), q in testkit::arb_query())| {
        let switch_depth = q.len() / 2;
        let cfg = RuntimeConfig { native_threads: 4, deterministic_merge: true };

        let mut first_out = MatchResult::new(10_000, 64);
        let mut first_stats = ExecStats::default();
        let mut first_rt = RuntimeStats::default();
        execute_hcet_parallel(&g, &q, switch_depth, &cfg,
            &mut first_out, &mut first_stats, &mut first_rt);

        let mut second_out = MatchResult::new(10_000, 64);
        let mut second_stats = ExecStats::default();
        let mut second_rt = RuntimeStats::default();
        execute_hcet_parallel(&g, &q, switch_depth, &cfg,
            &mut second_out, &mut second_stats, &mut second_rt);

        if !first_stats.overflow && !second_stats.overflow {
            prop_assert_eq!(&first_out.paths, &second_out.paths);
        }
    });
}
