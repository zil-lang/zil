//! Independent Core metadata attacks and preserved source obligations for 237.
use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_base::{
    Diagnostic, FileId, Fingerprint, Limits, ModuleId, NodeId, Phase, ResultCategory, SourceUnit,
    TargetProfile,
};
use zil_driver::{
    CompileOptions, NativeBackend, Optimization, ProjectSources, Stage, compile_project,
    compile_source,
};
use zil_ir::*;

const ORIGINAL: &str = include_str!("review237_fixtures/original235.zil");
const SIMPLE: &str = r#"
resource Ticket{code:i32}
package Item{manages ticket:Ticket;value:i32}
package Bag<T:StoreElement>{manages items:Store<T>}
package Root<T:StoreElement>{child:Bag<T>;tag:i32}
fn wrap<T:StoreElement>(give items:Store<T>)->Root<T>{return Root::<T>{child:Bag::<T>{items:give items},tag:7}}
fn again<T:StoreElement>(give items:Store<T>)->Root<T>{return wrap(give items)}
fn main()->i32{let first:Store<Item>=store_new();let second:Store<Item>=store_new();let a=wrap(give first);let b=again(give second);return a.tag+b.tag-14}
"#;
static NEXT: AtomicUsize = AtomicUsize::new(0);
fn dir(label: &str) -> PathBuf {
    let root = std::env::var_os("ZIL_TEST_REPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let out = root.join(format!(
        "review237-{label}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&out).unwrap();
    out
}
fn unit(text: &str) -> SourceUnit {
    SourceUnit::new(FileId(0), ModuleId(0), "review237.zil", text)
}
fn core(text: &str) -> CoreModule {
    let u = unit(text);
    let parsed = zil_syntax::parse(&u).into_result().unwrap();
    zil_expand::expand(&u, &parsed).unwrap()
}
fn program(input: &CoreModule) -> CoreProgram {
    CoreProgram {
        root: input.module_id,
        source_fingerprint: input.source_fingerprint,
        modules: vec![ProgramModule {
            core: input.clone(),
            imports: vec![],
            exports: vec![],
        }],
    }
}
fn rejected(result: Result<impl Sized, Vec<Diagnostic>>, label: &str) {
    let errors = match result {
        Ok(_) => panic!("{label}: accepted malformed metadata"),
        Err(e) => e,
    };
    assert!(
        errors.iter().any(|d| d.phase == Phase::Types
            && matches!(
                d.category,
                ResultCategory::SafeReject | ResultCategory::Unsupported
            )),
        "{label}: {errors:#?}"
    );
}
fn all_type_entries_reject(label: &str, input: &CoreModule) {
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    let p = program(input);
    let out = dir(label);
    fs::write(out.join("core.txt"), format!("{input:#?}")).unwrap();
    let inferred = zil_types::infer_source_closures(input, &limits);
    let single = zil_types::check(input, &target, &limits);
    let shapes = zil_types::resolve_program_shapes(&p, &limits);
    let checked = zil_types::check_program(&p, &target, &limits);
    for (name, text) in [
        ("infer", format!("{inferred:#?}")),
        ("single", format!("{single:#?}")),
        ("shapes", format!("{shapes:#?}")),
        ("program", format!("{checked:#?}")),
    ] {
        fs::write(out.join(format!("{name}.txt")), text).unwrap();
    }
    // This public preparation API returns untrusted Core. It may defer a
    // semantic rejection, but cannot erase the attacker-controlled tables.
    match inferred {
        Ok(prepared) => {
            assert_eq!(prepared.aggregate_templates, input.aggregate_templates);
            assert_eq!(
                prepared.aggregate_applications.len(),
                input.aggregate_applications.len()
            );
            for (after, before) in prepared
                .aggregate_applications
                .iter()
                .zip(&input.aggregate_applications)
            {
                assert_eq!(after.aggregate, before.aggregate);
                assert_eq!(after.template_module, before.template_module);
                assert_eq!(after.template, before.template);
                assert_eq!(after.owner, before.owner);
                assert_eq!(after.arguments.len(), before.arguments.len());
                // This preparation may resolve Concrete Named types. It must
                // preserve the actual slots and the exact lexical owner.
                for (a, b) in after.arguments.iter().zip(&before.arguments) {
                    assert!(
                        matches!(
                            (a, b),
                            (
                                CoreAggregateArgument::Concrete(_),
                                CoreAggregateArgument::Concrete(_)
                            )
                        ) || a == b
                    );
                }
            }
            rejected(
                zil_types::check(&prepared, &target, &limits),
                &format!("{label}/prepared-single"),
            );
        }
        Err(errors) => rejected(Err::<CoreModule, _>(errors), &format!("{label}/infer")),
    }
    rejected(single, &format!("{label}/single"));
    rejected(shapes, &format!("{label}/shapes"));
    rejected(checked, &format!("{label}/program"));
}
fn native(label: &str, text: &str, expected: &str) {
    for optimization in [Optimization::O0, Optimization::O2] {
        let out = dir(label);
        fs::write(out.join("source.zil"), text).unwrap();
        let backend = if cfg!(feature = "llvm-api") {
            NativeBackend::LlvmApi
        } else {
            NativeBackend::Clang
        };
        let r = compile_source(
            &unit(text),
            &CompileOptions {
                stop_after: Stage::Build,
                backend,
                optimization,
                output: Some(out.join("program")),
                emit_dir: Some(out.join("phases")),
                ..CompileOptions::default()
            },
        );
        fs::write(out.join("report.json"), r.to_json()).unwrap();
        assert_eq!(
            r.status,
            ResultCategory::Success,
            "{label}: {}",
            r.to_json()
        );
        let summary = r.native_codegen.as_ref().unwrap();
        assert_eq!(summary.backend, backend);
        assert!(summary.fallback.is_none());
        let result = Command::new(&r.artifacts["executable"]).output().unwrap();
        fs::write(out.join("stdout"), &result.stdout).unwrap();
        fs::write(out.join("stderr"), &result.stderr).unwrap();
        assert_eq!(result.status.code(), Some(0), "{label}: {result:?}");
        assert_eq!(result.stdout, expected.as_bytes(), "{label}: {result:?}");
        assert!(result.stderr.is_empty());
    }
}

#[test]
fn original235_success_obligation_is_preserved_byte_for_byte_and_executes() {
    native("original235", ORIGINAL, "AZ")
}

fn retained_source() -> String {
    ORIGINAL.replace("let root=wrap(give items)", "let root=again(give items)")
        + "\nfn again<T:StoreElement>(give items:Store<T>)->Root<T>{return wrap(give items)}\n"
}

#[test]
fn direct_and_retained_applications_share_family_identity_across_function_owners() {
    let text = retained_source();
    native("two-owners", &text, "AZ");
    let input = core(&text);
    assert!(!input.aggregate_templates.is_empty());
    assert!(!input.aggregate_applications.is_empty());
    let t = TargetProfile::host_llvm();
    let l = Limits::default();
    zil_types::check(&input, &t, &l).unwrap();
    zil_types::check_program(&program(&input), &t, &l).unwrap();
}

#[test]
fn unused_template_and_application_tables_cannot_override_the_generated_header() {
    let original = core(&retained_source());
    assert!(!original.aggregate_templates.is_empty());
    assert!(!original.aggregate_applications.is_empty());
    for mode in [
        "duplicate-template",
        "empty-family-name",
        "wrong-formal-index",
        "empty-formal-name",
        "duplicate-application",
        "missing-family",
        "foreign-family-module",
        "missing-header",
        "header-field-name",
        "header-field-permission",
        "header-field-ownership",
        "header-field-type",
        "missing-argument",
        "extra-argument",
    ] {
        let mut input = original.clone();
        match mode {
            "duplicate-template" => input
                .aggregate_templates
                .push(input.aggregate_templates[0].clone()),
            "empty-family-name" => input.aggregate_templates[0].name.clear(),
            "wrong-formal-index" => input.aggregate_templates[0].parameter.index = 1,
            "empty-formal-name" => input.aggregate_templates[0].parameter.name.clear(),
            "duplicate-application" => input
                .aggregate_applications
                .push(input.aggregate_applications[0].clone()),
            "missing-family" => input.aggregate_applications[0].template = NodeId(u32::MAX),
            "foreign-family-module" => {
                input.aggregate_applications[0].template_module = ModuleId(73)
            }
            "missing-header" => input.aggregate_applications[0].aggregate = NodeId(u32::MAX),
            "missing-argument" => input.aggregate_applications[0].arguments.clear(),
            "extra-argument" => input.aggregate_applications[0]
                .arguments
                .push(CoreAggregateArgument::Concrete(Type::I32)),
            _ => {
                let id = input.aggregate_applications[0].aggregate;
                let header = input.aggregates.iter_mut().find(|a| a.id == id).unwrap();
                match mode {
                    "header-field-name" => header.fields[0].name.push_str("_forged"),
                    "header-field-permission" => header.fields[0].public = !header.fields[0].public,
                    "header-field-ownership" => {
                        header.fields[0].manages = !header.fields[0].manages
                    }
                    "header-field-type" => header.fields[0].ty = Type::I64,
                    _ => unreachable!(),
                }
            }
        }
        all_type_entries_reject(mode, &input);
    }
}

#[test]
fn a_same_spelled_parameter_in_another_function_cannot_supply_application_authority() {
    let original = core(&retained_source());
    let index = original
        .aggregate_applications
        .iter()
        .position(|a| {
            a.owner.is_some()
                && a.arguments
                    .iter()
                    .any(|a| matches!(a, CoreAggregateArgument::Parameter { .. }))
        })
        .expect("retained ordinary owner application");
    let owning = original.aggregate_applications[index].owner.unwrap();
    let other = original
        .functions
        .iter()
        .find(|f| f.id != owning.function && !f.type_parameters.is_empty())
        .expect("another T owner")
        .id;
    for mode in [
        "absent-owner",
        "wrong-module",
        "non-template-owner",
        "same-spelled-other-owner",
        "parameter-outside-owner",
    ] {
        let mut input = original.clone();
        let ordinary = input
            .functions
            .iter()
            .find(|f| f.name == "main")
            .unwrap()
            .id;
        let application = &mut input.aggregate_applications[index];
        match mode {
            "absent-owner" => application.owner = None,
            "wrong-module" => application.owner.as_mut().unwrap().module = ModuleId(61),
            "non-template-owner" => application.owner.as_mut().unwrap().function = ordinary,
            "same-spelled-other-owner" => application.owner.as_mut().unwrap().function = other,
            "parameter-outside-owner" => {
                application.arguments[0] = CoreAggregateArgument::Parameter { index: 1 }
            }
            _ => unreachable!(),
        }
        all_type_entries_reject(mode, &input);
    }
}

#[test]
fn concrete_arguments_with_free_f_cannot_borrow_another_functions_same_f_binder() {
    let original = core(ORIGINAL);
    let caller = original
        .functions
        .iter()
        .find(|f| f.name == "run")
        .expect("complete-State caller F owner")
        .id;
    let other = original
        .functions
        .iter()
        .find(|f| f.id != caller && f.callable_parameters.iter().any(|b| b.generic_name == "F"))
        .expect("different callable F owner")
        .id;
    let index = original
        .aggregate_applications
        .iter()
        .position(|a| {
            a.owner.is_some_and(|o| o.function == caller)
                && a.arguments
                    .iter()
                    .any(|a| matches!(a, CoreAggregateArgument::Concrete(_)))
        })
        .expect("concrete nominal argument containing caller F");
    for mode in ["unowned-free-F", "same-F-wrong-owner"] {
        let mut input = original.clone();
        input.aggregate_applications[index].owner = if mode == "unowned-free-F" {
            None
        } else {
            Some(CoreAggregateOwner {
                module: input.module_id,
                function: other,
            })
        };
        all_type_entries_reject(mode, &input);
    }
}

#[test]
fn runtime_ct_folding_preserves_both_tables_and_exact_owner_ids() {
    let input = core(&format!("{SIMPLE}fn folded()->i32{{return #(6*7)}}"));
    let target = TargetProfile::host_llvm();
    let limits = Limits::default();
    let p = program(&input);
    let context = zil_ct::SourceProgramContext::new(&p, &limits).unwrap();
    for output in [
        zil_ct::evaluate_source_constants(&input, &target, &limits).unwrap(),
        zil_ct::evaluate_source_constants_with_context(&input, &target, &limits, &context)
            .unwrap()
            .0,
    ] {
        assert_eq!(output.aggregate_templates, input.aggregate_templates);
        assert_eq!(output.aggregate_applications, input.aggregate_applications);
    }
}

#[test]
fn unused_recursive_recipe_is_rejected_without_relying_on_a_value_use() {
    let mut input = core("resource Ticket{code:i32}fn main(){}");
    let span = input.functions[0].span;
    let family = NodeId(9000);
    input.aggregate_templates.push(CoreAggregateTemplate {
        id: family,
        name: "UnusedCycle".into(),
        parameter: CoreTypeParameter {
            index: 0,
            name: "T".into(),
            bound: CoreTypeBound::StoreElement,
        },
        fields: vec![
            CoreAggregateFieldRecipe {
                name: "child".into(),
                public: false,
                manages: false,
                ty: CoreAggregateTypeRecipe::Apply {
                    module: input.module_id,
                    template: family,
                    arguments: vec![CoreAggregateTypeRecipe::Parameter { index: 0 }],
                },
                span,
            },
            CoreAggregateFieldRecipe {
                name: "marker".into(),
                public: false,
                manages: true,
                ty: CoreAggregateTypeRecipe::Concrete(Type::Named("Ticket".into())),
                span,
            },
        ],
        span,
    });
    all_type_entries_reject("unused-recursive-recipe", &input);
}

const LIBRARY: &str = r#"
pub package Bag<T:StoreElement>{manages items:Store<T>}
pub package Root<T:StoreElement>{pub child:Bag<T>}
pub fn wrap<T:StoreElement>(give items:Store<T>)->Root<T>{return Root::<T>{child:Bag::<T>{items:give items}}}
pub fn unwrap<T:StoreElement>(root:Root<T>)->Store<T> contract{result(give)}{let child=root.child;return give child.items}
"#;
fn imported_project() -> ProjectSources {
    let client=ORIGINAL.replace("package Bag<T:StoreElement>{manages items:Store<T>}\n", "")
        .replace("package Root<T:StoreElement>{child:Bag<T>}\n", "")
        .replace("fn wrap<T:StoreElement>(give items:Store<T>)->Root<T>{return Root::<T>{child:Bag::<T>{items:give items}}}\n", "")
        .replace("let root=wrap(give items);let child=root.child;", "let root=library.wrap(give items);let unwrapped=library.unwrap::<Item<F>>(root);")
        .replace("store_at_mut(child.items,0)","store_at_mut(unwrapped,0)");
    ProjectSources {
        root: ModuleId(0),
        modules: vec![
            SourceUnit::new(
                FileId(0),
                ModuleId(0),
                "main.zil",
                format!("import library;\n{client}"),
            ),
            SourceUnit::new(FileId(1), ModuleId(1), "library.zil", LIBRARY),
        ],
        paths: [(vec!["library".into()], ModuleId(1))].into(),
    }
}
fn project_core(p: &ProjectSources) -> CoreProgram {
    let asts = p
        .modules
        .iter()
        .map(|s| zil_syntax::parse(s).into_result().unwrap())
        .collect::<Vec<_>>();
    let imports = vec![
        vec![CoreImport {
            alias: "library".into(),
            module: ModuleId(1),
            span: asts[0].items[0].span,
        }],
        vec![],
    ];
    let expanded = zil_expand::expand_program_with_limits(
        &p.modules,
        &asts.iter().collect::<Vec<_>>(),
        &imports,
        &Limits::default(),
    )
    .unwrap();
    CoreProgram {
        root: p.root,
        source_fingerprint: Fingerprint::ZERO,
        modules: expanded.into_iter().map(|(m, _)| m).collect(),
    }
}

#[test]
fn imported_family_preserves_the_callers_lexical_callable_owner_through_ct() {
    let project = imported_project();
    let input = project_core(&project);
    let target = TargetProfile::host_llvm();
    let limits = Limits::default();
    zil_types::check_program(&input, &target, &limits).unwrap();
    let context = zil_ct::SourceProgramContext::new(&input, &limits).unwrap();
    let mut normalized = input.clone();
    for module in &mut normalized.modules {
        let (output, _) = zil_ct::evaluate_source_constants_with_context(
            &module.core,
            &target,
            &limits,
            &context,
        )
        .unwrap();
        assert_eq!(output.aggregate_templates, module.core.aggregate_templates);
        assert_eq!(
            output.aggregate_applications,
            module.core.aggregate_applications
        );
        module.core = output;
    }
    zil_types::check_program(&normalized, &target, &limits).unwrap();
    let mut forged = input.clone();
    let foreign_function = forged
        .modules
        .iter()
        .find(|m| m.core.module_id == ModuleId(1))
        .unwrap()
        .core
        .functions
        .iter()
        .find(|f| !f.type_parameters.is_empty())
        .unwrap()
        .id;
    let application = forged
        .modules
        .iter_mut()
        .flat_map(|m| &mut m.core.aggregate_applications)
        .find(|a| {
            a.template_module == ModuleId(1) && a.owner.is_some_and(|o| o.module == ModuleId(0))
        })
        .expect("imported recipe used by caller's F");
    application.owner = Some(CoreAggregateOwner {
        module: ModuleId(1),
        function: foreign_function,
    });
    rejected(
        zil_types::resolve_program_shapes(&forged, &limits),
        "foreign-owner/shapes",
    );
    rejected(
        zil_types::check_program(&forged, &target, &limits),
        "foreign-owner/program",
    );
    for optimization in [Optimization::O0, Optimization::O2] {
        let out = dir("cross-module");
        for source in &project.modules {
            fs::write(
                out.join(format!("module{}.zil", source.module_id.0)),
                &source.text,
            )
            .unwrap();
        }
        let backend = if cfg!(feature = "llvm-api") {
            NativeBackend::LlvmApi
        } else {
            NativeBackend::Clang
        };
        let report = compile_project(
            &project,
            &CompileOptions {
                stop_after: Stage::Build,
                backend,
                optimization,
                output: Some(out.join("program")),
                emit_dir: Some(out.join("phases")),
                ..CompileOptions::default()
            },
        );
        fs::write(out.join("report.json"), report.to_json()).unwrap();
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        let summary = report.native_codegen.as_ref().unwrap();
        assert_eq!(summary.backend, backend);
        assert!(summary.fallback.is_none());
        let result = Command::new(&report.artifacts["executable"])
            .output()
            .unwrap();
        fs::write(out.join("stdout"), &result.stdout).unwrap();
        fs::write(out.join("stderr"), &result.stderr).unwrap();
        assert_eq!(result.status.code(), Some(0), "{result:?}");
        assert_eq!(result.stdout, b"AZ");
        assert!(result.stderr.is_empty());
    }
}

fn phantom_metadata(name: &str) -> CoreModule {
    let mut input = core("resource Ticket{code:i32}fn main(){}");
    let span = input.functions[0].span;
    input.aggregate_templates.push(CoreAggregateTemplate {
        id: NodeId(9100),
        name: name.into(),
        parameter: CoreTypeParameter {
            index: 0,
            name: "T".into(),
            bound: CoreTypeBound::StoreElement,
        },
        fields: vec![CoreAggregateFieldRecipe {
            name: "marker".into(),
            public: false,
            manages: true,
            ty: CoreAggregateTypeRecipe::Concrete(Type::Named("Ticket".into())),
            span,
        }],
        span,
    });
    input
}

#[test]
fn unused_metadata_text_is_prepaid_in_types_and_ct_before_any_clone() {
    let target = TargetProfile::host_llvm();
    let limits = Limits {
        max_analysis_steps: 20_000,
        ..Limits::default()
    };
    let good = phantom_metadata("UnusedPhantom");
    zil_types::check(&good, &target, &limits).unwrap();
    zil_ct::evaluate_source_constants(&good, &target, &limits).unwrap();
    let huge = phantom_metadata(&"X".repeat(65_536));
    for (phase, result) in [
        (
            Phase::Types,
            zil_types::check(&huge, &target, &limits).map(|_| ()),
        ),
        (
            Phase::Ct,
            zil_ct::evaluate_source_constants(&huge, &target, &limits).map(|_| ()),
        ),
    ] {
        let errors = result.expect_err("metadata text must not be copied for free");
        assert!(
            errors
                .iter()
                .all(|d| d.phase == phase && d.category == ResultCategory::ResourceLimit),
            "{errors:#?}"
        );
    }
}

#[test]
fn an_uninstantiated_family_still_has_complete_recipe_and_ownership_checks() {
    for mode in [
        "free-Concrete-F",
        "recipe-parameter-slot",
        "unowned-address",
        "duplicate-field",
        "no-owned-leaf",
        "absent-Apply-family",
    ] {
        let mut input = phantom_metadata("UnusedInvalid");
        let template = &mut input.aggregate_templates[0];
        let field = &mut template.fields[0];
        match mode {
            "free-Concrete-F" => {
                field.ty = CoreAggregateTypeRecipe::Concrete(Type::Named("F".into()))
            }
            "recipe-parameter-slot" => field.ty = CoreAggregateTypeRecipe::Parameter { index: 1 },
            "unowned-address" => {
                field.manages = false;
                field.ty = CoreAggregateTypeRecipe::Concrete(Type::Address {
                    mutable: false,
                    inner: Box::new(Type::I32),
                });
            }
            "duplicate-field" => {
                let copy = field.clone();
                template.fields.push(copy);
            }
            "no-owned-leaf" => {
                field.manages = false;
                field.ty = CoreAggregateTypeRecipe::Concrete(Type::I32);
            }
            "absent-Apply-family" => {
                field.manages = false;
                field.ty = CoreAggregateTypeRecipe::Apply {
                    module: input.module_id,
                    template: NodeId(9292),
                    arguments: vec![CoreAggregateTypeRecipe::Parameter { index: 0 }],
                };
            }
            _ => unreachable!(),
        }
        all_type_entries_reject(mode, &input);
    }
}

#[test]
fn types_new_recipe_work_has_an_exact_fresh_limit_boundary() {
    let input = core(SIMPLE);
    types_work_boundary(&input, "types-budget");
}

fn types_work_boundary(input: &CoreModule, label: &str) {
    let target = TargetProfile::host_llvm();
    let default = Limits::default();
    let expected = zil_types::check(input, &target, &default).unwrap();
    let run = |steps| match zil_types::check(
        input,
        &target,
        &Limits {
            max_analysis_steps: steps,
            ..default.clone()
        },
    ) {
        Ok(actual) => {
            assert_eq!(actual, expected);
            true
        }
        Err(errors) => {
            assert!(
                errors
                    .iter()
                    .all(|d| d.category == ResultCategory::ResourceLimit),
                "{errors:#?}"
            );
            false
        }
    };
    let (mut low, mut high) = (0, default.max_analysis_steps);
    assert!(run(high));
    while high - low > 1 {
        let mid = low + (high - low) / 2;
        if run(mid) { high = mid } else { low = mid }
    }
    assert!(run(high));
    assert!(!run(high - 1));
    assert!(run(high));
    let out = dir(label);
    fs::write(out.join("boundary.json"),format!("{{\"types_N\":{high},\"N_pass\":true,\"N_minus_one_resource_limit\":true,\"fresh_N_pass\":true}}\n")).unwrap();
}

const PHANTOM: &str = r#"
resource Ticket{code:i32}
package ItemA{manages ticket:Ticket;value:i32}
package ItemB{manages ticket:Ticket;value:i32}
package Phantom<T:StoreElement>{manages marker:Ticket}
fn first<T:StoreElement>(give items:Store<T>)->Phantom<T>{return Phantom::<T>{marker:Ticket{code:7}}}
fn second<T:StoreElement>(give items:Store<T>)->Phantom<T>{return Phantom::<T>{marker:Ticket{code:7}}}
fn accept(value:Phantom<ItemA>)->i32{return value.marker.code}
"#;

#[test]
fn phantom_actuals_participate_in_identity_but_resolved_function_owners_do_not() {
    let good = format!(
        "{PHANTOM}package Item<F>{{manages ticket:Ticket;callback:F;value:i32}}package Packet<F>{{manages marker:Ticket;callback:F}}fn make()->impl Fn()->i32{{let n=8;return move()=>n}}fn pack<F:Fn()->i32>(callback:F)->Packet<F>{{return Packet::<F>{{marker:Ticket{{code:7}},callback:callback}}}}fn run<F:Fn()->i32>(packet:Packet<F>)->i32{{let a:Store<Item<F>>=store_new();let b:Store<Item<F>>=store_new();let x:Phantom<Item<F>>=first(give a);let y:Phantom<Item<F>>=second(give b);let z=Phantom::<Item<F>>{{marker:Ticket{{code:7}}}};return x.marker.code+y.marker.code+z.marker.code-21}}fn main()->i32{{return run(pack(make()))}}"
    );
    native("phantom-direct-retained", &good, "");
    let bad = format!(
        "{PHANTOM}fn main()->i32{{let b:Store<ItemB>=store_new();return accept(second(give b))-7}}"
    );
    let out = dir("phantom-mismatch");
    fs::write(out.join("source.zil"), &bad).unwrap();
    let report = compile_source(
        &unit(&bad),
        &CompileOptions {
            stop_after: Stage::Verify,
            ..CompileOptions::default()
        },
    );
    fs::write(out.join("report.json"), report.to_json()).unwrap();
    assert!(
        matches!(
            report.status,
            ResultCategory::SafeReject | ResultCategory::Unsupported
        ),
        "{}",
        report.to_json()
    );
    assert!(!report.artifacts.contains_key("executable"));
}

#[test]
fn canonical_product_names_do_not_erase_cross_module_field_privacy() {
    for field in ["child", "items"] {
        let mut project = imported_project();
        if field == "child" {
            project.modules[1].text = project.modules[1].text.replace("pub child", "child");
        }
        project.modules[0].text = format!(
            "import library;resource Ticket{{n:i32}}package Item{{manages ticket:Ticket;value:i32}}fn main()->i32{{let items:Store<Item>=store_new();let root=library.wrap(give items);let child=root.child;{}return 0}}",
            if field == "items" {
                "let secret=child.items;let n=store_len(secret);"
            } else {
                ""
            }
        );
        let out = dir(&format!("private-{field}"));
        let report = compile_project(
            &project,
            &CompileOptions {
                stop_after: Stage::Verify,
                emit_dir: Some(out.join("phases")),
                ..CompileOptions::default()
            },
        );
        fs::write(out.join("report.json"), report.to_json()).unwrap();
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{}",
            report.to_json()
        );
        assert!(
            report
                .diagnostics
                .iter()
                .any(|d| d.code == "E-FIELD-PRIVATE"),
            "{}",
            report.to_json()
        );
        assert!(!report.artifacts.contains_key("executable"));
    }
}

