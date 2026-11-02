//! Real source -> Types -> Analyze. Verify/native are separate slice evidence.
use zil_base::{FileId, Limits, ModuleId, SourceUnit, TargetProfile};
use zil_ir::*;
fn typed(text: &str) -> TypedModule {
    let unit = SourceUnit::new(FileId(0), ModuleId(0), "253.zil", text);
    let syntax = zil_syntax::parse(&unit).into_result().unwrap();
    let core = zil_expand::expand(&unit, &syntax).unwrap();
    zil_types::check(&core, &TargetProfile::host_llvm(), &Limits::default()).unwrap()
}
fn checked(text: &str) -> TypedModule {
    let module = typed(text);
    let limits = Limits::default();
    let plan = zil_analyze::analyze(&module, &limits).unwrap();
    let wire = zil_ir::wire::encode_module_candidate(&module, &plan, &limits).unwrap();
    let raw = zil_ir::wire::decode_module_candidate(&wire, &limits)
        .unwrap()
        .0;
    assert_eq!(zil_analyze::analyze(&raw, &limits).unwrap(), plan);
    module
}
#[test]
fn original_and_copy_and_mutable_sources_reach_analyze() {
    for source in [
        include_str!("impl252_fixtures/original.zil"),
        "fn keep<F:FnOnce(i32)->i32>(f:F)->F{return f}fn twice<F:Fn(i32)->i32>(f:F)->i32{let current=keep(f);return current(1)+current(2)}fn main()->i32{let offset=3;let f=move(n:i32)=>n+offset;let current=keep(f);return current(1)+current(2)+f(3)+twice(f)-24}",
        "fn keep<F:FnOnce(i32)->i32>(f:F)->F{return f}fn step<F:FnMut(i32)->i32>(f:F)->F{var current=f;current(1);return keep(current)}fn main()->i32{var count=0;var f=move(n:i32)=>{count+=n;return count};var current=step(f);return current(2)-3}",
    ] {
        checked(source);
    }
}

#[test]
fn six_frozen_callback_signatures_reach_analyze() {
    for (name, source) in [
        (
            "fn-copy",
            include_str!("../../../tests/impl256/frozen-prep/probes/fn-copy.zil"),
        ),
        (
            "fnmut-copy",
            include_str!("../../../tests/impl256/frozen-prep/probes/fnmut-copy.zil"),
        ),
        (
            "fnonce-copy",
            include_str!("../../../tests/impl256/frozen-prep/probes/fnonce-copy.zil"),
        ),
        (
            "fn-owned",
            include_str!("../../../tests/impl256/frozen-prep/probes/fn-owned.zil"),
        ),
        (
            "fnmut-owned",
            include_str!("../../../tests/impl256/frozen-prep/probes/fnmut-owned.zil"),
        ),
        (
            "fnonce-owned",
            include_str!("../../../tests/impl256/frozen-prep/probes/fnonce-owned.zil"),
        ),
    ] {
        let module = typed(source);
        zil_analyze::analyze(&module, &Limits::default())
            .unwrap_or_else(|e| panic!("{name}: {e:?}"));
    }
}

#[test]
fn frozen_cross_module_source_reaches_analyze_without_verify() {
    use zil_driver::{CompileOptions, ProjectSources, Stage, compile_project};
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/impl256/frozen-prep/project");
    let names = ["main", "model", "relay"];
    let project = ProjectSources {
        root: ModuleId(0),
        modules: names
            .iter()
            .enumerate()
            .map(|(index, name)| {
                let path = root.join(format!("{name}.zil"));
                SourceUnit::new(
                    FileId(index as u32),
                    ModuleId(index as u32),
                    path.to_string_lossy().into_owned(),
                    std::fs::read_to_string(path).unwrap(),
                )
            })
            .collect(),
        paths: [
            (vec!["model".into()], ModuleId(1)),
            (vec!["relay".into()], ModuleId(2)),
        ]
        .into_iter()
        .collect(),
    };
    let report = compile_project(
        &project,
        &CompileOptions {
            stop_after: Stage::Analyze,
            ..CompileOptions::default()
        },
    );
    assert_eq!(
        report.status,
        zil_base::ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 0);
}
