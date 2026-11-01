//! Author checks for the public source-only Product identity contract.
use zil_base::{FileId, Limits, ModuleId, Phase, ResultCategory, SourceUnit, TargetProfile};
use zil_ir::*;
const ORIGINAL: &str = include_str!("impl237_fixtures/original235.zil");
fn core(text: &str) -> CoreModule {
    let u = SourceUnit::new(FileId(0), ModuleId(0), "types237.zil", text);
    let ast = zil_syntax::parse(&u).into_result().unwrap();
    zil_expand::expand(&u, &ast).unwrap()
}
fn accepted(text: &str) -> TypedModule {
    zil_types::check(&core(text), &TargetProfile::host_llvm(), &Limits::default()).unwrap()
}
fn reject(c: &CoreModule) {
    let target = TargetProfile::host_llvm();
    let limits = Limits::default();
    for r in [
        zil_types::check(c, &target, &limits),
        zil_types::check_program(
            &CoreProgram {
                root: c.module_id,
                source_fingerprint: c.source_fingerprint,
                modules: vec![ProgramModule {
                    core: c.clone(),
                    imports: vec![],
                    exports: vec![],
                }],
            },
            &target,
            &limits,
        )
        .map(|p| p.module),
    ] {
        let errors =
            r.expect_err("malformed public Product metadata must fail independently in Types");
        assert!(
            errors.iter().all(|d| d.phase == Phase::Types
                && matches!(
                    d.category,
                    ResultCategory::SafeReject | ResultCategory::Unsupported
                )),
            "{errors:#?}"
        );
    }
}
#[test]
fn original235_generic_wrapper_reaches_types() {
    accepted(ORIGINAL);
}
#[test]
fn direct_and_retained_application_have_one_complete_nominal_identity() {
    accepted(&ORIGINAL.replace(
        "let root=wrap(give items)",
        "let root:Root<Item<F>>=wrap(give items)",
    ));
}
#[test]
fn unused_owner_and_recipe_mutations_are_rejected() {
    let input = core(ORIGINAL);
    assert!(!input.aggregate_templates.is_empty());
    assert!(!input.aggregate_applications.is_empty());
    for kind in [
        "owner",
        "owner-slot",
        "family",
        "header-field",
        "recipe-field",
        "duplicate-application",
        "duplicate-family",
    ] {
        let mut c = input.clone();
        let a = c
            .aggregate_applications
            .iter()
            .position(|a| {
                a.arguments
                    .iter()
                    .any(|a| matches!(a, CoreAggregateArgument::Parameter { .. }))
            })
            .unwrap();
        match kind {
            "owner" => {
                c.aggregate_applications[a].owner.as_mut().unwrap().function =
                    zil_base::NodeId(u32::MAX)
            }
            "owner-slot" => {
                c.aggregate_applications[a].arguments[0] =
                    CoreAggregateArgument::Parameter { index: 1 }
            }
            "family" => c.aggregate_applications[a].template = zil_base::NodeId(u32::MAX),
            "header-field" => {
                let id = c.aggregate_applications[a].aggregate;
                c.aggregates.iter_mut().find(|d| d.id == id).unwrap().fields[0].public ^= true;
            }
            "recipe-field" => c.aggregate_templates[0].fields[0].name.push('x'),
            "duplicate-application" => c
                .aggregate_applications
                .push(c.aggregate_applications[a].clone()),
            "duplicate-family" => c.aggregate_templates.push(c.aggregate_templates[0].clone()),
            _ => unreachable!(),
        }
        reject(&c);
    }
}

