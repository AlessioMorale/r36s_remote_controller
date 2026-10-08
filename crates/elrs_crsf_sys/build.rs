//! Compiles the C++ CRSF library sources and the cxx facade. The library's ament CMake is
//! not used; only its `src/crsf/*.cpp` and headers are needed (design §3.5).

use std::path::PathBuf;

fn main() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    // The library is the deps/elrs_joy submodule; ELRS_CRSF_PROTOCOL_DIR overrides it
    let protocol_dir = std::env::var("ELRS_CRSF_PROTOCOL_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| manifest.join("../../deps/elrs_joy/elrs_joy_crsf_protocol"));
    let protocol_dir = protocol_dir
        .canonicalize()
        .unwrap_or_else(|_| {
            panic!(
                "CRSF library not found at {} (run `git submodule update --init`)",
                protocol_dir.display()
            )
        });

    let sources: Vec<PathBuf> = std::fs::read_dir(protocol_dir.join("src/crsf"))
        .expect("read src/crsf")
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "cpp"))
        .collect();

    cxx_build::bridge("src/lib.rs")
        .file("cpp/crsf_ffi.cpp")
        .files(&sources)
        .include(protocol_dir.join("include"))
        .include(&manifest)
        .std("c++20")
        .flag_if_supported("-Wall")
        .flag_if_supported("-Wextra")
        .flag_if_supported("-Wno-unused-parameter")
        .compile("elrs_crsf_ffi");

    println!("cargo:rerun-if-changed=src/lib.rs");
    println!("cargo:rerun-if-changed=cpp/crsf_ffi.hpp");
    println!("cargo:rerun-if-changed=cpp/crsf_ffi.cpp");
    println!("cargo:rerun-if-env-changed=ELRS_CRSF_PROTOCOL_DIR");
    println!("cargo:rerun-if-changed={}", protocol_dir.join("src").display());
    println!("cargo:rerun-if-changed={}", protocol_dir.join("include").display());
    for source in &sources {
        println!("cargo:rerun-if-changed={}", source.display());
    }
    // Fixture location for tests in dependent crates
    println!("cargo:fixtures={}", protocol_dir.join("test/fixtures").display());
}
