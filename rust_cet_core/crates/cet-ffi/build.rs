//! Build the C reference engine as a static library so integration tests can
//! diff Rust and C behavior symbol-for-symbol.
//!
//! The C source lives at `../../../c_engine`. We compile with pthreads
//! **disabled** so the reference and the Rust `cet-parallel` driver can be
//! compared apples-to-apples on the deterministic serial code path.

fn main() {
    // Always announce the cfg so rustc doesn't warn when it's unset.
    println!("cargo:rustc-check-cfg=cfg(have_c_engine)");

    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    let repo_root = std::path::Path::new(&manifest_dir)
        .parent()
        .and_then(|p| p.parent())
        .and_then(|p| p.parent())
        .expect("resolve repo root")
        .to_path_buf();
    let c_engine = repo_root.join("c_engine");
    if !c_engine.exists() {
        // If the C engine source tree is unavailable, skip — the integration
        // tests that need it will opt out at compile time via a cfg feature.
        return;
    }

    let mut build = cc::Build::new();
    build
        .include(c_engine.join("include"))
        .include(c_engine.join("src"))
        .file(c_engine.join("src/dsl.c"))
        .file(c_engine.join("src/graph.c"))
        .file(c_engine.join("src/algorithms.c"))
        .file(c_engine.join("src/optimizer.c"))
        .file(c_engine.join("src/sliding.c"))
        .file(c_engine.join("src/runtime.c"))
        .file(c_engine.join("src/parallel_hcet.c"))
        .warnings(false);

    build.compile("cet_engine_c");

    // Re-run the build if any C source or header changes.
    println!("cargo:rerun-if-changed={}", c_engine.display());
    println!("cargo:rustc-cfg=have_c_engine");
}
