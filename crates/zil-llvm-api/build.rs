fn main() {
    // One checked-in pin drives the link name, runtime guard and receipts.
    // No host llvm-config lookup or implicit version fallback is permitted.
    let version = include_str!("../../tools/llvm/version.txt").trim();
    let components: Vec<u32> = version
        .split('.')
        .map(|part| {
            part.parse()
                .expect("LLVM version must contain unsigned integers")
        })
        .collect();
    assert_eq!(
        components.len(),
        3,
        "LLVM version must be major.minor.patch"
    );
    assert_eq!(
        version,
        format!("{}.{}.{}", components[0], components[1], components[2]),
        "LLVM version must be canonical"
    );
    let out = std::path::PathBuf::from(std::env::var_os("OUT_DIR").expect("Cargo OUT_DIR"));
    std::fs::write(
        out.join("llvm_version.rs"),
        format!(
            "pub const LLVM_VERSION: &str = {version:?};\n#[cfg(feature = \"native\")]\nconst LLVM_VERSION_COMPONENTS: [u32; 3] = {components:?};\n"
        ),
    )
    .expect("write pinned LLVM version");
    println!("cargo:rerun-if-changed=../../tools/llvm/version.txt");
    println!("cargo:rerun-if-env-changed=ZIL_LLVM_LIB_DIR");
    if std::env::var_os("CARGO_FEATURE_NATIVE").is_some() {
        if let Some(path) = std::env::var_os("ZIL_LLVM_LIB_DIR") {
            println!(
                "cargo:rustc-link-search=native={}",
                std::path::Path::new(&path).display()
            );
        }
        println!("cargo:rustc-link-lib=dylib=LLVM-{}", components[0]);
    }
}
