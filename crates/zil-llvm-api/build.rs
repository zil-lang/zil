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
    if std::env::var_os("CARGO_FEATURE_NATIVE_PRODUCTS").is_some() {
        build_native_products(&out);
    }
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

fn build_native_products(out: &std::path::Path) {
    use std::process::Command;
    fn checked(command: &mut Command, label: &str) -> std::process::Output {
        let output = command
            .output()
            .unwrap_or_else(|error| panic!("{label}: {error}"));
        assert!(
            output.status.success(),
            "{label}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        output
    }
    for name in ["ZIL_LLVM_INCLUDE_DIR", "ZIL_LLVM_CONFIG", "ZIL_CLANG"] {
        println!("cargo:rerun-if-env-changed={name}");
    }
    let include = std::env::var_os("ZIL_LLVM_INCLUDE_DIR")
        .expect("native-products requires pinned ZIL_LLVM_INCLUDE_DIR");
    let config = std::env::var_os("ZIL_LLVM_CONFIG")
        .expect("native-products requires pinned ZIL_LLVM_CONFIG");
    let clang = std::path::PathBuf::from(
        std::env::var_os("ZIL_CLANG").expect("native-products requires pinned ZIL_CLANG"),
    );
    let compiler = clang.with_file_name("clang++");
    let flags = checked(
        Command::new(&config).arg("--cxxflags"),
        "pinned llvm-config --cxxflags",
    );
    let flags = String::from_utf8(flags.stdout).expect("llvm-config flags are UTF-8");
    let mut objects = Vec::new();
    for source in ["work_products", "machine_products"] {
        let file = format!("native/{source}.cpp");
        println!("cargo:rerun-if-changed={file}");
        let object = out.join(format!("{source}.o"));
        let mut command = Command::new(&compiler);
        command
            .args(flags.split_whitespace())
            .arg("-std=c++17")
            .arg("-fPIC")
            .arg("-I")
            .arg(&include)
            .arg("-Inative")
            .arg("-c")
            .arg(&file)
            .arg("-o")
            .arg(&object);
        checked(&mut command, "compile pinned native-products adapter");
        objects.push(object);
    }
    for header in [
        "work_products.h",
        "machine_products.h",
        "zil_llvm_work_products_hooks.h",
    ] {
        println!("cargo:rerun-if-changed=native/{header}");
    }
    let archive = out.join("libzil_native_products.a");
    let mut ar = Command::new(clang.with_file_name("llvm-ar"));
    ar.arg("crs").arg(&archive).args(&objects);
    checked(&mut ar, "archive native-products adapter");
    println!("cargo:rustc-link-search=native={}", out.display());
    println!("cargo:rustc-link-lib=static=zil_native_products");
    println!("cargo:rustc-link-lib=dylib=stdc++");
    println!("cargo:rustc-link-lib=dylib=dl");
}
