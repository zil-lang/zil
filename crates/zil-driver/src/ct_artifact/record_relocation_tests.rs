//! Real Source controls for recursive, source-authenticated pure value copies.
use super::*;
use zil_base::ResultCategory;

fn fixtures() -> [(&'static str, &'static str); 2] {
    [
        ("original", include_str!("../../../../tests/language-readiness/terminal/344/anonymous/ct6-cross-factory-pure-composites.zil")),
        ("recursive", include_str!("../../../../tests/language-readiness/terminal/344/anonymous/ct6-recursive-cross-schema-composites.zil")),
    ]
}

#[test]
fn g4454_actual_recursive_values_reach_runtime_types_and_verify() {
    for (name, text) in fixtures() {
        let source = SourceUnit::new(zil_base::FileId(0), ModuleId(0), format!("{name}.zil"), text);
        let dir = std::env::temp_dir().join(format!("g4454-types-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let report = crate::compile_source(&source, &crate::CompileOptions {
            stop_after: crate::Stage::Types,
            emit_dir: Some(dir.clone()),
            ..crate::CompileOptions::default()
        });
        assert_eq!(report.status, ResultCategory::Success, "{}", report.to_json());
        let typed = std::fs::read_to_string(report.artifacts.get("typed").unwrap()).unwrap();
        for opcode in ["MakeAggregate", "ExtractField", "ExtractIndex", "MakeVariant", "VariantTag", "ReadVariant"] {
            assert!(typed.contains(opcode), "{name}: actual Typed omitted {opcode}");
        }
        let report = crate::compile_source(&source, &crate::CompileOptions::default());
        assert_eq!(report.status, ResultCategory::Success, "{}", report.to_json());
        assert_eq!(report.verification_runs, 1);
        std::fs::remove_dir_all(dir).unwrap();
    }
}

#[test]
fn g4454_actual_recursive_values_preserve_wide_bits_native_o0_o2() {
    for (name, text) in fixtures() {
        let source = SourceUnit::new(zil_base::FileId(0), ModuleId(0), format!("{name}.zil"), text);
        for optimization in [zil_backend_llvm::Optimization::O0, zil_backend_llvm::Optimization::O2] {
            let dir = std::env::temp_dir().join(format!("g4454-native-{name}-{}-{optimization:?}", std::process::id()));
            std::fs::create_dir_all(&dir).unwrap();
            let report = crate::compile_source(&source, &crate::CompileOptions {
                backend: zil_backend_llvm::NativeBackend::Clang,
                stop_after: crate::Stage::Build,
                optimization,
                entry: Some("main".into()),
                output: Some(dir.join("program")),
                ..crate::CompileOptions::default()
            });
            assert_eq!(report.status, ResultCategory::Success, "{}", report.to_json());
            let output = std::process::Command::new(report.artifacts.get("executable").unwrap()).output().unwrap();
            assert!(output.status.success(), "{output:?}");
            assert_eq!(output.stdout, b"18446744073709551557\n");
            eprintln!("G4.4.5.4 {name} Clang {optimization:?}: full u64=18446744073709551557");
            std::fs::remove_dir_all(dir).unwrap();
        }
    }
}
