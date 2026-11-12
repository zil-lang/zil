use zil_base::{FileId, Limits, ModuleId, SourceUnit, TargetProfile};
const SOURCE: &str = "fn make(a:&i32,b:&i32)->impl Fn()->i32{return move()=>*a+*b}fn main()->i32{let a=20;let b=22;let f=make(&a,&b);return f()-42}";
#[test]
fn returned_borrowed_environment_has_actual_external_source_proposals() {
    let source = SourceUnit::new(FileId(0), ModuleId(0), "returned-borrow.zil", SOURCE);
    let syntax = zil_syntax::parse(&source).into_result().unwrap();
    let core = zil_expand::expand(&source, &syntax).unwrap();
    let module = zil_types::check(&core, &TargetProfile::host_llvm(), &Limits::default()).unwrap();
    let factory = module.functions.iter().find(|f| f.name == "make").unwrap();
    let c = factory.callable_return_contract.as_ref().unwrap();
    assert_eq!(c.capture_sources.as_ref().unwrap().len(), 2);
    let plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    zil_verify::verify_module(module.clone(), plan, &module.target, &Limits::default()).unwrap();
}

const PROJECT_MODEL: &str = r#"
extern "C" fn putchar(n:i32)->i32;
pub resource Mark{pub code:i32}
impl Drop for Mark{fn drop(self:Self){unsafe{putchar(.code)}}}
pub struct Pair{pub left:i32,pub right:i32}
pub package Owner{manages marker:Mark;pub bias:i32}
pub fn owner(code:i32,bias:i32)->Owner{return Owner{marker:Mark{code:code},bias:bias}}
"#;
const PROJECT_FACTORY: &str = r#"
import model;
fn choose<life a>(left:&'a model.Pair,right:&'a model.Pair,first:bool)->&'a i32{
 if first{return &left.left}
 return &right.right
}
pub fn make<life a>(owner:model.Owner,left:&'a model.Pair,right:&'a model.Pair,first:bool)->impl Fn()->i32{
 let target=choose(left,right,first)
 return move()=>owner.bias+*target
}
"#;
const PROJECT_MAIN: &str = r#"
import model;import factory;
fn main()->i32{
 let left=model.Pair{left:20,right:91}
 let right=model.Pair{left:92,right:22}
 let f=factory.make(model.owner(65,1),&left,&right,true)
 let g=factory.make(model.owner(66,2),&left,&right,false)
 let saved=f
 return saved()+g()-45
}
"#;

