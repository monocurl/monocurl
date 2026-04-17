use std::{env, path::PathBuf};

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let upstream_dir = manifest_dir.join("upstream");
    let source_dir = upstream_dir.join("Source");
    let include_dir = upstream_dir.join("Include");

    let mut build = cc::Build::new();
    build
        .include(&source_dir)
        .include(&include_dir)
        .warnings(false)
        .file(source_dir.join("bucketalloc.c"))
        .file(source_dir.join("dict.c"))
        .file(source_dir.join("geom.c"))
        .file(source_dir.join("mesh.c"))
        .file(source_dir.join("priorityq.c"))
        .file(source_dir.join("sweep.c"))
        .file(source_dir.join("tess.c"));

    build.compile("tess2_upstream");

    println!("cargo:rerun-if-changed={}", source_dir.display());
    println!("cargo:rerun-if-changed={}", include_dir.display());
    println!(
        "cargo:rerun-if-changed={}",
        upstream_dir.join("LICENSE.txt").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        upstream_dir.join("README.md").display()
    );

    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("linux") {
        println!("cargo:rustc-link-lib=m");
    }
}
