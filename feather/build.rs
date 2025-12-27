use std::env;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;

/* This overrides memory.x provided by feather_m0 crate */
fn main() {
    let out = &PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let (flash_origin, flash_ram): (&str, &str) = match env::var("CARGO_FEATURE_BOOTLOADER_ENABLED")
    {
        Ok(_) => ("0x00002000", "248K"),
        Err(_) => ("0x00000000", "256K"),
    };
    let memory = format!(
        r#"MEMORY
    {{
      FLASH (rx) : ORIGIN = {}, LENGTH = {}
      RAM (xrw)  : ORIGIN = 0x20000000, LENGTH = 32K
    }}
    _stack_start = ORIGIN(RAM) + LENGTH(RAM);"#,
        flash_origin, flash_ram
    );
    File::create(out.join("memory.x"))
        .unwrap()
        .write_all(&memory.as_bytes())
        .unwrap();
    println!("cargo:rustc-link-search={}", out.display());
    println!("cargo:rerun-if-changed=memory.x");
    println!("cargo:rerun-if-changed=build.rs");

    println!("cargo:rustc-link-arg=--nmagic");
    println!("cargo:rustc-link-arg=-Tlink.x");
    if env::var("CARGO_FEATURE_DEFMT").is_ok() {
        println!("cargo:rustc-link-arg=-Tdefmt.x");
    }
}
