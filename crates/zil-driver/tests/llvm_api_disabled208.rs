#![cfg(not(feature = "llvm-api"))]
#[test]
fn selecting_unbuilt_backend_returns_unsupported_not_clang_fallback() {
    use zil_base::{FileId, ModuleId, ResultCategory, SourceUnit};
    use zil_driver::{CompileOptions, NativeBackend, Stage, compile_source};
    let source = SourceUnit::new(
        FileId(0),
        ModuleId(0),
        "main.zil",
        "fn main()->i32 { return 0; }",
    );
    let r = compile_source(
        &source,
        &CompileOptions {
            backend: NativeBackend::LlvmApi,
            stop_after: Stage::Build,
            ..CompileOptions::default()
        },
    );
    assert_eq!(r.status, ResultCategory::Unsupported, "{}", r.to_json());
    assert_eq!(r.diagnostics[0].code, "E-LLVM-API-UNAVAILABLE");
    assert!(r.commands.is_empty());
}