#[test]
fn natural_product_input_infers_t_across_two_retained_function_owners() {
    let definitions = "\nfn relay<T:StoreElement>(root:Root<T>)->Root<T>{return root}\nfn relay_again<T:StoreElement>(root:Root<T>)->Root<T>{return relay(root)}\n";
    let explicit = ORIGINAL.replace(
        "let root=wrap(give items)",
        "let root=relay_again::<Item<F>>(relay::<Item<F>>(wrap(give items)))",
    ) + definitions;
    native("product-input-explicit-control", &explicit, "AZ");
    let natural = ORIGINAL.replace(
        "let root=wrap(give items)",
        "let root=relay_again(relay(wrap(give items)))",
    ) + definitions;
    native("product-input-natural", &natural, "AZ");
}

const PHANTOM_RELAY: &str = r#"
fn phantom_relay<T:StoreElement>(value:Phantom<T>)->Phantom<T>{return value}
fn phantom_again<T:StoreElement>(value:Phantom<T>)->Phantom<T>{return phantom_relay(value)}
"#;
fn phantom_input_source(explicit: bool) -> String {
    let first = if explicit {
        "phantom_again::<Item<F>>(phantom_relay::<Item<F>>(first(give a)))"
    } else {
        "phantom_again(phantom_relay(first(give a)))"
    };
    let second = if explicit {
        "phantom_relay::<Item<F>>(second(give b))"
    } else {
        "phantom_relay(second(give b))"
    };
    format!(
        "{PHANTOM}{PHANTOM_RELAY}package Item<F>{{manages ticket:Ticket;callback:F;value:i32}}package Packet<F>{{manages marker:Ticket;callback:F}}fn make()->impl Fn()->i32{{let n=8;return move()=>n}}fn pack<F:Fn()->i32>(callback:F)->Packet<F>{{return Packet::<F>{{marker:Ticket{{code:7}},callback:callback}}}}fn run<F:Fn()->i32>(packet:Packet<F>)->i32{{let a:Store<Item<F>>=store_new();let b:Store<Item<F>>=store_new();let x={first};let y={second};let z=Phantom::<Item<F>>{{marker:Ticket{{code:7}}}};return x.marker.code+y.marker.code+z.marker.code-21}}fn main()->i32{{return run(pack(make()))}}"
    )
}

