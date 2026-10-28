fn main() {
    println!("cargo:rerun-if-env-changed=ZIL_LLVM_LIB_DIR");
    if std::env::var_os("CARGO_FEATURE_NATIVE").is_some() {
        if let Some(path) = std::env::var_os("ZIL_LLVM_LIB_DIR") {
            println!(
                "cargo:rustc-link-search=native={}",
                std::path::Path::new(&path).display()
            );
        }
        println!("cargo:rustc-link-lib=dylib=LLVM-17");
    }
}
