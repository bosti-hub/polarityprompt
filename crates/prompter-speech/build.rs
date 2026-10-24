//! Tells the linker where libvosk lives. The library is downloaded by
//! `scripts/fetch-models.sh` into `models/libvosk/` (not committed).
//! Override with the `VOSK_LIB_DIR` environment variable.

use std::env;
use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=VOSK_LIB_DIR");

    let default = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../models/libvosk");
    let dir = env::var_os("VOSK_LIB_DIR").map(PathBuf::from).unwrap_or(default);
    let dir = dir.canonicalize().unwrap_or(dir);
    if !dir.exists() {
        println!(
            "cargo:warning=libvosk not found in {}. Run scripts/fetch-models.sh first.",
            dir.display()
        );
    }
    println!("cargo:rustc-link-search=native={}", dir.display());
    // Let this crate's own tests and binaries find the dylib at run time
    // (rustc-link-arg does not propagate to dependents; apps/desktop sets its own rpath).
    if cfg!(target_os = "macos") {
        println!("cargo:rustc-link-arg=-Wl,-rpath,{}", dir.display());
    }
}