#[test]
fn natural_phantom_input_infers_actual_arguments_that_do_not_appear_in_fields() {
    native(
        "phantom-input-explicit-control",
        &phantom_input_source(true),
        "",
    );
    native("phantom-input-natural", &phantom_input_source(false), "");
}

const INPUT_RELAY_MODEL: &str = r#"
package InputBag<T:StoreElement>{manages items:Store<T>}
package InputRoot<T:StoreElement>{child:InputBag<T>}
package OtherRoot<T:StoreElement>{child:InputBag<T>}
package OtherPhantom<T:StoreElement>{manages marker:Ticket}
fn input_wrap<T:StoreElement>(give items:Store<T>)->InputRoot<T>{return InputRoot::<T>{child:InputBag::<T>{items:give items}}}
fn input_relay<T:StoreElement>(root:InputRoot<T>)->InputRoot<T>{return root}
fn input_pick<T:StoreElement>(left:InputRoot<T>,right:InputRoot<T>)->InputRoot<T>{return left}
fn phantom_pick<T:StoreElement>(left:Phantom<T>,right:Phantom<T>)->Phantom<T>{return left}
fn unconstrained<T:StoreElement>()->Phantom<T>{return Phantom::<T>{marker:Ticket{code:7}}}
"#;

#[test]
fn natural_input_inference_requires_the_same_family_and_one_complete_actual_t() {
    for (label, body) in [
        (
            "root-different-family-same-layout",
            "let items:Store<ItemA>=store_new();let other=OtherRoot::<ItemA>{child:InputBag::<ItemA>{items:give items}};let value=input_relay(other);",
        ),
        (
            "root-conflicting-actuals",
            "let a:Store<ItemA>=store_new();let b:Store<ItemB>=store_new();let value=input_pick(input_wrap(give a),input_wrap(give b));",
        ),
        (
            "root-explicit-wrong-actual",
            "let items:Store<ItemA>=store_new();let value=input_relay::<ItemB>(input_wrap(give items));",
        ),
        (
            "phantom-conflicting-actuals",
            "let a=Phantom::<ItemA>{marker:Ticket{code:7}};let b=Phantom::<ItemB>{marker:Ticket{code:7}};let value=phantom_pick(a,b);",
        ),
        (
            "phantom-different-family-same-layout",
            "let other=OtherPhantom::<ItemA>{marker:Ticket{code:7}};let value=phantom_relay(other);",
        ),
        ("phantom-no-input-constraint", "let value=unconstrained();"),
        (
            "give-is-still-not-product-move",
            "let items:Store<ItemA>=store_new();let old=input_wrap(give items);let value=input_relay(give old);",
        ),
    ] {
        let text = format!("{PHANTOM}{PHANTOM_RELAY}{INPUT_RELAY_MODEL}fn main(){{{body}}}");
        let out = dir(label);
        fs::write(out.join("source.zil"), &text).unwrap();
        let report = compile_source(
            &unit(&text),
            &CompileOptions {
                stop_after: Stage::Verify,
                emit_dir: Some(out.join("phases")),
                ..CompileOptions::default()
            },
        );
        fs::write(out.join("report.json"), report.to_json()).unwrap();
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{label}: {}",
            report.to_json()
        );
        assert!(
            report
                .diagnostics
                .iter()
                .any(|d| matches!(d.phase, Phase::Expand | Phase::Types)),
            "{label}: {}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 0);
        assert!(!report.artifacts.contains_key("executable"));
    }
}

