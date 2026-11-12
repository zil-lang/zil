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
    zil_types::check(&core, &target, &limits).unwrap()
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
                    c.capture_sources.as_mut().unwrap()[0].sources[0].base =
                        StorageBase::Parameter(1)
                }
                2 => c.capture_sources = Some(vec![]),
                3 => c.capture_sources = None,
                4 => {
                    c.capture_sources.as_mut().unwrap()[0].sources[0].base =
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
            errors.iter().any(|d| d.code == "E-BORROW-CONFLICT"),
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