const PHANTOM: &str = r#"
resource R{n:i32}
package A{manages r:R}
package B{manages r:R}
package Phantom<T:StoreElement>{manages bytes:ByteBuffer}
fn phantom<T:StoreElement>(give items:Store<T>)->Phantom<T>{return Phantom::<T>{bytes:bytes_new()}}
fn main(){let a:Store<A>=store_new();let p=phantom(give a);let b:Store<B>=store_new();let q=phantom(give b)}
"#;
#[test]
fn phantom_arguments_are_complete_nominal_identity_even_without_fields() {
    let m = accepted(PHANTOM);
    let results = m
        .functions
        .iter()
        .filter_map(|f| {
            if let Type::Package { name, fields } = &f.return_type {
                Some((name, fields))
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].1, results[1].1);
    assert_ne!(results[0].0, results[1].0);
}
#[test]
fn phantom_actual_bound_is_checked_from_raw_metadata() {
    let mut input = core(PHANTOM);
    for a in &mut input.aggregate_applications {
        a.owner = None;
        a.arguments = vec![CoreAggregateArgument::Concrete(Type::I32)];
    }
    reject(&input);
}
#[test]
fn unused_recipe_cannot_hide_unknown_types_or_recursion() {
    let control = core(PHANTOM);
    let mut unused = control.aggregate_templates[0].clone();
    unused.id = zil_base::NodeId(1000000);
    unused.name = "unused-family".into();
    for recursive in [false, true] {
        let mut c = control.clone();
        let mut t = unused.clone();
        t.fields[0].ty = if recursive {
            CoreAggregateTypeRecipe::Apply {
                module: c.module_id,
                template: t.id,
                arguments: vec![CoreAggregateTypeRecipe::Parameter { index: 0 }],
            }
        } else {
            CoreAggregateTypeRecipe::Concrete(Type::Named("not_a_type".into()))
        };
        c.aggregate_templates.push(t);
        reject(&c);
    }
}
#[test]
fn preserved_source_executes_at_both_optimization_levels() {
    use std::{fs, process::Command};
    use zil_driver::{CompileOptions, NativeBackend, Optimization, Stage, compile_source};
    let backend = if cfg!(feature = "llvm-api") {
        NativeBackend::LlvmApi
    } else {
        NativeBackend::Clang
    };
    for optimization in [Optimization::O0, Optimization::O2] {
        let d = std::env::var_os("ZIL_TEST_REPORT_DIR")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(std::env::temp_dir)
            .join(format!(
                "author237-original-{optimization:?}-{}",
                std::process::id()
            ));
        fs::create_dir_all(&d).unwrap();
        let executable = d.join("program");
        let r = compile_source(
            &SourceUnit::new(FileId(0), ModuleId(0), "original235.zil", ORIGINAL),
            &CompileOptions {
                backend,
                stop_after: Stage::Build,
                optimization,
                output: Some(executable.clone()),
                emit_dir: Some(d.join("phases")),
                ..CompileOptions::default()
            },
        );
        fs::write(d.join("compile.json"), r.to_json()).unwrap();
        assert_eq!(r.status, ResultCategory::Success, "{}", r.to_json());
        let summary = r.native_codegen.unwrap();
        assert_eq!(summary.backend, backend);
        assert!(summary.fallback.is_none());
        let o = Command::new(executable).output().unwrap();
        fs::write(d.join("stdout"), &o.stdout).unwrap();
        fs::write(d.join("stderr"), &o.stderr).unwrap();
        assert_eq!(o.status.code(), Some(0));
        assert_eq!(o.stdout, b"AZ");
        assert!(o.stderr.is_empty());
    }
}

#[test]
fn product_input_infers_complete_element_type_through_ordinary_value_relay() {
    let source = ORIGINAL.replace(
        "let root=wrap(give items)",
        "let root=relay(wrap(give items))",
    ) + "\nfn relay<T:StoreElement>(root:Root<T>)->Root<T>{return root}\n";
    accepted(&source);
    let phantom = PHANTOM
        .replace("let p=phantom(give a)", "let p=relay(phantom(give a))")
        .replace("let q=phantom(give b)", "let q=relay(phantom(give b))")
        + "\nfn relay<T:StoreElement>(root:Phantom<T>)->Phantom<T>{return root}\n";
    let m = accepted(&phantom);
    let relay = m
        .functions
        .iter()
        .filter(|f| {
            matches!(&f.return_type, Type::Package { .. })
                && f.parameters.len() == 1
                && matches!(
                    f.locals[f.parameters[0].0 as usize].ty,
                    Type::Package { .. }
                )
        })
        .collect::<Vec<_>>();
    assert_eq!(relay.len(), 2);
    assert_ne!(relay[0].return_type, relay[1].return_type);
}

const NESTED_SCOPE: &str = r#"
resource Ticket{code:i32}
package Phantom<T:StoreElement>{manages bytes:ByteBuffer}
package Carrier<T:StoreElement>{manages items:Store<T>}
package Item<F>{manages ticket:Ticket;callback:F}
package Packet<F>{manages marker:Ticket;callback:F}
fn nested<T:StoreElement>(root:Carrier<Phantom<T>>)->Carrier<Phantom<T>>{return root}
fn make()->impl Fn()->i32{return move()=>8}
fn pack<F:Fn()->i32>(callback:F)->Packet<F>{return Packet::<F>{marker:Ticket{code:7},callback:callback}}
fn run<F:Fn()->i32>(packet:Packet<F>)->i32{let items:Store<Phantom<Item<F>>>=store_new();let carrier=Carrier::<Phantom<Item<F>>>{items:give items};let moved=nested(carrier);return i32(store_len(moved.items))}
fn main()->i32{return run(pack(make()))}
"#;
#[test]
fn a_free_binder_hidden_in_phantom_arguments_requires_the_actual_outer_owner() {
    let input = core(NESTED_SCOPE);
    accepted(NESTED_SCOPE);
    let family = input
        .aggregate_templates
        .iter()
        .find(|t| t.name == "Carrier")
        .unwrap()
        .id;
    let run = input.functions.iter().find(|f| f.name == "run").unwrap().id;
    let app = input
        .aggregate_applications
        .iter()
        .position(|a| a.template == family && a.owner.is_some_and(|o| o.function == run))
        .unwrap();
    let mut altered = input.clone();
    altered.aggregate_applications[app].owner = None;
    reject(&altered);
}

#[test]
fn prepared_and_function_session_recheck_unused_metadata_after_a_scalar_success() {
    let c = core("fn main()->i32{return 7}");
    let good = CoreProgram {
        root: c.module_id,
        source_fingerprint: c.source_fingerprint,
        modules: vec![ProgramModule {
            core: c,
            imports: vec![],
            exports: vec![],
        }],
    };
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    let prepared = zil_types::infer_program_for_ct(&good, &limits)
        .unwrap()
        .into_prepared();
    let mut session = zil_types::FunctionSession::default();
    session
        .check_prepared_program(&prepared, &target, &limits)
        .unwrap();
    assert!(!session.is_empty());
    let mut bad = good.clone();
    let span = bad.modules[0].core.functions[0].span;
    bad.modules[0]
        .core
        .aggregate_templates
        .push(CoreAggregateTemplate {
            id: zil_base::NodeId(999),
            name: "Unused".into(),
            parameter: CoreTypeParameter {
                index: 0,
                name: "T".into(),
                bound: CoreTypeBound::StoreElement,
            },
            fields: vec![CoreAggregateFieldRecipe {
                name: "unbound".into(),
                public: false,
                manages: false,
                ty: CoreAggregateTypeRecipe::Concrete(Type::Named("not_a_type".into())),
                span,
            }],
            span,
        });
    let prepared = zil_types::infer_program_for_ct(&good, &limits)
        .unwrap()
        .after_evaluation(bad);
    assert!(zil_types::check_prepared_program(&prepared, &target, &limits).is_err());
    assert!(
        session
            .check_prepared_program(&prepared, &target, &limits)
            .is_err()
    );
    assert_eq!(session.last_stats().reused_functions, 0);
}