fn nested_phantom_input_source(explicit: bool) -> String {
    let call = if explicit {
        "nested_relay::<Item<F>>(carrier)"
    } else {
        "nested_relay(carrier)"
    };
    format!(
        "{PHANTOM}package Carrier<T:StoreElement>{{manages items:Store<T>}}fn nested_relay<T:StoreElement>(value:Carrier<Phantom<T>>)->Carrier<Phantom<T>>{{return value}}package Item<F>{{manages ticket:Ticket;callback:F;value:i32}}package Packet<F>{{manages marker:Ticket;callback:F}}fn make()->impl Fn()->i32{{let n=8;return move()=>n}}fn pack<F:Fn()->i32>(callback:F)->Packet<F>{{return Packet::<F>{{marker:Ticket{{code:7}},callback:callback}}}}fn run<F:Fn()->i32>(packet:Packet<F>)->i32{{let items:Store<Phantom<Item<F>>> = store_new();let carrier=Carrier::<Phantom<Item<F>>>{{items:give items}};let moved={call};return i32(store_len(moved.items))}}fn main()->i32{{return run(pack(make()))}}"
    )
}

#[test]
fn natural_input_finds_t_in_nested_family_arguments_even_when_inner_t_is_phantom() {
    native(
        "nested-phantom-explicit-control",
        &nested_phantom_input_source(true),
        "",
    );
    native(
        "nested-phantom-natural",
        &nested_phantom_input_source(false),
        "",
    );
}

