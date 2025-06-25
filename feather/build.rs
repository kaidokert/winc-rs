use std::env;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;

/* This overrides memory.x provided by feather_m0 crate */
fn main() {
    let out = &PathBuf::from(env::var_os("OUT_DIR").unwrap());
    File::create(out.join("memory.x"))
        .unwrap()
        .write_all(include_bytes!("memory.x"))
        .unwrap();
    println!("cargo:rustc-link-search={}", out.display());
    println!("cargo:rerun-if-changed=memory.x");
    println!("cargo:rerun-if-changed=build.rs");

    println!("cargo:rustc-link-arg=--nmagic");
    println!("cargo:rustc-link-arg=-Tlink.x");
    let defmt_feature = env::var("CARGO_CFG_FEATURE").unwrap_or_default();
    if defmt_feature.contains("defmt") {
        println!("cargo:rustc-link-arg=-Tdefmt.x");
    }
}
