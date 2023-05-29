use std::env;
#[cfg(target_os = "linux")]
use std::fs;
#[cfg(target_os = "linux")]
use std::os::unix;
use std::path;

use cxx_build::CFG;

fn main() {
    CFG.include_prefix = "lzokay-sys";
    let src_dir = path::Path::new(env!("CARGO_MANIFEST_DIR")).join("lzokay");

    println!(
        "cargo:rerun-if-changed={}",
        src_dir.join("build.rs").to_str().unwrap()
    );
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

        let cpp_runtime_path = path::PathBuf::from(
            String::from_utf8_lossy(
                &std::process::Command::new(env::var("CXX").unwrap_or_else(|_| "c++".to_string()))
                    .arg(format!("--print-file-name={cpp_runtime}"))
                    .output()
                    .expect("Failed to execute $CXX to get runtime library path")
                    .stdout,
            )
            .as_ref()
            .trim_end(),
        );

        let build_lib_dir =
            path::PathBuf::from(env::var("OUT_DIR").unwrap()).join("lzokay-sys-lib");
        let cpp_runtime_build_path = build_lib_dir.join(cpp_runtime_path.file_name().unwrap());
        fs::create_dir_all(&build_lib_dir).expect("Cannot create lib dir");
        fs::remove_file(&cpp_runtime_build_path).unwrap_or(());
        unix::fs::symlink(&cpp_runtime_path, cpp_runtime_build_path)
            .expect("Cannot create link to C++ runtime");

        println!(
            "cargo:rustc-link-search=native={}",
            build_lib_dir.to_string_lossy()
        );

        println!(
            "cargo:rustc-link-lib=static={}",
            cpp_runtime
                .chars()
                .skip("lib".len())
                .take(cpp_runtime.len() - "lib.a".len())
                .collect::<String>()
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