#[test]
fn nested_phantom_identity_survives_rebinding_between_two_ordinary_t_owners() {
    let text = nested_phantom_input_source(false).replace(
        "let moved=nested_relay(carrier)",
        "let moved:Carrier<Phantom<Item<F>>>=nested_again(nested_relay(carrier))",
    ) + "\nfn nested_again<T:StoreElement>(value:Carrier<Phantom<T>>)->Carrier<Phantom<T>>{return nested_relay(value)}\n";
    native("nested-phantom-two-T-owners", &text, "");
}

fn nested_phantom_owner_attack(owner_name: Option<&str>) -> CoreModule {
    let mut input = core(&nested_phantom_input_source(false));
    zil_types::check(&input, &TargetProfile::host_llvm(), &Limits::default())
        .expect("unmodified nested phantom source remains valid");
    let family = input
        .aggregate_templates
        .iter()
        .find(|t| t.name == "Carrier")
        .expect("outer Carrier family")
        .id;
    let run = input
        .functions
        .iter()
        .find(|f| f.name == "run")
        .expect("original F owner")
        .id;
    let index = input
        .aggregate_applications
        .iter()
        .position(|a| a.template == family && a.owner.is_some_and(|o| o.function == run))
        .expect("Carrier whose argument hides F in Phantom's nominal arguments");
    let replacement = owner_name.map(|name| {
        let function = input
            .functions
            .iter()
            .find(|f| f.name == name)
            .expect("alternate same-spelled F function");
        assert!(
            function
                .callable_parameters
                .iter()
                .any(|b| b.generic_name == "F")
        );
        assert_ne!(function.id, run);
        CoreAggregateOwner {
            module: input.module_id,
            function: function.id,
        }
    });
    input.aggregate_applications[index].owner = replacement;
    input
}

