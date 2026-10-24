use std::path::PathBuf;

fn main() {
    // libvosk is a dynamic library. In development it lives in models/libvosk/;
    // in a bundle it is copied into Contents/Frameworks (tauri.conf.json → bundle.macOS.frameworks).
    // Development builds find libvosk in the repo; release builds only look in the
    // bundle (@executable_path/../Frameworks, added by tauri-build).
    if cfg!(target_os = "macos") && std::env::var("PROFILE").as_deref() == Ok("debug") {
        let default = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../models/libvosk");
        let dir = std::env::var_os("VOSK_LIB_DIR").map(PathBuf::from).unwrap_or(default);
        let dir = dir.canonicalize().unwrap_or(dir);
        println!("cargo:rustc-link-arg=-Wl,-rpath,{}", dir.display());
    }
    tauri_build::build()
}
