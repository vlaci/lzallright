use std::env;
use std::path;

use cxx_build::CFG;

fn main() {
    CFG.include_prefix = "lzokay-sys";
    let src_dir = path::Path::new(env!("CARGO_MANIFEST_DIR")).join("lzokay");

    println!("cargo:rerun-if-changed={}", file!());
    println!("cargo:rerun-if-changed={}", src_dir.to_str().unwrap());

    #[cfg(target_os = "linux")]
    {
        let out = String::from_utf8(
            std::process::Command::new("c++")
                .args(["-v"])
                .output()
                .expect("Cannot find C++ compiler")
                .stderr,
        )
        .unwrap();

        let cpp_runtime = if out.contains("gcc") {
            "libstdc++.a"
        } else if out.contains("clang") {
            "libc++.a"
        } else {
            panic!("No compatible compiler found. Either clang or gcc is needed.");
        };

        let cpp_runtime_path =
            std::process::Command::new(env::var("CXX").unwrap_or_else(|_| "c++".to_string()))
                .arg(format!("--print-file-name={cpp_runtime}"))
                .output()
                .expect("Failed to execute $CXX to get runtime library path")
                .stdout;

        println!(
            "cargo:rustc-link-search=native={}",
            String::from_utf8_lossy(&cpp_runtime_path)
                .trim()
                .strip_suffix(&cpp_runtime)
                .expect("Failed to strip suffix"),
        );
    }

    #[cfg(target_os = "macos")]
    {
        println!("cargo:rustc-link-lib=dylib=c++");
        println!("cargo:rustc-link-lib=dylib=c++abi");
    }

    cxx_build::bridge("src/lib.rs")
        .file(src_dir.join("lzokay.cpp").to_str().unwrap())
        .flag("-std=c++14")
        .flag_if_supported("-O2")
        .flag_if_supported("-Wno-maybe-uninitialized")
        .compile("lzokay-sys");
}