#[test]
fn hidden_phantom_arguments_cannot_borrow_another_functions_same_f_owner() {
    all_type_entries_reject(
        "hidden-phantom-same-F-wrong-owner",
        &nested_phantom_owner_attack(Some("pack")),
    );
}

#[test]
fn hidden_phantom_arguments_cannot_erase_their_outer_application_owner() {
    all_type_entries_reject(
        "hidden-phantom-absent-owner",
        &nested_phantom_owner_attack(None),
    );
}

#[test]
fn an_unused_concrete_recipe_cannot_capture_f_hidden_in_another_familys_arguments() {
    let mut input = core(&nested_phantom_input_source(false));
    let run = input.functions.iter().find(|f| f.name == "run").unwrap().id;
    let phantom = input
        .aggregate_templates
        .iter()
        .find(|t| t.name == "Phantom")
        .unwrap()
        .id;
    let header = input
        .aggregate_applications
        .iter()
        .find(|a| a.template == phantom && a.owner.is_some_and(|o| o.function == run))
        .and_then(|a| input.aggregates.iter().find(|d| d.id == a.aggregate))
        .expect("Phantom actual with F absent from its fields");
    let span = header.span;
    let hidden = header.name.clone();
    input.aggregate_templates.push(CoreAggregateTemplate {
        id: NodeId(9200),
        name: "UnusedCannotCaptureCallerF".into(),
        parameter: CoreTypeParameter {
            index: 0,
            name: "T".into(),
            bound: CoreTypeBound::StoreElement,
        },
        fields: vec![CoreAggregateFieldRecipe {
            name: "child".into(),
            public: false,
            manages: false,
            ty: CoreAggregateTypeRecipe::Concrete(Type::Named(hidden)),
            span,
        }],
        span,
    });
    all_type_entries_reject("unused-recipe-hidden-free-F", &input);
}

