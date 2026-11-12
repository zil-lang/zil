//! Ordinary cross-module lazy owning adapters and lending callback consumers.
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_base::{
    FileId, Fingerprint, Limits, ModuleId, Phase, ResultCategory, SourceUnit, TargetProfile,
};
use zil_driver::{
    CompileOptions, NativeBackend, Optimization, ProjectSources, Stage, compile_project,
};
use zil_ir::{CoreImport, CoreProgram};

static NEXT: AtomicUsize = AtomicUsize::new(0);
const ITER: &str = include_str!("../../../lib/std/iter.zil");
const STORE: &str = include_str!("../../../lib/std/store.zil");
const OWNING: &str =
    include_str!("../../../tests/language-readiness/iterator-adapters/owning-pipeline.zil");
const BORROWED: &str =
    include_str!("../../../tests/language-readiness/iterator-adapters/borrowed-consumer.zil");
const OWNED_CALLBACK: &str =
    include_str!("../../../tests/language-readiness/iterator-adapters/owned-consumer.zil");
const OWNED_FOLD: &str =
    include_str!("../../../tests/language-readiness/iterator-adapters/owned-fold.zil");
const STORE_LENDING: &str =
    include_str!("../../../tests/language-readiness/iterator-adapters/store-lending.zil");

fn project(source: &str) -> ProjectSources {
    ProjectSources {
        root: ModuleId(0),
        modules: [
            ("main.zil", source),
            ("std.iter.zil", ITER),
            ("std.store.zil", STORE),
        ]
        .into_iter()
        .enumerate()
        .map(|(i, (name, text))| SourceUnit::new(FileId(i as u32), ModuleId(i as u32), name, text))
        .collect(),
        paths: [
            (vec!["std".into(), "iter".into()], ModuleId(1)),
            (vec!["std".into(), "store".into()], ModuleId(2)),
        ]
        .into(),
    }
}
fn typed(source: &str) -> zil_ir::TypedModule {
    let p = project(source);
    let asts = p
        .modules
        .iter()
        .map(|u| zil_syntax::parse(u).into_result().unwrap())
        .collect::<Vec<_>>();
    let imports = asts
        .iter()
        .map(|ast| {
            ast.items
                .iter()
                .filter_map(|item| {
                    if let zil_syntax::ast::ItemKind::Import(import) = &item.kind {
                        Some(CoreImport {
                            alias: import
                                .alias
                                .clone()
                                .unwrap_or_else(|| import.path.last().unwrap().clone()),
                            module: p.paths[&import.path],
                            span: item.span,
                        })
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    let limits = Limits::default();
    let expanded = zil_expand::expand_program_with_limits_target_context(
        &p.modules,
        &asts.iter().collect::<Vec<_>>(),
        &imports,
        &limits,
        &TargetProfile::host_llvm(),
        zil_expand::source_context::SourceIdentityContext::ClosedSnapshot {
            root: p.root,
            aliases: &p.paths,
            declarations: zil_expand::source_context::DeclarationPlacement::Original,
        },
    )
    .unwrap();
    let core = CoreProgram {
        root: p.root,
        source_fingerprint: Fingerprint::of(
            p.modules
                .iter()
                .flat_map(|s| s.text.as_bytes())
                .copied()
                .collect::<Vec<_>>()
                .as_slice(),
        ),
        modules: expanded.into_iter().map(|(m, _)| m).collect(),
    };
    let typed = zil_types::check_program(&core, &TargetProfile::host_llvm(), &limits)
        .unwrap()
        .module;
    typed
}
fn raw(source: &str) -> (zil_ir::TypedModule, zil_ir::CandidatePlan) {
    let typed = typed(source);
    let limits = Limits::default();
    let plan = zil_analyze::analyze(&typed, &limits).unwrap();
    zil_verify::verify_module(typed.clone(), plan.clone(), &typed.target, &limits).unwrap();
    let wire = zil_ir::wire::encode_module_candidate(&typed, &plan, &limits).unwrap();
    assert_eq!(
        zil_ir::wire::decode_module_candidate(&wire, &limits).unwrap(),
        (typed.clone(), plan.clone())
    );
    zil_verify::verify_module_bytes(&wire, &typed.target, &limits).unwrap();
    (typed, plan)
}
fn dir(label: &str) -> PathBuf {
    let root = std::env::var_os("ZIL_TEST_REPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let dir = root.join(format!(
        "iterator-{label}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&dir).unwrap();
    dir
}
fn quote(path: &Path) -> String {
    format!("'{}'", path.to_string_lossy().replace('\'', "'\"'\"'"))
}
fn native(label: &str, source: &str, expected: &[u8], failures: bool) {
    for backend in [NativeBackend::Clang, NativeBackend::LlvmApi] {
        if backend == NativeBackend::LlvmApi && !cfg!(feature = "llvm-api") {
            continue;
        }
        for optimization in [Optimization::O0, Optimization::O2] {
            let dir = dir(label);
            fs::write(dir.join("main.zil"), source).unwrap();
            let clang = std::env::var_os("ZIL_CLANG")
                .map(PathBuf::from)
                .unwrap_or_else(|| "clang".into());
            let wrapper = if failures {
                use std::os::unix::fs::PermissionsExt;
                let observer = dir.join("allocation.o");
                let compiled = Command::new(&clang)
                    .args(["-std=c11", "-Wall", "-Wextra", "-Werror", "-c"])
                    .arg(
                        Path::new(env!("CARGO_MANIFEST_DIR"))
                            .join("../../tests/language-readiness/iterator-adapters/allocation.c"),
                    )
                    .arg("-o")
                    .arg(&observer)
                    .output()
                    .unwrap();
                assert!(compiled.status.success(), "{compiled:?}");
                let wrapper = dir.join("link.sh");
                fs::write(&wrapper,format!("#!/bin/sh\nset -eu\nlink=1\nfor arg in \"$@\"; do case \"$arg\" in -c|-S|-E|--version) link=0;; esac; done\nif [ \"$link\" = 1 ]; then exec {} \"$@\" -x none {} -Wl,--wrap=realloc -Wl,--wrap=free; fi\nexec {} \"$@\"\n",quote(&clang),quote(&observer),quote(&clang))).unwrap();
                fs::set_permissions(&wrapper, fs::Permissions::from_mode(0o700)).unwrap();
                Some(wrapper)
            } else {
                None
            };
            let executable = dir.join("program");
            let report = compile_project(
                &project(source),
                &CompileOptions {
                    stop_after: Stage::Build,
                    backend,
                    optimization,
                    clang: wrapper,
                    output: Some(executable.clone()),
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
            assert_eq!(report.verification_runs, 1);
            let generated = report.native_codegen.as_ref().unwrap();
            assert_eq!(generated.backend, backend);
            assert!(generated.fallback.is_none());
            let mut cases = vec![(None, 0, expected)];
            if failures {
                cases.extend([
                    (Some("1"), 1, b"|1:0:0:0".as_slice()),
                    (Some("2"), 4, b"ABC|2:1:1:0".as_slice()),
                    (Some("3"), 4, b"ABCPW|3:2:2:0".as_slice()),
                ]);
            }
            for (failure, exit, stdout) in cases {
                let mut command = Command::new(&executable);
                if let Some(at) = failure {
                    command.env("ZIL_ITER_FAIL_AT", at);
                }
                let out = command.output().unwrap();
                fs::write(
                    dir.join(format!("run-{}.txt", failure.unwrap_or("success"))),
                    format!("{out:?}"),
                )
                .unwrap();
                assert_eq!(
                    out.status.code(),
                    Some(exit),
                    "{label} {backend:?}/{optimization:?} fail={failure:?}: {out:?}"
                );
                assert_eq!(out.stdout, stdout, "{label} fail={failure:?}");
                assert!(out.stderr.is_empty());
            }
            if std::env::var_os("ZIL_TEST_REPORT_DIR").is_none() {
                fs::remove_dir_all(dir).unwrap();
            }
        }
    }
}
#[test]
fn owning_map_filter_collect_are_lazy_composable_and_cleanup_on_allocation_failure() {
    raw(OWNING);
    native("owning-pipeline", OWNING, b"ACPWB|3:3:3:0", true);
}
#[test]
fn associated_owning_items_reach_borrowed_and_owning_callback_consumers() {
    for (label, source, stdout) in [
        ("borrowed", BORROWED, b"AB".as_slice()),
        ("owned-callback", OWNED_CALLBACK, b"BA".as_slice()),
        ("owned-fold", OWNED_FOLD, b"BAZ".as_slice()),
    ] {
        raw(source);
        native(label, source, stdout, false);
    }
}
#[test]
fn independent_structural_and_nominal_premises_retain_store_lending_sources() {
    raw(STORE_LENDING);
    native("store-lending", STORE_LENDING, b"QAB", false);
}
#[test]
fn unused_definitions_check_life_origins_and_true_loop_break_edges() {
    let valid = "fn forever<T:Copy>(value:T)->T{while true{return value}}fn main()->i32{return forever(42)-42}";
    let report = compile_project(&project(valid), &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    let bad_loop = "fn missing<T:Copy>(value:T)->T{while true{break}}fn main(){}";
    let wrong_origin = STORE_LENDING
        .split("fn main()")
        .next()
        .unwrap()
        .replace("map:&'a Map<K,V>,key:&K", "map:&Map<K,V>,key:&'a K")
        + "fn main(){}";
    for bad in [bad_loop, wrong_origin.as_str()] {
        let report = compile_project(&project(bad), &CompileOptions::default());
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{}",
            report.to_json()
        );
        assert_eq!(report.phase, Phase::Expand, "{}", report.to_json());
    }
}

#[test]
fn projected_premises_and_store_lending_use_the_current_expand_budget() {
    let source = SourceUnit::new(FileId(0), ModuleId(0), "store-lending.zil", STORE_LENDING);
    let syntax = zil_syntax::parse(&source).into_result().unwrap();
    let run = |n| {
        zil_expand::expand_with_map_limits(
            &source,
            &syntax,
            &Limits {
                max_analysis_steps: n,
                ..Limits::default()
            },
        )
        .map(|result| result.core)
    };
    let expected = run(Limits::default().max_analysis_steps).unwrap();
    let (mut low, mut high) = (0, Limits::default().max_analysis_steps);
    while low + 1 < high {
        let middle = low + (high - low) / 2;
        match run(middle) {
            Ok(actual) => {
                assert_eq!(actual, expected);
                high = middle;
            }
            Err(errors) => {
                assert!(!errors.is_empty());
                assert!(
                    errors
                        .iter()
                        .all(|e| e.phase == Phase::Expand
                            && e.category == ResultCategory::ResourceLimit),
                    "{errors:#?}"
                );
                low = middle;
            }
        }
    }
    assert_eq!(run(high).unwrap(), expected);
    assert!(
        run(high - 1)
            .unwrap_err()
            .iter()
            .all(|e| e.category == ResultCategory::ResourceLimit)
    );
    assert_eq!(run(high).unwrap(), expected);
    println!("ordinary owning projections Expand N={high}; N-1 rejected; fresh N equal");
}

#[test]
fn a_lending_callback_cannot_alias_its_live_iterator_receiver() {
    let control = compile_project(&project(BORROWED), &CompileOptions::default());
    assert_eq!(
        control.status,
        ResultCategory::Success,
        "{}",
        control.to_json()
    );
    let conflict = BORROWED.replace(
        "var visit=(item:&Item)=>{total+=item.value}",
        "var visit=(item:&Item)=>{cursor.index+=1;total+=item.value}",
    );
    assert_ne!(conflict, BORROWED);
    let rejected = compile_project(&project(&conflict), &CompileOptions::default());
    assert_eq!(
        rejected.status,
        ResultCategory::SafeReject,
        "{}",
        rejected.to_json()
    );
    assert!(!rejected.diagnostics.is_empty());
}

#[test]
fn an_inline_manager_projection_still_pins_its_complete_owner() {
    let (mut typed, mut plan) = raw(STORE_LENDING);
    let source = STORE_LENDING.replace(
        "Option.Some(value)=>{return value.value-42}",
        "Option.Some(value)=>{consume(wrapper);return value.value-42}",
    );
    // A projected Store result keeps its complete inline manager alive.
    let source = format!("fn consume<T:Value+SyncDrop>(value:T){{}}\n{source}");
    let rejected = compile_project(&project(&source), &CompileOptions::default());
    assert_eq!(
        rejected.status,
        ResultCategory::SafeReject,
        "{}",
        rejected.to_json()
    );
    let mut changed = 0;
    for function in &mut typed.functions {
        if !function.name.ends_with("main") {
            continue;
        }
        for block in &mut function.blocks {
            let drop=block.instructions.iter().position(|instruction|{
                if let zil_ir::InstructionKind::DropPackage{local}=instruction.kind {
                    matches!(&function.locals[local.0 as usize].ty,zil_ir::Type::Package{fields,..}if fields.iter().any(|(field,_)|field=="map"))
                }else{false}
            });
            let read = block.instructions.iter().position(|instruction| {
                matches!(
                    instruction.kind,
                    zil_ir::InstructionKind::ReadAddress { .. }
                )
            });
            if let (Some(drop), Some(read)) = (drop, read)
                && read < drop
            {
                let cleanup = block.instructions.remove(drop);
                block.instructions.insert(read, cleanup);
                changed += 1;
                break;
            }
        }
    }
    assert_eq!(
        changed, 1,
        "an actual parent cleanup must move before its child read"
    );
    plan.module_fingerprint = typed.fingerprint();
    let limits = Limits::default();
    let bytes = zil_ir::wire::encode_module_candidate(&typed, &plan, &limits).unwrap();
    for errors in [
        zil_verify::verify_module(typed.clone(), plan, &typed.target, &limits).unwrap_err(),
        zil_verify::verify_module_bytes(&bytes, &typed.target, &limits).unwrap_err(),
    ] {
        assert!(
            errors.iter().any(|e| e.phase == Phase::Verify
                && e.category == ResultCategory::SafeReject
                && e.code != "E-VERIFY-MODULE"),
            "{errors:#?}"
        );
    }
}

#[test]
fn equal_typed_managers_retain_independent_actual_roots() {
    let original=STORE_LENDING.replace("    let key=Item", "    let other_entries:Store<Entry<Item,Item>>=store_new()\n    let other_map=Map::<Item,Item>{entries:give other_entries}\n    let other_wrapper=Wrapper::<Item,Item>{map:other_map}\n    let key=Item");
    let valid = original.replace(
        "Option.Some(value)=>{return value.value-42}",
        "Option.Some(value)=>{consume(other_wrapper);return value.value-42}",
    );
    assert_ne!(valid, STORE_LENDING);
    let valid = format!("fn consume<T:Value+SyncDrop>(value:T){{}}\n{valid}");
    let report = compile_project(&project(&valid), &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1);
    let invalid = valid.replace("consume(other_wrapper)", "consume(wrapper)");
    let report = compile_project(&project(&invalid), &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
}

#[test]
fn value_is_an_inline_move_contract_and_cleanup_is_an_independent_premise() {
    let prefix =
        "fn identity<T:Value>(value:T)->T{return value} fn dispose<T:Value+SyncDrop>(value:T){} ";
    for main in [
        "fn main()->i32{return identity(42)-42}",
        "fn main(){dispose(42)}",
        "package Holder<T:Value>{value:T}fn unwrap<T:Value>(value:Holder<T>)->T{return value.value}fn main()->i32{return unwrap(Holder::<i32>{value:42})-42}",
    ] {
        let source = format!("{prefix}{main}");
        let report = compile_project(&project(&source), &CompileOptions::default());
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
    }
    for bad in [
        "fn drop_missing<T:Value>(value:T){}",
        "fn early<T:Value>(value:T)->i32{return 0}",
        "fn replaced<T:Value>(a:T,b:T)->T{var value=a;value=b;return value}",
        "fn bypass<T:Value>(value:T){while true{let held=value;break}}",
        "fn bad_store<T:Value>(give values:Store<T>){}",
        "package Hidden<T:Value>{manages values:Store<T>}",
        "fn pretend<T:Value>(value:T)->T{let again=value;return value}",
    ] {
        let source = format!("{bad} fn main(){{}}");
        let report = compile_project(&project(&source), &CompileOptions::default());
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{bad}: {}",
            report.to_json()
        );
        assert_eq!(report.phase, Phase::Expand, "{bad}: {}", report.to_json());
    }
}

#[test]
fn nested_iterator_store_uses_complete_actual_resource_definitions() {
    let valid =
        "import std.iter\nfn need<T:Value+SyncDrop>(){} fn main(){need::<iter.Drain<i32>>()}";
    let report = compile_project(&project(valid), &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    let nested = "import std.iter\nfn main(){let nested:Store<iter.Drain<i32>>=store_new()}";
    let report = compile_project(&project(nested), &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    raw(nested);
    let consumer =
        include_str!("../../../tests/language-readiness/iterator-adapters/nested-drain.zil");
    raw(consumer);
    native("nested-drain", consumer, b"BA", false);

    // Changing an existing Store<i32> into Store<Drain<i32>> without its
    // actual registered definition still cannot manufacture a valid owner.
    let source = "import std.iter\nfn make(give values:Store<i32>)->iter.Drain<i32>{return iter.drain(give values)} fn main(){let values:Store<i32>=store_new()}";
    let (mut typed, mut plan) = raw(source);
    let drain = typed
        .functions
        .iter()
        .find(|f| f.name.ends_with("make"))
        .unwrap()
        .return_type
        .clone();
    assert!(
        matches!(&drain,zil_ir::Type::Package{fields,..} if fields.iter().any(|(_,ty)|matches!(ty,zil_ir::Type::Managed(_))))
    );
    fn nested_type(ty: &mut zil_ir::Type, drain: &zil_ir::Type) {
        match ty {
            zil_ir::Type::Managed(inner)
            | zil_ir::Type::Desc(inner)
            | zil_ir::Type::Address { inner, .. } => nested_type(inner, drain),
            zil_ir::Type::OwnedStore { element } => **element = drain.clone(),
            _ => {}
        }
    }
    let main = typed
        .functions
        .iter_mut()
        .find(|f| f.name == "main")
        .unwrap();
    for local in &mut main.locals {
        nested_type(&mut local.ty, &drain);
    }
    for value in &mut main.values {
        nested_type(value, &drain);
    }
    let mut changed = 0;
    for instruction in main.blocks.iter_mut().flat_map(|b| &mut b.instructions) {
        if let zil_ir::InstructionKind::OwnedStoreNew { element } = &mut instruction.kind {
            *element = drain.clone();
            changed += 1;
        }
    }
    assert_eq!(changed, 1);
    plan.module_fingerprint = typed.fingerprint();
    let limits = Limits::default();
    let bytes = zil_ir::wire::encode_module_candidate(&typed, &plan, &limits).unwrap();
    for (raw, plan) in [
        (typed.clone(), plan),
        zil_ir::wire::decode_module_candidate(&bytes, &limits).unwrap(),
    ] {
        for errors in [
            zil_analyze::analyze(&raw, &limits).unwrap_err(),
            zil_verify::verify_module(raw.clone(), plan, &raw.target, &limits).unwrap_err(),
        ] {
            assert!(
                errors
                    .iter()
                    .any(|e| e.message.contains("Store") && e.code != "E-VERIFY-MODULE"),
                "{errors:?}"
            );
        }
    }
}

#[test]
fn value_product_recipes_cannot_forge_a_store_element_premise() {
    use zil_ir::{CoreAggregateTypeRecipe as Recipe, CoreTypeBound};
    let source = SourceUnit::new(
        FileId(0),
        ModuleId(0),
        "value.zil",
        "package Holder<T:Value>{value:T} fn main(){}",
    );
    let ast = zil_syntax::parse(&source).into_result().unwrap();
    let mut core = zil_expand::expand(&source, &ast).unwrap();
    let limits = Limits::default();
    zil_types::check(&core, &TargetProfile::host_llvm(), &limits).unwrap();
    let template = core
        .aggregate_templates
        .iter_mut()
        .find(|t| t.name == "Holder")
        .unwrap();
    assert_eq!(template.parameters[0].bound, CoreTypeBound::Value);
    template.fields[0].manages = true;
    template.fields[0].ty = Recipe::Store {
        element: Box::new(Recipe::Parameter { index: 0 }),
    };
    let errors = zil_types::check(&core, &TargetProfile::host_llvm(), &limits).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|e| e.phase == Phase::Types && e.message.contains("Store")),
        "{errors:?}"
    );
}

#[test]
fn borrowed_values_retain_the_actual_source_when_forwarded() {
    let source = "fn identity<T:Value>(value:T)->T{return value} fn main()->i32{let input=42;let output=identity(&input);return *output-42}";
    let report = compile_project(&project(source), &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
}

#[test]
fn value_is_not_a_task_synchronous_drop_certificate() {
    let safe = "fn need<T:Value>(){} fn unused(give task:Task<i32>){task_finish_destroy(give task)} fn main(){need::<Task<i32>>()}";
    let report = compile_project(&project(safe), &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    let unknown = safe.replace("task_finish_destroy(give task)", "");
    let report = compile_project(&project(&unknown), &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
    assert_eq!(report.phase, Phase::Analyze, "{}", report.to_json());
    let forged = safe.replace("T:Value>", "T:Value+SyncDrop>");
    let report = compile_project(&project(&forged), &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
    assert_eq!(report.phase, Phase::Expand, "{}", report.to_json());
}

#[test]
fn owning_sum_values_move_and_clean_the_actual_variant_once() {
    let source = "extern \"C\" fn putchar(n:i32)->i32;resource Mark{code:i32}impl Drop for Mark{fn drop(self:Self){unsafe{putchar(.code)}}}package Item{manages mark:Mark;value:i32}enum Choice{Some(Item),None}fn identity<T:Value>(value:T)->T{return value}fn main()->i32{let value=identity(Choice.Some(Item{mark:Mark{code:65},value:42}));match value{Choice.Some(item)=>{return item.value-42},Choice.None()=>{return 1}}}";
    raw(source);
    native("sum-value", source, b"A", false);
}

#[test]
fn qualified_borrowed_values_forward_the_complete_context() {
    let source = "struct View<life a>{value:&'a i32}fn identity<T:Value>(value:T)->T{return value}fn forward<life a>(input:&'a i32)->View<'a>{let view:View<'a>=View::<'a>{value:input};return identity(view)}fn main()->i32{var input=42;let output=forward(&input);return *output.value-42}";
    let report = compile_project(&project(source), &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    raw(source);
    let conflict = source.replace(
        "return *output.value-42",
        "input=99;return *output.value-42",
    );
    let report = compile_project(&project(&conflict), &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
}

#[test]
fn an_associated_argument_cannot_supply_its_own_missing_iterator_premise() {
    let source = "import std.iter\nfn unused<I:Value+SyncDrop,M:iter.Mapper<I.Item>+Value+SyncDrop>(input:I,callback:M){} fn main(){}";
    let report = compile_project(&project(source), &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
    assert_eq!(report.phase, Phase::Expand, "{}", report.to_json());
}

#[test]
fn owning_iterator_items_are_values_and_only_collect_requires_store_elements() {
    let source = include_str!(
        "../../../tests/language-readiness/iterator-adapters/ordinary-value-items.zil"
    );
    raw(source);
    native("value-items", source, b"AGK", false);
    let missing = "import std.iter\nfn unused<I:iter.Iterator+Value+SyncDrop>(input:I){let result=iter.collect(input)}fn main(){}";
    let report = compile_project(&project(missing), &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
    assert_eq!(report.phase, Phase::Expand, "{}", report.to_json());
}

#[test]
fn independent_value_actual_contexts_keep_their_joint_partition() {
    let source = r#"
struct View<life a>{value:&'a i32}
fn choose<T:Value,U:Value+SyncDrop>(first:T,second:U)->T{return first}
fn combine<life a,life b>(left:&'a i32,right:&'b mut i32)->i32{
 let first:View<'a>=View::<'a>{value:left}
 let second:View<'b>=View::<'b>{value:&*right}
 let selected=choose(first,second)
 *right=100
 let again:View<'a>=View::<'a>{value:left}
 let same:View<'a>=View::<'a>{value:left}
 let selected_same=choose(again,same)
 return *selected.value+*selected_same.value
}
fn main()->i32{let left=41;var right=1;let other=42;var last=2;return combine(&left,&mut right)+combine(&other,&mut last)-166}
"#;
    let (typed, _) = raw(source);
    assert_eq!(
        typed
            .functions
            .iter()
            .filter(|function| function.name.contains("choose"))
            .count(),
        2
    );
    native("value-context-partition", source, b"", false);
    let wrong = source.replace(
        "let second:View<'b>=View::<'b>{value:&*right}",
        "let second:View<'a>=View::<'b>{value:&*right}",
    );
    let report = compile_project(&project(&wrong), &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
}

#[test]
fn structural_bound_names_preserve_existing_ordinary_nominal_types() {
    let source = "resource R{n:i32}package Value{manages mark:R;n:i32}struct SyncDrop{n:i32}fn identity<T:Value+SyncDrop>(value:T)->T{return value}fn main()->i32{let value=identity(Value{mark:R{n:1},n:42});let label:SyncDrop=SyncDrop{n:0};return value.n+label.n-42}";
    for report in [
        compile_project(&project(source), &CompileOptions::default()),
        zil_driver::compile_source(
            &SourceUnit::new(FileId(0), ModuleId(0), "nominal.zil", source),
            &CompileOptions::default(),
        ),
    ] {
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
    }
    let aliases = "type Value=i32;type SyncDrop=i32;fn identity<T:Value+SyncDrop>(value:T)->T{return value}fn main()->i32{let value:Value=42;return identity(value)-42}";
    for report in [
        compile_project(&project(aliases), &CompileOptions::default()),
        zil_driver::compile_source(
            &SourceUnit::new(FileId(0), ModuleId(0), "aliases.zil", aliases),
            &CompileOptions::default(),
        ),
    ] {
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
    }
    for bad in [
        "fn bad<Value>(value:Value)->Value{return value}",
        "trait Value{}",
        "trait SyncDrop{}",
        "struct Item{n:i32}impl Value for Item{}",
    ] {
        let source = format!("{bad} fn main(){{}}");
        let report = compile_project(&project(&source), &CompileOptions::default());
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{bad}: {}",
            report.to_json()
        );
    }
}

#[test]
fn observing_zero_debt_children_does_not_restore_a_partially_taken_parent() {
    let source = "package Pair{value:i32}package Parent{left:Pair;right:Pair}fn consume(value:Parent)->i32{return value.left.value}fn main()->i32{let parent=Parent{left:Pair{value:1},right:Pair{value:2}};let other=Parent{left:Pair{value:4},right:Pair{value:5}};let child=parent.left;return child.value+consume(other)-5}";
    let (mut typed, mut plan) = raw(source);
    let main = typed
        .functions
        .iter_mut()
        .find(|f| f.name == "main")
        .unwrap();
    let parent = main
        .locals
        .iter()
        .find(|local| local.name == "parent")
        .unwrap()
        .id;
    let other = main
        .locals
        .iter()
        .find(|local| local.name == "other")
        .unwrap()
        .id;
    assert!(main.blocks.iter().flat_map(|b| &b.instructions).any(
        |i| matches!(i.kind,zil_ir::InstructionKind::TakePackageField{local,..} if local==parent)
    ));
    let mut changed = 0;
    for instruction in main.blocks.iter_mut().flat_map(|b| &mut b.instructions) {
        if let zil_ir::InstructionKind::TakeLocal { local } = &mut instruction.kind
            && *local == other
        {
            *local = parent;
            changed += 1;
        }
    }
    assert_eq!(changed, 1);
    plan.module_fingerprint = typed.fingerprint();
    let limits = Limits::default();
    let bytes = zil_ir::wire::encode_module_candidate(&typed, &plan, &limits).unwrap();
    for (raw, plan) in [
        (typed.clone(), plan),
        zil_ir::wire::decode_module_candidate(&bytes, &limits).unwrap(),
    ] {
        for errors in [
            zil_analyze::analyze(&raw, &limits).unwrap_err(),
            zil_verify::verify_module(raw.clone(), plan, &raw.target, &limits).unwrap_err(),
        ] {
            assert!(
                errors.iter().any(
                    |e| e.category == ResultCategory::SafeReject && e.code != "E-VERIFY-MODULE"
                ),
                "{errors:?}"
            );
        }
    }
}

#[test]
fn store_element_entails_value_and_synchronous_cleanup_in_unused_definitions() {
    let valid = "import std.iter\nfn need<T:Value+SyncDrop>(value:T){} fn dispose<T:StoreElement>(value:T){need(value)} fn projected<I:iter.Iterator+Value+SyncDrop>(input:I,value:I.Item) where I.Item:StoreElement {need(value)} fn main(){}";
    let report = compile_project(&project(valid), &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    for invalid in [
        "fn need<T:SyncDrop>(value:T){}fn missing<T:Value>(value:T){need(value)}fn main(){}",
        "fn need<T:StoreElement>(value:T){}fn inverse<T:Value+SyncDrop>(value:T){need(value)}fn main(){}",
        "fn need<T:SyncDrop>(){}fn main(){need::<Task<i32>>()}",
    ] {
        let report = compile_project(&project(invalid), &CompileOptions::default());
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{}",
            report.to_json()
        );
        assert_eq!(report.phase, Phase::Expand, "{}", report.to_json());
    }
}

#[test]
fn all_original_owning_iterators_reconstruct_default_analysis_candidates() {
    let cases = [
        ("pipeline", OWNING),
        ("borrowed", BORROWED),
        ("callback", OWNED_CALLBACK),
        ("fold", OWNED_FOLD),
        ("lending", STORE_LENDING),
        (
            "value-items",
            include_str!(
                "../../../tests/language-readiness/iterator-adapters/ordinary-value-items.zil"
            ),
        ),
        (
            "nested-drain",
            include_str!("../../../tests/language-readiness/iterator-adapters/nested-drain.zil"),
        ),
    ];
    let limits = Limits::default();
    let mut failures = vec![];
    for (name, source) in cases {
        let typed = typed(source);
        let artifacts = std::env::var_os("ZIL_STDLIB_ANALYZE_ARTIFACTS")
            .map(|path| std::path::PathBuf::from(path).join(name));
        if let Some(path) = &artifacts {
            fs::create_dir_all(path).unwrap();
            fs::write(path.join("main.zil"), source).unwrap();
            fs::write(path.join("typed.txt"), format!("{typed:#?}")).unwrap();
        }
        match zil_analyze::analyze(&typed, &limits) {
            Ok(plan) => {
                let wire = zil_ir::wire::encode_module_candidate(&typed, &plan, &limits).unwrap();
                assert_eq!(
                    zil_ir::wire::decode_module_candidate(&wire, &limits).unwrap(),
                    (typed, plan)
                );
                if let Some(path) = artifacts {
                    fs::write(path.join("candidate.wire"), wire).unwrap();
                }
            }
            Err(errors) => failures.push((name, errors)),
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}