#[test]
fn projected_multiple_sources_and_owned_capture_compose_across_modules() {
    use std::{collections::BTreeMap, fs, process::Command};
    use zil_driver::{CompileOptions, NativeBackend, Optimization, ProjectSources, Stage};
    let dir = std::env::temp_dir().join(format!("zil-returned-source-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let project = ProjectSources {
        root: ModuleId(0),
        modules: vec![
            SourceUnit::new(FileId(0), ModuleId(0), "main.zil", PROJECT_MAIN),
            SourceUnit::new(FileId(1), ModuleId(1), "model.zil", PROJECT_MODEL),
            SourceUnit::new(FileId(2), ModuleId(2), "factory.zil", PROJECT_FACTORY),
        ],
        paths: BTreeMap::from([
            (vec!["model".into()], ModuleId(1)),
            (vec!["factory".into()], ModuleId(2)),
        ]),
    };
    let mut backends = vec![NativeBackend::Clang];
    if cfg!(feature = "llvm-api") {
        backends.push(NativeBackend::LlvmApi)
    }
    for backend in backends {
        for optimization in [Optimization::O0, Optimization::O2] {
            let output = dir.join("program");
            let report = zil_driver::compile_project(
                &project,
                &CompileOptions {
                    stop_after: Stage::Build,
                    backend,
                    optimization,
                    output: Some(output.clone()),
                    ..CompileOptions::default()
                },
            );
            assert_eq!(
                report.status,
                zil_base::ResultCategory::Success,
                "{}",
                report.to_json()
            );
            assert_eq!(report.verification_runs, 1);
            let generated = report.native_codegen.as_ref().unwrap();
            assert_eq!(generated.backend, backend);
            assert!(generated.fallback.is_none());
            let executed = Command::new(output).output().unwrap();
            assert_eq!(executed.status.code(), Some(0), "{executed:?}");
            assert_eq!(executed.stdout, b"AB");
            assert!(executed.stderr.is_empty());
        }
    }
    fs::remove_dir_all(dir).unwrap();
}

fn typed_source(text: &str) -> zil_ir::TypedModule {
    typed_source_with_limit(text, &Limits::default()).unwrap()
}
fn typed_source_with_limit(
    text: &str,
    typing_limits: &Limits,
) -> zil_base::CompileResult<zil_ir::TypedModule> {
    let source = SourceUnit::new(FileId(0), ModuleId(0), "source-proof.zil", text);
    let syntax = zil_syntax::parse(&source).into_result().unwrap();
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    let core = zil_expand::expand_with_map_limits_target_context(
        &source,
        &syntax,
        &limits,
        &target,
        zil_expand::source_context::SourceIdentityContext::ClosedSnapshot {
            root: source.module_id,
            aliases: &Default::default(),
            declarations: zil_expand::source_context::DeclarationPlacement::Original,
        },
    )
    .unwrap()
    .core;
    zil_types::check(&core, &target, typing_limits)
}
#[test]
fn analyze_reconstructs_sources_when_same_typed_operands_or_claims_are_forged() {
    use zil_ir::{InstructionKind as I, StorageBase};
    let original = typed_source(SOURCE);
    zil_analyze::analyze(&original, &Limits::default()).unwrap();
    for mutation in 0..6 {
        let mut module = original.clone();
        let factory = module
            .functions
            .iter_mut()
            .find(|f| f.name == "make")
            .unwrap();
        if mutation == 0 {
            let mut changes = 0;
            for instruction in factory.blocks.iter_mut().flat_map(|b| &mut b.instructions) {
                if let I::MakeClosure { captures, .. } = &mut instruction.kind {
                    captures[0] = captures[1];
                    changes += 1;
                }
            }
            assert_eq!(changes, 1);
        } else {
            let c = factory.callable_return_contract.as_mut().unwrap();
            match mutation {
                1 => {
                    exact_source_mut(&mut c.capture_sources.as_mut().unwrap()[0].sources[0]).base =
                        StorageBase::Parameter(1)
                }
                2 => c.capture_sources = Some(vec![]),
                3 => c.capture_sources = None,
                4 => {
                    exact_source_mut(&mut c.capture_sources.as_mut().unwrap()[0].sources[0]).base =
                        StorageBase::Parameter(99)
                }
                _ => {
                    c.capture_sources.as_mut().unwrap()[0].position.base = StorageBase::Capture(99)
                }
            }
        }
        let errors =
            zil_analyze::analyze(&module, &Limits::default()).expect_err("forged capture source");
        assert!(
            errors.iter().any(|d| d.phase == zil_base::Phase::Analyze
                && d.category == zil_base::ResultCategory::SafeReject),
            "{mutation}: {errors:?}"
        );
    }
}
#[test]
fn local_storage_cannot_be_laundered_as_a_new_external_environment() {
    for text in [
        "fn make(n:i32)->impl Fn()->i32{return ()=>n}fn main(){}",
        "fn make(a:&i32)->impl Fn()->i32{let n=7;let p=&n;return move()=>*p}fn main(){}",
    ] {
        let source = SourceUnit::new(FileId(0), ModuleId(0), "local-source.zil", text);
        let syntax = zil_syntax::parse(&source).into_result().unwrap();
        let core = zil_expand::expand(&source, &syntax).unwrap();
        let errors = zil_types::check(&core, &TargetProfile::host_llvm(), &Limits::default())
            .expect_err("local source must not escape");
        assert!(
            errors.iter().any(|d| d.code == "E-CALLABLE-RETURN-ORIGIN"),
            "{errors:?}"
        );
    }
}

#[test]
fn analyze_keeps_returned_capture_loans_until_the_actual_last_use() {
    const SHARED: &str = "fn make(p:&i32)->impl Fn()->i32{return move()=>*p}";
    const EXCLUSIVE: &str =
        "fn make(p:&mut i32)->impl FnMut()->i32{return move()=>{*p+=1;return *p}}";
    for source in [
        format!("{SHARED}fn main()->i32{{var n=1;let f=make(&n);n=9;return f()}}"),
        format!("{EXCLUSIVE}fn main()->i32{{var n=1;var f=make(&mut n);n=9;return f()}}"),
    ] {
        let module = typed_source(&source);
        let errors = zil_analyze::analyze(&module, &Limits::default())
            .expect_err("captured parent must remain live");
        assert!(
            errors.iter().any(|d| d.code == "E-BORROW-CONFLICT"
                || d.code == "E-ANALYZE-IR"
                    && d.message.contains("actual stored target/parent loan")),
            "{errors:?}"
        );
    }
    for source in [
        format!(
            "{SHARED}fn main()->i32{{var n=1;let f=make(&n);let answer=f();n=9;return answer+n}}"
        ),
        format!(
            "{EXCLUSIVE}fn main()->i32{{var n=1;var f=make(&mut n);let answer=f();n=9;return answer+n}}"
        ),
    ] {
        let module = typed_source(&source);
        zil_analyze::analyze(&module, &Limits::default()).unwrap();
    }
}

const HOLDER_SOURCE: &str = r#"
struct Holder<life a>{pointer:&'a i32}
fn hold<life a>(pointer:&'a i32)->Holder<'a>{return Holder::<'a>{pointer:pointer}}
fn make<life a>(holder:&Holder<'a>)->impl Fn()->i32{let pointer=holder.pointer;return move()=>*pointer}
fn main()->i32{let n=41;let holder=hold(&n);let f=make(&holder);return f()-41}
"#;
#[test]
fn analyze_returns_a_stored_leaf_with_its_actual_target_and_parent_loans() {
    let module = typed_source(HOLDER_SOURCE);
    let plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    assert!(
        plan.functions
            .iter()
            .flat_map(|f| &f.holds)
            .any(|hold| !hold.roots.is_empty())
    );
}

#[test]
fn stored_factory_keeps_the_original_slot_target_after_later_replacement() {
    let head = HOLDER_SOURCE.split("fn main").next().unwrap();
    let body = "fn main()->i32{var n:i32=41;let other:i32=9;var holder=hold(&n);let f=make(&holder);holder.pointer=&other;return f()-41}";
    let module = typed_source(&format!("{head}{body}"));
    zil_analyze::analyze(&module, &Limits::default()).unwrap();
    let bad = body.replace("return f()-41", "n=42;return f()-41");
    let module = typed_source(&format!("{head}{bad}"));
    let errors = zil_analyze::analyze(&module, &Limits::default())
        .expect_err("returned environment still borrows the actual old slot target");
    assert!(
        errors
            .iter()
            .any(|d| d.code == "E-ANALYZE-IR"
                && d.message.contains("actual stored target/parent loan")),
        "{errors:?}"
    );
}

#[test]
fn analyze_transfers_real_owned_and_borrowed_fields_into_one_returned_environment() {
    let source = r#"
resource R{code:i32}
package Mixed<life a>{manages owned:R;pointer:&'a i32}
fn wrap<life a>(pointer:&'a i32)->Mixed<'a>{return Mixed::<'a>{owned:R{code:1},pointer:pointer}}
fn make<life a>(holder:Mixed<'a>)->impl Fn()->i32{let pointer=holder.pointer;let owned=give holder.owned;return move()=>owned.code+*pointer}
fn main()->i32{let n=41;let f=make(wrap(&n));return f()-42}
"#;
    let module = typed_source(source);
    let factory = module.functions.iter().find(|f| f.name == "make").unwrap();
    let contract = factory.callable_return_contract.as_ref().unwrap();
    assert_eq!(contract.cleanup.len(), 1);
    assert_eq!(contract.capture_sources.as_ref().unwrap().len(), 1);
    zil_analyze::analyze(&module, &Limits::default()).unwrap();
}

#[test]
fn emit_untrusted_factory_candidates_for_independent_verify() {
    use std::fs;
    let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../out/runs/returned-source-candidates-recovery-20261112");
    fs::create_dir_all(&directory).unwrap();
    let owned = format!("{PROJECT_MODEL}{PROJECT_FACTORY}{PROJECT_MAIN}")
        .replace("import model;", "")
        .replace("import factory;", "")
        .replace("model.", "")
        .replace("factory.", "");
    let requested = std::env::var("ZIL_RETURNED_SOURCE_FILTER").ok();
    let mut emitted = 0;
    for (label, text) in [
        ("inline-nested-qualified-callback",include_str!("../../../tests/fixtures/returned-owning-holders/inline-nested-qualified-callback.zil").to_owned()),
        (
            "nested-qualified-callback",
            include_str!(
                "../../../tests/fixtures/returned-owning-holders/nested-qualified-callback.zil"
            )
            .to_owned(),
        ),
        (
            "external-mutable-holder-helper",
            include_str!(
                "../../../tests/fixtures/returned-owning-holders/external-mutable-holder-helper.zil"
            )
            .to_owned(),
        ),
        (
            "external-mutable-callback",
            include_str!(
                "../../../tests/fixtures/returned-owning-holders/external-mutable-callback.zil"
            )
            .to_owned(),
        ),
        (
            "call-before-return",
            include_str!("../../../tests/fixtures/returned-owning-holders/call-before-return.zil")
                .to_owned(),
        ),
        (
            "forward-qualified",
            include_str!("../../../tests/fixtures/returned-owning-holders/forward-qualified.zil")
                .to_owned(),
        ),
        (
            "forward-lending",
            include_str!("../../../tests/fixtures/returned-owning-holders/forward-lending.zil")
                .to_owned(),
        ),
        (
            "forward-borrowed",
            include_str!("../../../tests/fixtures/returned-owning-holders/forward-borrowed.zil")
                .to_owned(),
        ),
        (
            "mutable-whole",
            include_str!("../../../tests/fixtures/returned-owning-holders/mutable-whole.zil")
                .to_owned(),
        ),
        ("two-addresses", SOURCE.to_owned()),
        ("owned-projected-multiple", owned),
        (
            "whole-qualified-owner",
            include_str!("../../../tests/fixtures/returned-owning-holders/whole-mixed.zil")
                .to_owned(),
        ),
        ("loop-sources", LOOP_BORROWED.to_owned()),
        ("nested-borrowed", NESTED_BORROWED.to_owned()),
        ("wrapped-borrowed", WRAPPED_BORROWED.to_owned()),
        ("holder-pointer", HOLDER_SOURCE.to_owned()),
        (
            "holder-pointer-replaced",
            format!(
                "{}{}",
                HOLDER_SOURCE.split("fn main").next().unwrap(),
                "fn main()->i32{var n:i32=41;let other:i32=9;var holder=hold(&n);let f=make(&holder);holder.pointer=&other;return f()-41}"
            ),
        ),
    ] {
        if requested.as_deref().is_some_and(|requested| requested != label) { continue; }
        emitted += 1;
        let module = typed_source(&text);
        let candidate = zil_analyze::analyze(&module, &Limits::default()).unwrap();
        let wire =
            zil_ir::wire::encode_module_candidate(&module, &candidate, &Limits::default()).unwrap();
        fs::write(directory.join(format!("{label}.zil")), text).unwrap();
        fs::write(directory.join(format!("{label}.wire")), wire).unwrap();
        fs::write(
            directory.join(format!("{label}.typed.txt")),
            format!("{module:#?}"),
        )
        .unwrap();
        fs::write(
            directory.join(format!("{label}.candidate.txt")),
            format!("{candidate:#?}"),
        )
        .unwrap();
    }
    assert!(
        emitted > 0,
        "requested original source must actually be reconstructed"
    );
}

const NESTED_BORROWED: &str = r#"
fn make<life a>(pointer:&'a i32)->impl Fn()->i32{
 let inner=move()=>*pointer
 return move()=>inner()
}
fn main()->i32{let value=41;let outer=make(&value);return outer()-41}
"#;
const WRAPPED_BORROWED: &str = r#"
fn wrap<F:Fn()->i32>(input:F)->impl Fn()->i32{return move()=>input()}
fn main()->i32{let value=41;let pointer=&value;let inner=move()=>*pointer;let outer=wrap(inner);return outer()-41}
"#;
#[test]
fn nested_borrowed_environments_propose_each_actual_leaf() {
    for text in [NESTED_BORROWED, WRAPPED_BORROWED] {
        let module = typed_source(text);
        let contract = module
            .functions
            .iter()
            .find_map(|function| function.callable_return_contract.as_ref())
            .unwrap();
        assert!(
            contract
                .capture_sources
                .as_ref()
                .unwrap()
                .iter()
                .any(|binding| binding
                    .position
                    .path
                    .iter()
                    .any(|step| matches!(step, zil_ir::ContractProjection::Capture(_))))
        );
        zil_analyze::analyze(&module, &Limits::default()).unwrap();
    }
}

const LOOP_BORROWED: &str = r#"
fn make(a:&i32,b:&i32,n:i32)->impl Fn()->i32{
 var pointer=a;var i=0
 while i<n{pointer=b;i+=1}
 return move()=>*pointer
}
fn main()->i32{let a=20;let b=22;let first=make(&a,&b,0);let second=make(&a,&b,1);return first()+second()-42}
"#;
#[test]
fn loop_source_equations_keep_both_actual_inputs() {
    let cycle = LOOP_BORROWED.replace(
        "pointer=b;i+=1",
        "if i==0{pointer=b}else{pointer=pointer};i+=1",
    );
    let recurrent = typed_source(&cycle);
    zil_analyze::analyze(&recurrent, &Limits::default()).unwrap();
    let module = typed_source(LOOP_BORROWED);
    zil_analyze::analyze(&module, &Limits::default()).unwrap();
    let factory = module
        .functions
        .iter()
        .position(|function| function.callable_return_contract.is_some())
        .unwrap();
    let bindings = module.functions[factory]
        .callable_return_contract
        .as_ref()
        .unwrap()
        .capture_sources
        .as_ref()
        .unwrap();
    assert_eq!(bindings[0].sources.len(), 2);
    for omitted in 0..2 {
        let mut bad = module.clone();
        bad.functions[factory]
            .callable_return_contract
            .as_mut()
            .unwrap()
            .capture_sources
            .as_mut()
            .unwrap()[0]
            .sources
            .remove(omitted);
        assert!(zil_analyze::analyze(&bad, &Limits::default()).is_err());
    }
}

fn check_raw_and_wire(text: &str) {
    let module = typed_source(text);
    let candidate = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    zil_verify::verify_module(
        module.clone(),
        candidate.clone(),
        &module.target,
        &Limits::default(),
    )
    .unwrap();
    let wire =
        zil_ir::wire::encode_module_candidate(&module, &candidate, &Limits::default()).unwrap();
    zil_verify::verify_module_bytes(&wire, &module.target, &Limits::default()).unwrap();
}
fn check_native_factory(text: &str, stdout: &[u8]) {
    use zil_driver::{CompileOptions, NativeBackend, Optimization, Stage};
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let mut backends = vec![NativeBackend::Clang];
    if cfg!(feature = "llvm-api") {
        backends.push(NativeBackend::LlvmApi);
    }
    for backend in backends {
        for optimization in [Optimization::O0, Optimization::O2] {
            let directory = std::env::temp_dir().join(format!(
                "zil-factory-native-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            ));
            std::fs::create_dir(&directory).unwrap();
            let executable = directory.join("program");
            let report = zil_driver::compile_source(
                &SourceUnit::new(FileId(0), ModuleId(0), "factory-native.zil", text),
                &CompileOptions {
                    stop_after: Stage::Build,
                    backend,
                    optimization,
                    output: Some(executable.clone()),
                    ..CompileOptions::default()
                },
            );
            assert_eq!(
                report.status,
                zil_base::ResultCategory::Success,
                "{}",
                report.to_json()
            );
            assert_eq!(report.verification_runs, 1);
            assert_eq!(report.native_codegen.as_ref().unwrap().backend, backend);
            assert!(report.native_codegen.as_ref().unwrap().fallback.is_none());
            let actual = std::process::Command::new(executable).output().unwrap();
            assert_eq!(actual.status.code(), Some(0), "{actual:?}");
            assert_eq!(actual.stdout, stdout, "{actual:?}");
            assert!(actual.stderr.is_empty());
            std::fs::remove_dir_all(directory).unwrap();
        }
    }
}

#[test]
fn actual_holder_slot_epochs_and_mixed_environment_cleanup_reach_machine_code() {
    let head = HOLDER_SOURCE.split("fn main").next().unwrap();
    let replaced = format!(
        "{head}fn main()->i32{{var n:i32=41;let other:i32=9;var holder=hold(&n);let f=make(&holder);holder.pointer=&other;let answer=f();n=42;return answer+n-83}}"
    );
    check_raw_and_wire(&replaced);
    check_native_factory(&replaced, b"");
    let mixed = r#"
extern "C" fn putchar(value:i32)->i32;
resource R{code:i32}
impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
package Mixed<life a>{manages owned:R;pointer:&'a i32}
fn wrap<life a>(pointer:&'a i32)->Mixed<'a>{return Mixed::<'a>{owned:R{code:65},pointer:pointer}}
fn make<life a>(holder:Mixed<'a>)->impl Fn()->i32{let pointer=holder.pointer;let owned=give holder.owned;return move()=>owned.code+*pointer}
fn run(fail:bool)->void!i32{let n=1;let f=make(wrap(&n));if fail{return ()};return f()-66}
fn main()->i32{match run(true){.err(_)=>{},.ok(_)=>{return 1}};match run(false){.err(_)=>{return 2},.ok(value)=>{return value}}}
"#;
    check_raw_and_wire(mixed);
    check_native_factory(mixed, b"AA");
}

const MULTI_NESTED: &str = r#"
fn inner(pointer:&i32)->impl Fn()->i32{return move()=>*pointer}
fn make(left:&i32,right:&i32)->impl Fn()->i32{
 let first=inner(left);let second=inner(right)
 return move()=>first()+second()
}
fn main()->i32{let left=20;let right=22;let saved=make(&left,&right);return saved()-42}
"#;
#[test]
fn nested_and_wrapped_actual_environments_reach_both_backends() {
    for text in [
        NESTED_BORROWED,
        WRAPPED_BORROWED,
        LOOP_BORROWED,
        MULTI_NESTED,
    ] {
        check_raw_and_wire(text);
        check_native_factory(text, b"");
    }
    let module = typed_source(MULTI_NESTED);
    let factory = module
        .functions
        .iter()
        .position(|function| function.name == "make")
        .unwrap();
    let mut forged = module.clone();
    let instruction = forged.functions[factory]
        .blocks
        .iter_mut()
        .flat_map(|block| &mut block.instructions)
        .find(|instruction| {
            matches!(
                instruction.kind,
                zil_ir::InstructionKind::MakeClosure { .. }
            )
        })
        .unwrap();
    let zil_ir::InstructionKind::MakeClosure { captures, .. } = &mut instruction.kind else {
        unreachable!()
    };
    captures.swap(0, 1);
    assert!(zil_analyze::analyze(&forged, &Limits::default()).is_err());
    let mut dishonest = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    dishonest.module_fingerprint = forged.fingerprint();
    assert!(
        zil_verify::verify_module(
            forged.clone(),
            dishonest.clone(),
            &forged.target,
            &Limits::default()
        )
        .is_err()
    );
    let wire =
        zil_ir::wire::encode_module_candidate(&forged, &dishonest, &Limits::default()).unwrap();
    assert!(zil_verify::verify_module_bytes(&wire, &forged.target, &Limits::default()).is_err());
}

#[test]
fn whole_qualified_owner_capture_has_owner_and_leaf_responsibilities() {
    let source = include_str!("../../../tests/fixtures/returned-owning-holders/whole-mixed.zil");
    let module = typed_source(source);
    let factory = module
        .functions
        .iter()
        .find(|function| function.name == "make")
        .unwrap();
    let contract = factory.callable_return_contract.as_ref().unwrap();
    assert_eq!(contract.cleanup, vec![0]);
    assert_eq!(contract.hold, vec![0]);
    assert!(!contract.capture_sources.as_ref().unwrap().is_empty());
    assert!(
        module.closures[contract.closure.0 as usize]
            .storage_life
            .is_some()
    );
    zil_analyze::analyze(&module, &Limits::default()).unwrap();
    check_raw_and_wire(source);
    check_native_factory(source, b"AA");
}

#[test]
fn mutable_qualified_owner_publishes_actual_environment_slot_epoch() {
    let source = include_str!("../../../tests/fixtures/returned-owning-holders/mutable-whole.zil");
    let module = typed_source(source);
    zil_analyze::analyze(&module, &Limits::default()).unwrap();
    check_raw_and_wire(source);
    check_native_factory(source, b"A");
}

#[test]
fn mutable_environment_release_tracks_actual_old_and_new_targets() {
    let source = include_str!("../../../tests/fixtures/returned-owning-holders/mutable-whole.zil");
    let head = source.split("fn main").next().unwrap();
    for (body, accepted) in [
        (
            "fn main()->i32{var a=20;var b=22;var f=make(wrap(&a),&b);let first=f();a=21;let second=f();return first+second+a-63}",
            true,
        ),
        (
            "fn main()->i32{var a=20;var b=22;var f=make(wrap(&a),&b);a=21;let first=f();return first-21}",
            false,
        ),
        (
            "fn main()->i32{var a=20;var b=22;var f=make(wrap(&a),&b);let first=f();b=23;let second=f();return first+second-43}",
            false,
        ),
        (
            "fn main()->i32{var a=20;var b=22;var f=make(wrap(&a),&b);let first=f();let second=f();b=23;return first+second+b-65}",
            true,
        ),
        (
            "fn main()->i32{var a=10;var b=11;var c=20;var d=21;var f=make(wrap(&a),&b);var g=make(wrap(&c),&d);let first=f();a=1;let old=g();let next=f();let last=g();return first+old+next+last-62}",
            true,
        ),
        (
            "fn main()->i32{var a=10;var b=11;var c=20;var d=21;var f=make(wrap(&a),&b);var g=make(wrap(&c),&d);let first=f();c=1;let old=g();return first+old-11}",
            false,
        ),
    ] {
        let text = format!("{head}{body}");
        let module = typed_source(&text);
        let result = zil_analyze::analyze(&module, &Limits::default());
        assert_eq!(result.is_ok(), accepted, "{body}: {result:?}");
    }
}

#[test]
fn ordinary_parameter_forwarding_keeps_each_actual_external_environment() {
    let source =
        include_str!("../../../tests/fixtures/returned-owning-holders/forward-borrowed.zil");
    let module = typed_source(source);
    zil_analyze::analyze(&module, &Limits::default()).unwrap();
}

#[test]
fn mutable_environment_raw_and_wire_reconstruct_both_slot_epochs() {
    let source = include_str!("../../../tests/fixtures/returned-owning-holders/mutable-whole.zil");
    let module = typed_source(source);
    let plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    for (target, after_call, accepted) in [
        ("a", 0, false),
        ("a", 1, true),
        ("b", 1, false),
        ("b", 2, true),
    ] {
        let mut changed = module.clone();
        let main = changed
            .functions
            .iter()
            .position(|f| f.name == "main")
            .unwrap();
        let f = &mut changed.functions[main];
        let local = f
            .locals
            .iter()
            .find(|local| local.name == target)
            .unwrap()
            .id;
        f.locals[local.0 as usize].mutable = true;
        let value = zil_ir::ValueId(f.values.len() as u32);
        f.values.push(zil_ir::Type::I32);
        let block = f
            .blocks
            .iter_mut()
            .find(|block| {
                block
                    .instructions
                    .iter()
                    .any(|i| matches!(i.kind, zil_ir::InstructionKind::CallClosureMut { .. }))
            })
            .unwrap();
        let calls = block
            .instructions
            .iter()
            .enumerate()
            .filter_map(|(index, i)| {
                matches!(i.kind, zil_ir::InstructionKind::CallClosureMut { .. }).then_some(index)
            })
            .collect::<Vec<_>>();
        let at = if after_call == 0 {
            calls[0]
        } else {
            calls[after_call - 1] + 1
        };
        block.instructions.splice(
            at..at,
            [
                zil_ir::Instruction {
                    result: Some(value),
                    kind: zil_ir::InstructionKind::Constant(zil_ir::Constant::I32(99)),
                    span: f.span,
                },
                zil_ir::Instruction {
                    result: None,
                    kind: zil_ir::InstructionKind::Store { local, value },
                    span: f.span,
                },
            ],
        );
        let mut claim = plan.clone();
        claim.module_fingerprint = changed.fingerprint();
        claim.functions[main].holds.push(zil_ir::ValueHold {
            value,
            roots: Vec::new(),
        });
        let raw = zil_verify::verify_module(
            changed.clone(),
            claim.clone(),
            &changed.target,
            &Limits::default(),
        );
        let wire =
            zil_ir::wire::encode_module_candidate(&changed, &claim, &Limits::default()).unwrap();
        let decoded = zil_verify::verify_module_bytes(&wire, &changed.target, &Limits::default());
        assert_eq!(raw.is_ok(), accepted, "{target}/{after_call}: {raw:?}");
        assert_eq!(
            decoded.is_ok(),
            accepted,
            "Wire {target}/{after_call}: {decoded:?}"
        );
        assert_eq!(
            zil_analyze::analyze(&changed, &Limits::default()).is_ok(),
            accepted
        );
    }
}

#[test]
fn forwarded_lending_buffer_environments_preserve_real_lending_indices() {
    let source =
        include_str!("../../../tests/fixtures/returned-owning-holders/forward-lending.zil");
    let module = typed_source(source);
    let helpers = module
        .functions
        .iter()
        .filter(|f| {
            matches!(
                f.callable_return_contract.as_ref().map(|c| c.source),
                Some(zil_ir::CallableReturnSource::Parameter(_))
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(helpers.len(), 2);
    for helper in helpers {
        let c = helper.callable_return_contract.as_ref().unwrap();
        assert_eq!(c.lending_captures, [0]);
        assert_eq!(c.cleanup, [0]);
        assert_eq!(
            c.lending_captures,
            module.closures[c.closure.0 as usize].lending_captures
        );
    }
    zil_analyze::analyze(&module, &Limits::default()).unwrap();
}

#[test]
fn qualified_owner_forwarding_preserves_late_actual_free_classes() {
    let source =
        include_str!("../../../tests/fixtures/returned-owning-holders/forward-qualified.zil");
    let module = typed_source(source);
    zil_analyze::analyze(&module, &Limits::default()).unwrap();
}

#[test]
fn call_before_return_uses_terminal_capture_sources_and_releases_the_old_target() {
    let source =
        include_str!("../../../tests/fixtures/returned-owning-holders/call-before-return.zil");
    let module = typed_source(source);
    for function in module.functions.iter().filter(|f| {
        matches!(
            f.callable_return_contract.as_ref().map(|c| c.source),
            Some(zil_ir::CallableReturnSource::Parameter(_))
        )
    }) {
        let c = function.callable_return_contract.as_ref().unwrap();
        let holder = c
            .capture_sources
            .as_ref()
            .unwrap()
            .iter()
            .find(|row| row.position.base == zil_ir::StorageBase::Capture(0))
            .unwrap();
        assert_eq!(
            holder.sources,
            vec![zil_ir::StorageSource::Exact(zil_ir::StoragePosition {
                base: zil_ir::StorageBase::Parameter(0),
                path: vec![zil_ir::ContractProjection::Capture(1)]
            })],
            "{}",
            function.name
        );
    }
    zil_analyze::analyze(&module, &Limits::default()).unwrap();
    for index in module
        .functions
        .iter()
        .enumerate()
        .filter_map(|(index, f)| {
            matches!(
                f.callable_return_contract.as_ref().map(|c| c.source),
                Some(zil_ir::CallableReturnSource::Parameter(_))
            )
            .then_some(index)
        })
    {
        let mut dishonest = module.clone();
        let c = dishonest.functions[index]
            .callable_return_contract
            .as_mut()
            .unwrap();
        let holder = c
            .capture_sources
            .as_mut()
            .unwrap()
            .iter_mut()
            .find(|row| row.position.base == zil_ir::StorageBase::Capture(0))
            .unwrap();
        exact_source_mut(&mut holder.sources[0]).path = vec![
            zil_ir::ContractProjection::Capture(0),
            zil_ir::ContractProjection::Place(zil_ir::PlaceProjection::Field(1)),
        ];
        let errors = zil_analyze::analyze(&dishonest, &Limits::default()).unwrap_err();
        assert!(
            errors
                .iter()
                .any(|error| error.message.contains("terminal capture source exceeds")),
            "{errors:?}"
        );
    }
    let bad = source.replace("a=21;let answer=f()", "b=23;let answer=f()");
    assert!(zil_analyze::analyze(&typed_source(&bad), &Limits::default()).is_err());
}

#[test]
fn external_mutable_callback_publishes_current_environment_through_helpers() {
    let source = include_str!(
        "../../../tests/fixtures/returned-owning-holders/external-mutable-callback.zil"
    );
    let module = typed_source(source);
    zil_analyze::analyze(&module, &Limits::default()).unwrap();
    let reused = source
        .replace(
            "let first=relay(&mut f)",
            "let receiver=&mut f;let first=relay(receiver)",
        )
        .replace(
            "relay(&mut f)+relay(&mut g)",
            "relay(receiver)+relay(&mut g)",
        );
    zil_analyze::analyze(&typed_source(&reused), &Limits::default()).unwrap();
    for bad in [
        source.replace("a=21", "b=21"),
        source.replace("c=31", "d=31"),
        source.replace("a=21", "c=21"),
        reused.replace("a=21", "b=21"),
    ] {
        assert!(zil_analyze::analyze(&typed_source(&bad), &Limits::default()).is_err());
    }
}

#[test]
fn mutable_callback_slot_updates_compose_with_ordinary_holder_helpers() {
    let source = include_str!(
        "../../../tests/fixtures/returned-owning-holders/external-mutable-holder-helper.zil"
    );
    zil_analyze::analyze(&typed_source(&source), &Limits::default()).unwrap();
}

#[test]
fn called_before_return_via_external_mutable_helper_uses_terminal_sources() {
    let source =
        include_str!("../../../tests/fixtures/returned-owning-holders/call-before-return.zil");
    let source = source
        .replace(
            "fn step<",
            "fn invoke<F:FnMut()->i32>(input:&mut F)->i32{return input()}\nfn step<",
        )
        .replace(
            "current();return current",
            "invoke(&mut current);return current",
        );
    zil_analyze::analyze(&typed_source(&source), &Limits::default()).unwrap();
}

#[test]
fn mutable_helper_source_and_environment_proofs_have_exact_request_budgets() {
    for (name, source) in [
        (
            "forward-qualified",
            include_str!("../../../tests/fixtures/returned-owning-holders/forward-qualified.zil"),
        ),
        (
            "call-before-return",
            include_str!("../../../tests/fixtures/returned-owning-holders/call-before-return.zil"),
        ),
        (
            "external-holder-helper",
            include_str!(
                "../../../tests/fixtures/returned-owning-holders/external-mutable-holder-helper.zil"
            ),
        ),
        (
            "nested-qualified",
            include_str!(
                "../../../tests/fixtures/returned-owning-holders/nested-qualified-callback.zil"
            ),
        ),
    ] {
        let limits = |n| Limits {
            max_analysis_steps: n,
            ..Limits::default()
        };
        let module = typed_source(source);
        let mut low = 1;
        let mut high = Limits::default().max_analysis_steps;
        while low < high {
            let mid = low + (high - low) / 2;
            if typed_source_with_limit(source, &limits(mid)).is_ok() {
                high = mid
            } else {
                low = mid + 1
            }
        }
        assert_eq!(
            typed_source_with_limit(source, &limits(low)).unwrap(),
            module
        );
        let errors = typed_source_with_limit(source, &limits(low - 1)).unwrap_err();
        assert!(
            errors
                .iter()
                .any(|error| error.category == zil_base::ResultCategory::ResourceLimit),
            "{name}: {errors:?}"
        );
        assert_eq!(
            typed_source_with_limit(source, &limits(low)).unwrap(),
            module
        );
        eprintln!("{name} Types exact N={low}, N-1 rejected, retry identical");
        let plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
        low = 1;
        high = Limits::default().max_analysis_steps;
        while low < high {
            let mid = low + (high - low) / 2;
            if zil_analyze::analyze(&module, &limits(mid)).is_ok() {
                high = mid
            } else {
                low = mid + 1
            }
        }
        let mut session = zil_analyze::AnalysisSession::default();
        for warm in [false, true] {
            assert_eq!(session.analyze(&module, &limits(low)).unwrap(), plan);
            if warm {
                assert!(session.last_stats().reused_functions > 0);
            }
            let errors = session.analyze(&module, &limits(low - 1)).unwrap_err();
            assert!(
                errors
                    .iter()
                    .any(|error| error.category == zil_base::ResultCategory::ResourceLimit),
                "{name}: {errors:?}"
            );
            assert_eq!(zil_analyze::analyze(&module, &limits(low)).unwrap(), plan);
        }
        eprintln!("{name} Analyze exact N={low}, N-1 rejected, fresh/warm identical");
    }
}

#[test]
fn nested_qualified_factory_preserves_child_classes_sources_and_cleanup() {
    let source = include_str!(
        "../../../tests/fixtures/returned-owning-holders/nested-qualified-callback.zil"
    );
    let module = typed_source(source);
    zil_analyze::analyze(&module, &Limits::default()).unwrap();
    let factory = module
        .functions
        .iter()
        .position(|f| {
            f.callable_return_contract.as_ref().is_some_and(|c| {
                module.closures[c.closure.0 as usize]
                    .captures
                    .iter()
                    .any(|ty| matches!(ty, zil_ir::Type::Closure(_)))
            })
        })
        .unwrap();
    let c = module.functions[factory]
        .callable_return_contract
        .as_ref()
        .unwrap();
    assert_eq!(c.cleanup, [0]);
    assert!(
        module.closures[c.closure.0 as usize]
            .storage_life
            .as_ref()
            .is_some_and(|life| life.fixed_binder_count > 0)
    );
    for attack in 0..2 {
        let mut dishonest = module.clone();
        let c = dishonest.functions[factory]
            .callable_return_contract
            .as_mut()
            .unwrap();
        if attack == 0 {
            c.cleanup.clear();
        } else {
            let row = c
                .capture_sources
                .as_mut()
                .unwrap()
                .iter_mut()
                .find(|row| row.position.path.len() > 1)
                .unwrap();
            exact_source_mut(&mut row.sources[0]).path =
                vec![zil_ir::ContractProjection::Capture(1)];
        }
        assert!(
            zil_analyze::analyze(&dishonest, &Limits::default()).is_err(),
            "attack {attack}"
        );
    }
    let double = source.replace(
        "var f=nested(make(wrap(&a),&b))",
        "var f=nested(nested(make(wrap(&a),&b)))",
    );
    zil_analyze::analyze(&typed_source(&double), &Limits::default()).unwrap();
    let bad = source.replace("a=21", "b=21");
    assert!(zil_analyze::analyze(&typed_source(&bad), &Limits::default()).is_err());
}

#[test]
fn inline_nested_qualified_factory_transfers_actual_child_classes() {
    let source = include_str!(
        "../../../tests/fixtures/returned-owning-holders/inline-nested-qualified-callback.zil"
    );
    let module = typed_source(source);
    assert!(module.closures.iter().any(|d| {
        d.captures
            .iter()
            .any(|ty| matches!(ty, zil_ir::Type::Closure(_)))
    }));
    for d in &module.closures {
        if d.captures
            .iter()
            .any(|ty| matches!(ty, zil_ir::Type::BorrowedStorage { .. }))
        {
            assert!(d.storage_life.is_some());
        }
    }
    zil_analyze::analyze(&module, &Limits::default()).unwrap();
    let mut erased = module.clone();
    for d in &mut erased.closures {
        d.storage_life = None;
    }
    for function in &mut erased.functions {
        if function.environment.is_some() {
            function.storage_life = None;
        }
        if let Some(life) = &mut function.storage_life {
            life.classes
                .retain(|row| row.position.base != zil_ir::StorageBase::Result);
        }
    }
    assert!(zil_analyze::analyze(&erased, &Limits::default()).is_err());
    let bad = include_str!(
        "../../../tests/fixtures/returned-owning-holders/inline-nested-distinct-formal-classes.zil"
    );
    assert!(zil_analyze::analyze(&typed_source(bad), &Limits::default()).is_err());
}

#[test]
fn captured_mutable_holder_publishes_the_original_external_slot() {
    let source =
        include_str!("../../../tests/fixtures/returned-owning-holders/captured-mutable-holder.zil");
    let module = typed_source(source);
    zil_analyze::analyze(&module, &Limits::default()).unwrap();
    let helpers = "fn invoke<F:FnMut()->i32>(f:&mut F)->i32{return f()} fn relay<F:FnMut()->i32>(f:&mut F)->i32{return invoke(f)}";
    let helper = source
        .replace("fn main", &format!("{helpers} fn main"))
        .replace("let first=f()", "let first=relay(&mut f)")
        .replace("let second=f()", "let second=relay(&mut f)");
    zil_analyze::analyze(&typed_source(&helper), &Limits::default()).unwrap();
    let head = source.split("fn main").next().unwrap();
    let body = "fn main()->i32{var a=20;var b=22;var c=30;var d=32;var holder=hold(&a);var another=hold(&c);var f=make(&mut holder,&b);var g=make(&mut another,&d);let first=relay(&mut f);a=21;let third=relay(&mut g);c=31;let second=relay(&mut f);let fourth=relay(&mut g);let x=*holder.pointer;let y=*another.pointer;b=23;d=33;return first+second+third+fourth+x+y+a+b+c+d-266}";
    zil_analyze::analyze(
        &typed_source(&format!("{head}{helpers}{body}")),
        &Limits::default(),
    )
    .unwrap();
    for body in [
        "fn main()->i32{var a=20;var b=22;var holder=hold(&a);let saved=holder.pointer;var f=make(&mut holder,&b);let first=f();a=21;return first+*saved}",
        "fn main()->i32{var a=20;var b=22;var holder=hold(&a);var f=make(&mut holder,&b);let first=f();b=23;return first+f()}",
    ] {
        assert!(
            zil_analyze::analyze(&typed_source(&format!("{head}{body}")), &Limits::default())
                .is_err()
        );
    }
}

#[test]
fn exclusive_capture_readonly_fn_is_repeatable_but_its_environment_is_affine() {
    let source = include_str!(
        "../../../tests/fixtures/returned-owning-holders/exclusive-capture-readonly-fn.zil"
    );
    let module = typed_source(source);
    assert_eq!(module.closures[0].kind, zil_ir::CallableKind::Fn);
    zil_analyze::analyze(&module, &Limits::default()).unwrap();
    let bad = source.replace("let a=f();let b=f()", "let g=f;let a=f();let b=g()");
    let result = typed_source_with_limit(&bad, &Limits::default())
        .and_then(|m| zil_analyze::analyze(&m, &Limits::default()));
    assert!(
        result.is_err(),
        "an affine environment cannot be called after moving it"
    );
    let mut forged = module;
    let entry = &mut forged.functions[forged.closures[0].function.0 as usize];
    let output = entry
        .blocks
        .iter()
        .flat_map(|b| &b.instructions)
        .find_map(|i| {
            matches!(i.kind, zil_ir::InstructionKind::ReadCapture { .. })
                .then_some(i.result)
                .flatten()
        })
        .unwrap();
    let zil_ir::Type::Address { mutable, .. } = &mut entry.values[output.0 as usize] else {
        panic!()
    };
    *mutable = true;
    assert!(
        zil_analyze::analyze(&forged, &Limits::default()).is_err(),
        "Fn cannot expose its exclusive captured capability"
    );
}

#[test]
fn captured_mutable_holder_keeps_pointee_classes_and_call_scoped_leases() {
    for source in [
        include_str!(
            "../../../tests/fixtures/returned-owning-holders/captured-holder-distinct-pointee-classes.zil"
        ),
        include_str!(
            "../../../tests/fixtures/returned-owning-holders/fnmut-exclusive-capture-escape.zil"
        ),
    ] {
        let result = typed_source_with_limit(source, &Limits::default())
            .and_then(|module| zil_analyze::analyze(&module, &Limits::default()));
        let errors =
            result.expect_err("captured invocation cannot change a pointee class or escape");
        assert!(
            errors
                .iter()
                .any(|e| e.category != zil_base::ResultCategory::Unsupported),
            "{errors:?}"
        );
    }
}

#[test]
fn captured_mutable_holder_call_before_return_updates_external_memory() {
    let source = include_str!(
        "../../../tests/fixtures/returned-owning-holders/captured-mutable-holder-call-before-return.zil"
    );
    zil_analyze::analyze(&typed_source(source), &Limits::default()).unwrap();
}

#[test]
fn captured_mutable_holder_branch_targets_preserve_unselected_dependencies() {
    let source = include_str!(
        "../../../tests/fixtures/returned-owning-holders/captured-mutable-holder-branch-heads.zil"
    );
    zil_analyze::analyze(&typed_source(source), &Limits::default()).unwrap();
    let bad = source.replace("f();let x", "f();a=21;let x");
    assert!(zil_analyze::analyze(&typed_source(&bad), &Limits::default()).is_err());
}

fn exact_source_mut(source: &mut zil_ir::StorageSource) -> &mut zil_ir::StoragePosition {
    match source {
        zil_ir::StorageSource::Exact(position) => position,
        _ => panic!("fixture requires an exact source"),
    }
}