fn nested_raw_natural_core() -> CoreModule {
    nested_raw_natural_core_from(&nested_phantom_input_source(false))
}

fn nested_raw_natural_core_from(text: &str) -> CoreModule {
    let mut input = core(text);
    let run = input
        .functions
        .iter_mut()
        .find(|f| f.name == "run")
        .unwrap();
    let initializer = run
        .body
        .statements
        .iter_mut()
        .find_map(|s| match &mut s.kind {
            CoreStatementKind::Binding {
                name,
                initializer: Some(e),
                ..
            } if name == "moved" => Some(e),
            _ => None,
        })
        .expect("nested call initializer");
    let CoreExprKind::Call { callee, .. } = &mut initializer.kind else {
        panic!("expected a static call, got {initializer:#?}");
    };
    let CoreExprKind::TypeApply {
        callee: target,
        type_arguments,
    } = &callee.kind
    else {
        panic!("Expand should supply the direct control's type argument: {callee:#?}");
    };
    assert!(matches!(&target.kind, CoreExprKind::Name(n) if n == "nested_relay"));
    assert_eq!(type_arguments.len(), 1);
    *callee = target.clone();
    input
}

#[test]
fn raw_static_call_can_infer_t_through_nested_phantom_origin_arguments() {
    let input = nested_raw_natural_core();
    let out = dir("raw-nested-phantom-natural");
    fs::write(out.join("core.txt"), format!("{input:#?}")).unwrap();
    let target = TargetProfile::host_llvm();
    let limits = Limits::default();
    let single = zil_types::check(&input, &target, &limits);
    let checked = zil_types::check_program(&program(&input), &target, &limits);
    fs::write(out.join("single.txt"), format!("{single:#?}")).unwrap();
    fs::write(out.join("program.txt"), format!("{checked:#?}")).unwrap();
    let module =
        single.expect("raw static ordinary call must infer from full nested nominal arguments");
    checked.expect("program entry must preserve the same natural-call inference");
    let plan = zil_analyze::analyze(&module, &limits).unwrap();
    let bytes = zil_ir::wire::encode_module_candidate(&module, &plan, &limits).unwrap();
    fs::write(out.join("candidate.wire"), &bytes).unwrap();
    zil_verify::verify_module(module, plan, &target, &limits).unwrap();
    zil_verify::verify_module_bytes(&bytes, &target, &limits).unwrap();
}

