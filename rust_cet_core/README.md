# rust_cet_core

Rust port of the CET (Complex Event Temporal) pattern-match engine originally
implemented in `../c_engine`. This workspace is being brought up incrementally;
functional parity with the C engine is verified by property tests and by a
golden-file behavior corpus that both implementations must satisfy.

## Layout

| Crate | Purpose |
|---|---|
| `crates/cet-core` | Types (`Graph`, `Query`, `MatchResult`, `ExecStats`) and the MCET/TCET/HCET execution algorithms. |
| `crates/cet-dsl` | CSV pattern-string parser (`parse_query`). Replaces `c_engine/src/dsl.c`. |
| `crates/cet-parallel` | Rayon-based parallel driver. Replaces `c_engine/src/parallel_hcet.c`. |
| `crates/cet-ffi` | `extern "C"` shim exposing the same symbols as `c_engine/include/cet.h`. |
| `benches/` | Criterion benchmarks with committed baselines. |
| `fuzz/` | `cargo-fuzz` targets (DSL parser first). |

## Migration status

This is a scaffold. Types and error surfaces are in place; execution algorithms
are `todo!()` stubs to be filled in module-by-module under the property-test
suite. See `docs/` in the parent repo for the test strategy.

## Common commands

```bash
cargo build --workspace
cargo test  --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --check

# Benches (criterion)
cargo bench -p cet-benches

# Fuzz (requires `cargo install cargo-fuzz`)
cd fuzz && cargo +nightly fuzz run dsl
```
