//! Array admission must not silently open a retained array capture protocol.
use std::{fs, path::PathBuf};
use zil_base::{FileId, Limits, ModuleId, Phase, ResultCategory, SourceUnit, TargetProfile};
use zil_driver::{CompileOptions, compile_source};
use zil_ir::Type;

#[test]
fn owned_package_array_capture_is_closed_in_source_raw_and_wire() {
    let prefix = "resource R{n:i32}package P{manages r:R}fn make()->P=>P{r:R{n:42}};";
    let text = format!(
        "{prefix}fn main()->i32{{let a=[make(),make()];let f=move()=>a[0].r.n;return f()}}"
    );
    let report = compile_source(
        &SourceUnit::new(FileId(0), ModuleId(0), "capture167.zil", text),
        &CompileOptions::default(),
    );
    assert_eq!(
        report.status,
        ResultCategory::Unsupported,
        "{}",
        report.to_json()
    );
    assert_eq!(report.phase, Phase::Types, "{}", report.to_json());

    let source = SourceUnit::new(
        FileId(0),
        ModuleId(0),
        "raw-capture167.zil",
        format!("{prefix}fn main()->i32{{let p=make();let n=0;let f=move()=>n;return f()}}"),
    );
    let syntax = zil_syntax::parse(&source).into_result().unwrap();
    let core = zil_expand::expand(&source, &syntax).unwrap();
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    let mut module = zil_types::check(&core, &target, &limits).unwrap();
    let mut plan = zil_analyze::analyze(&module, &limits).unwrap();
    zil_verify::verify_module(module.clone(), plan.clone(), &target, &limits).unwrap();
    let package = module
        .functions
        .iter()
        .flat_map(|f| &f.locals)
        .find_map(|l| matches!(l.ty, Type::Package { .. }).then_some(l.ty.clone()))
        .unwrap();
    module.closures[0].captures[0] = Type::Array {
        element: Box::new(package),
        length: 2,
    };
    plan.module_fingerprint = module.fingerprint();
    let bytes = zil_ir::wire::encode_module_candidate(&module, &plan, &limits).unwrap();
    let root = std::env::var_os("ZIL_TEST_REPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let dir = root.join(format!("array-capture-boundary167-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("candidate.bin"), &bytes).unwrap();
    fs::write(dir.join("source.zil"), &source.text).unwrap();
    fs::write(dir.join("source-reject.json"), report.to_json()).unwrap();
    for (label, errors) in [
        (
            "raw",
            zil_verify::verify_module(module, plan, &target, &limits).unwrap_err(),
        ),
        (
            "wire",
            zil_verify::verify_module_bytes(&bytes, &target, &limits).unwrap_err(),
        ),
    ] {
        fs::write(dir.join(format!("{label}.txt")), format!("{errors:#?}\n")).unwrap();
        assert!(
            errors.iter().any(|d| d.phase == Phase::Verify
                && d.category == ResultCategory::Unsupported
                && d.message.contains("owned array capture")),
            "{errors:?}"
        );
    }
}