#[test]
fn nested_nominal_inference_work_has_an_exact_fresh_limit_boundary() {
    types_work_boundary(&nested_raw_natural_core(), "types-nested-inference-budget");
}

#[test]
fn recursive_query_accepts_two_finite_instances_of_the_same_phantom_family() {
    let text = nested_phantom_input_source(false)
        .replace("Carrier<Phantom<T>>", "Carrier<Phantom<Phantom<T>>>")
        .replace(
            "Store<Phantom<Item<F>>>",
            "Store<Phantom<Phantom<Item<F>>>>",
        )
        .replace(
            "Carrier::<Phantom<Item<F>>>",
            "Carrier::<Phantom<Phantom<Item<F>>>>",
        );
    let target = TargetProfile::host_llvm();
    let limits = Limits::default();
    let original = core(&text);
    zil_types::check(&original, &target, &limits).expect("finite source-derived control");
    let input = nested_raw_natural_core_from(&text);
    let out = dir("raw-repeated-phantom-family");
    fs::write(out.join("source.zil"), &text).unwrap();
    fs::write(out.join("core.txt"), format!("{input:#?}")).unwrap();
    let single = zil_types::check(&input, &target, &limits);
    let checked = zil_types::check_program(&program(&input), &target, &limits);
    fs::write(out.join("single.txt"), format!("{single:#?}")).unwrap();
    fs::write(out.join("program.txt"), format!("{checked:#?}")).unwrap();
    single.expect("the same family with distinct finite full arguments is not an origin cycle");
    checked.expect("program preparation must preserve both finite family instances");
}
