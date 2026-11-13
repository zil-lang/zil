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
    fn checked(command: &mut Command, label: &str, receipt: &mut String) -> std::process::Output {
        receipt.push_str(&format!("{label}: {command:?}\n"));
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
    let level = std::env::var("OPT_LEVEL").expect("Cargo OPT_LEVEL");
    assert!(
        matches!(level.as_str(), "0" | "1" | "2" | "3" | "s" | "z"),
        "unsupported Cargo OPT_LEVEL"
    );
    let mut receipt =
        format!("LLVM pin: 7a941dfb2877a0b8cf30323fefa85ad019e0b0f5\nCargo OPT_LEVEL: {level}\n");
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
        &mut receipt,
    );
    let flags = String::from_utf8(flags.stdout).expect("llvm-config flags are UTF-8");
    let mut objects = Vec::new();
    for source in [
        "work_products",
        "machine_products",
        "machine_emission_products",
        "analysis_facts",
        "packed_stage",
        "stage_body",
    ] {
        let file = format!("native/{source}.cpp");
        println!("cargo:rerun-if-changed={file}");
        let object = out.join(format!("{source}.o"));
        let mut command = Command::new(&compiler);
        command
            .args(flags.split_whitespace())
            .arg("-std=c++17")
            .arg("-fPIC")
            .arg(format!("-O{level}"))
            .arg("-I")
            .arg(&include)
            .arg("-Inative")
            .arg("-c")
            .arg(&file)
            .arg("-o")
            .arg(&object);
        checked(
            &mut command,
            "compile pinned native-products adapter",
            &mut receipt,
        );
        objects.push(object);
    }
    for header in [
        "work_products.h",
        "machine_products.h",
        "machine_emission_products.h",
        "machine_emission_cfi_tests.h",
        "zil_llvm_machine_emission_hooks.h",
        "machine_frame_products.h",
        "analysis_facts.h",
        "packed_stage.h",
        "stage_body.h",
        "zil_llvm_work_products_hooks.h",
        "X86MachineFunctionInfo.h",
    ] {
        println!("cargo:rerun-if-changed=native/{header}");
    }
    let archive = out.join("libzil_native_products.a");
    let mut ar = Command::new(clang.with_file_name("llvm-ar"));
    ar.arg("crs").arg(&archive).args(&objects);
    checked(&mut ar, "archive native-products adapter", &mut receipt);
    std::fs::write(out.join("native_products_build_commands.txt"), receipt)
        .expect("write native adapter build commands");
    println!("cargo:rustc-link-search=native={}", out.display());
    println!("cargo:rustc-link-lib=static=zil_native_products");
    println!("cargo:rustc-link-lib=dylib=stdc++");
    println!("cargo:rustc-link-lib=dylib=dl");
}
