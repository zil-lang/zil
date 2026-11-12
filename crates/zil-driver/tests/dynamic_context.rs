//! Concrete Context producers are retained as raw Typed input to both kernels.
use std::{collections::BTreeMap, fs, process::Command};
use zil_base::{FileId, ModuleId, ResultCategory, SourceUnit};
use zil_driver::{
    CompileOptions, NativeBackend, Optimization, ProjectSources, Stage, compile_project,
};

#[test]
fn borrowed_provider_produces_complete_typed_dependency_contract() {
    let text = r#"
struct Context<life a>{leaf:&'a i32}
struct Provider<life a>{pointer:&'a i32}
trait Read{fn read(self:&Self)->i32;}
impl<life a> Read for Provider<'a>{fn read(self:&Self)->i32{return *self.pointer}}
fn read_owned<life a>(p:Provider<'a>)->i32{
    match owned_dyn::<dyn Read,Context<'a>>(p){.err(_)=>{return 99},.ok(owner)=>{return owner.read()}}
}
fn start<life a>(value:&'a i32)->i32{
    match owned_dyn::<dyn Read,Context<'a>>(Provider::<'a>{pointer:value}){.err(_)=>{return 99},.ok(owner)=>{return owner.read()}}
}
fn main()->i32{let value=42;return start(&value)-42}
"#;
    let dir = std::env::var_os("ZIL_TEST_REPORT_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join(format!("dyn-context-typed-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("main.zil"), text).unwrap();
    let project = ProjectSources {
        root: ModuleId(0),
        modules: vec![SourceUnit::new(FileId(0), ModuleId(0), "main.zil", text)],
        paths: BTreeMap::new(),
    };
    let report = compile_project(
        &project,
        &CompileOptions {
            stop_after: Stage::Types,
            emit_dir: Some(dir.join("phases")),
            ..CompileOptions::default()
        },
    );
    fs::write(dir.join("compile.json"), report.to_json()).unwrap();
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    let limits = zil_base::Limits::default();
    let target = zil_base::TargetProfile::default();
    let source = &project.modules[0];
    let syntax = zil_syntax::parse(source).into_result().unwrap();
    let core = zil_expand::expand_with_map_limits_target_context(
        source,
        &syntax,
        &limits,
        &target,
        zil_expand::source_context::SourceIdentityContext::ClosedSnapshot {
            root: source.module_id,
            aliases: &BTreeMap::new(),
            declarations: zil_expand::source_context::DeclarationPlacement::Original,
        },
    )
    .unwrap()
    .core;
    let module = zil_types::check(&core, &target, &limits).unwrap();
    let empty = zil_ir::CandidatePlan::empty_for(&module);
    let wire = zil_ir::wire::encode_module_candidate(&module, &empty, &limits).unwrap();
    fs::write(dir.join("typed-unverified-empty-plan.wire"), &wire).unwrap();
    let (decoded, plan) = zil_ir::wire::decode_module_candidate(&wire, &limits).unwrap();
    assert_eq!(decoded, module);
    assert_eq!(plan, empty);
    let candidate = zil_analyze::analyze(&module, &limits).unwrap();
    zil_verify::verify_module(module.clone(), candidate.clone(), &target, &limits).unwrap();
    let context_table = module
        .dyn_vtables
        .iter()
        .position(|t| !t.ports.is_empty())
        .unwrap();
    let mut attacks = Vec::new();
    let mut missing = module.clone();
    missing.dyn_vtables[context_table].ports.clear();
    attacks.push(("missing-port", missing));
    let mut wrong = module.clone();
    wrong.dyn_vtables[context_table].ports[0].sources[0].path =
        vec![zil_ir::ContractProjection::Place(
            zil_ir::PlaceProjection::Field(9),
        )];
    attacks.push(("wrong-self-source", wrong));
    let mut wrong = module.clone();
    wrong.dyn_vtables[context_table].self_classes.clear();
    attacks.push(("missing-self-class", wrong));
    let mut wrong = module.clone();
    wrong.dyn_vtables[context_table].actuals[0] =
        zil_ir::DynGenericActual::Life(zil_ir::StorageBinder::Fixed(1));
    attacks.push(("wrong-impl-life", wrong));
    let mut wrong = module.clone();
    let implementation = wrong.dyn_vtables[context_table].implementation;
    wrong
        .dyn_declarations
        .iter_mut()
        .find(|d| d.id == implementation)
        .unwrap()
        .implemented_trait = None;
    attacks.push(("unknown-membership", wrong));
    for (name, attack) in attacks {
        assert!(
            zil_analyze::analyze(&attack, &limits).is_err(),
            "raw {name}"
        );
        let mut forged = candidate.clone();
        forged.module_fingerprint = attack.fingerprint();
        assert!(
            zil_verify::verify_module(attack.clone(), forged.clone(), &target, &limits).is_err(),
            "raw Verify {name}"
        );
        let wire = zil_ir::wire::encode_module_candidate(&attack, &forged, &limits).unwrap();
        let (decoded, forged) = zil_ir::wire::decode_module_candidate(&wire, &limits).unwrap();
        assert!(
            zil_analyze::analyze(&decoded, &limits).is_err(),
            "Wire {name}"
        );
        assert!(
            zil_verify::verify_module(decoded, forged, &target, &limits).is_err(),
            "Wire Verify {name}"
        );
        fs::write(dir.join(format!("attack-{name}.wire")), wire).unwrap();
    }
    let candidate_wire =
        zil_ir::wire::encode_module_candidate(&module, &candidate, &limits).unwrap();
    fs::write(dir.join("analyzed-unverified.wire"), &candidate_wire).unwrap();
    let mut backends = vec![NativeBackend::Clang];
    if cfg!(feature = "llvm-api") {
        backends.push(NativeBackend::LlvmApi);
    }
    for backend in backends {
        for optimization in [Optimization::O0, Optimization::O2] {
            let run_dir = dir.join(format!("{backend:?}-{optimization:?}"));
            fs::create_dir_all(&run_dir).unwrap();
            let executable = run_dir.join("program");
            let report = compile_project(
                &project,
                &CompileOptions {
                    backend,
                    optimization,
                    stop_after: Stage::Build,
                    output: Some(executable.clone()),
                    emit_dir: Some(run_dir.join("phases")),
                    ..CompileOptions::default()
                },
            );
            fs::write(run_dir.join("compile.json"), report.to_json()).unwrap();
            assert_eq!(
                report.status,
                ResultCategory::Success,
                "{}",
                report.to_json()
            );
            assert_eq!(report.verification_runs, 1);
            let output = Command::new(executable).output().unwrap();
            assert_eq!(
                output.status.code(),
                Some(0),
                "{backend:?} {optimization:?}: {output:?}"
            );
            assert!(output.stdout.is_empty(), "{output:?}");
            assert!(output.stderr.is_empty(), "{output:?}");
        }
    }
}
