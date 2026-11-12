//! Ordinary generic Tuple values keep each qualified child's domain and loans.
use std::{fs, process::Command};
use zil_base::{FileId, Limits, ModuleId, ResultCategory, SourceUnit, TargetProfile};
use zil_driver::{CompileOptions, NativeBackend, Optimization, Stage, compile_source};
use zil_ir::{StorageBase, Type};

const SHARED: &str = include_str!("../../../tests/fixtures/qualified-values/compound.zil");
fn input(text: &str) -> SourceUnit {
    SourceUnit::new(FileId(0), ModuleId(0), "qualified-compound.zil", text)
}
#[test]
fn shared_tuple_children_keep_distinct_domains_in_raw_and_wire() {
    let source = input(SHARED);
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    let syntax = zil_syntax::parse(&source).into_result().unwrap();
    let core = zil_expand::expand_with_map_limits_target_context(
        &source, &syntax, &limits, &target,
        zil_expand::source_context::SourceIdentityContext::ClosedSnapshot {
            root: source.module_id,
            aliases: &std::collections::BTreeMap::new(),
            declarations: zil_expand::source_context::DeclarationPlacement::Original,
        },
    ).unwrap().core;
    let module = zil_types::check(&core, &target, &limits).unwrap();
    let plan = zil_analyze::analyze(&module, &limits).unwrap();
    zil_verify::verify_module(module.clone(), plan.clone(), &target, &limits).unwrap();
    let wire = zil_ir::wire::encode_module_candidate(&module, &plan, &limits).unwrap();
    let (decoded, candidate) = zil_ir::wire::decode_module_candidate(&wire, &limits).unwrap();
    zil_verify::verify_module(decoded, candidate, &target, &limits).unwrap();
    let helper = module
        .functions
        .iter()
        .position(|f| {
            matches!(&f.return_type, Type::Tuple(fields) if fields.len()==2)
                && f.storage_life
                    .as_ref()
                    .is_some_and(|life| life.fixed_binder_count == 2)
        })
        .unwrap();
    for attack in 0..4 {
        let mut changed = module.clone();
        match attack {
            0 => {
                changed.functions[helper]
                    .storage_life
                    .as_mut()
                    .unwrap()
                    .classes
                    .pop();
            }
            1 => {
                let life = changed.functions[helper].storage_life.as_mut().unwrap();
                let first = life
                    .classes
                    .iter()
                    .find(|row| row.position.base == StorageBase::Result)
                    .unwrap()
                    .binder;
                let last = life
                    .classes
                    .iter_mut()
                    .rev()
                    .find(|row| row.position.base == StorageBase::Result)
                    .unwrap();
                assert_ne!(last.binder, first);
                last.binder = first;
            }
            2 => {
                let fields = changed
                    .functions
                    .iter_mut()
                    .flat_map(|f| &mut f.blocks)
                    .flat_map(|b| &mut b.instructions)
                    .find_map(|i| {
                        if let zil_ir::InstructionKind::MakeAggregate { fields } = &mut i.kind {
                            (fields.len() == 2).then_some(fields)
                        } else {
                            None
                        }
                    })
                    .unwrap();
                assert_ne!(fields[0], fields[1]);
                fields[1] = fields[0];
            }
            3 => {
                assert!(!changed.life_instances.is_empty());
                changed.life_instances.clear();
            }
            _ => unreachable!(),
        }
        let mut forged = plan.clone();
        forged.module_fingerprint = changed.fingerprint();
        for decoded in [false, true] {
            let (raw, candidate) = if decoded {
                let wire =
                    zil_ir::wire::encode_module_candidate(&changed, &forged, &limits).unwrap();
                zil_ir::wire::decode_module_candidate(&wire, &limits).unwrap()
            } else {
                (changed.clone(), forged.clone())
            };
            let errors = zil_verify::verify_module(raw, candidate, &target, &limits).unwrap_err();
            assert!(
                errors
                    .iter()
                    .any(|d| d.category == ResultCategory::SafeReject),
                "attack={attack}, Wire={decoded}: {errors:?}"
            );
        }
    }
}
#[test]
fn shared_tuple_generic_identity_runs_both_backends() {
    let directory =
        std::env::temp_dir().join(format!("zil-qualified-compound-{}", std::process::id()));
    fs::create_dir_all(&directory).unwrap();
    for backend in [NativeBackend::Clang, NativeBackend::LlvmApi] {
        if backend == NativeBackend::LlvmApi && !cfg!(feature = "llvm-api") {
            continue;
        }
        for optimization in [Optimization::O0, Optimization::O2] {
            let output = directory.join(format!("shared-{backend:?}-{optimization:?}"));
            let options = CompileOptions {
                stop_after: Stage::Build,
                output: Some(output.clone()),
                backend,
                optimization,
                ..CompileOptions::default()
            };
            let report = compile_source(&input(SHARED), &options);
            assert_eq!(
                report.status,
                ResultCategory::Success,
                "{}",
                report.to_json()
            );
            assert_eq!(report.verification_runs, 1);
            let codegen = report.native_codegen.as_ref().unwrap();
            assert_eq!(codegen.backend, backend);
            assert!(codegen.fallback.is_none());
            let result = Command::new(&output).output().unwrap();
            assert_eq!(result.status.code(), Some(0), "{result:?}");
            assert!(result.stdout.is_empty() && result.stderr.is_empty());
        }
    }
    fs::remove_dir_all(directory).unwrap();
}
