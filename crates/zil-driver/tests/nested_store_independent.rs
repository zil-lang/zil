//! Ordinary finite nested owners through the complete Source/independent
//! raw+Wire/native pipeline. Analyze contributes an untrusted public plan only.
use std::{
    fs,
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_base::{FileId, Fingerprint, Limits, ModuleId, ResultCategory, SourceUnit, TargetProfile};
use zil_driver::{CompileOptions, NativeBackend, Optimization, Stage, compile_path};
static NEXT: AtomicUsize = AtomicUsize::new(0);

fn typed(source: &str) -> zil_ir::TypedModule {
    let sources = [
        ("main", source),
        ("iter", include_str!("../../../lib/std/iter.zil")),
        ("store", include_str!("../../../lib/std/store.zil")),
    ];
    let units: Vec<_> = sources
        .iter()
        .enumerate()
        .map(|(id, (name, text))| {
            SourceUnit::new(
                FileId(id as u32),
                ModuleId(id as u32),
                format!("{name}.zil"),
                *text,
            )
        })
        .collect();
    let parsed: Vec<_> = units
        .iter()
        .map(|unit| zil_syntax::parse(unit).into_result().unwrap())
        .collect();
    let syntax: Vec<_> = parsed.iter().collect();
    let imports: Vec<Vec<_>> = syntax
        .iter()
        .map(|module| {
            module
                .items
                .iter()
                .filter_map(|item| {
                    let zil_syntax::ast::ItemKind::Import(import) = &item.kind else {
                        return None;
                    };
                    let name = import.path.last().unwrap();
                    Some(zil_ir::CoreImport {
                        alias: import.alias.clone().unwrap_or_else(|| name.clone()),
                        module: ModuleId(
                            sources.iter().position(|(key, _)| key == name).unwrap() as u32
                        ),
                        span: item.span,
                    })
                })
                .collect()
        })
        .collect();
    let target = TargetProfile::host_llvm();
    let limits = Limits::default();
    let modules =
        zil_expand::expand_program_with_limits_target(&units, &syntax, &imports, &limits, &target)
            .unwrap()
            .into_iter()
            .map(|(module, _)| module)
            .collect();
    zil_types::check_program(
        &zil_ir::CoreProgram {
            root: ModuleId(0),
            source_fingerprint: Fingerprint::of(source.as_bytes()),
            modules,
        },
        &target,
        &limits,
    )
    .unwrap()
    .module
}

fn pipeline(source: &str, name: &str, trace: &[u8]) {
    let module = typed(source);
    let limits = Limits::default();
    let plan = zil_analyze::analyze(&module, &limits).unwrap();
    zil_verify::verify_module(module.clone(), plan.clone(), &module.target, &limits).unwrap();
    let wire = zil_ir::wire::encode_module_candidate(&module, &plan, &limits).unwrap();
    zil_verify::verify_module_bytes(&wire, &module.target, &limits).unwrap();
    for backend in [NativeBackend::Clang, NativeBackend::LlvmApi] {
        if backend == NativeBackend::LlvmApi && !cfg!(feature = "llvm-api") {
            continue;
        }
        for optimization in [Optimization::O0, Optimization::O2] {
            let base = std::env::var_os("ZIL_TEST_REPORT_DIR")
                .map(std::path::PathBuf::from)
                .unwrap_or_else(std::env::temp_dir);
            let root = base.join(format!(
                "nested-store-{name}-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&root).unwrap();
            let input = root.join("main.zil");
            fs::write(&input, source).unwrap();
            let report = compile_path(
                &input,
                &CompileOptions {
                    stop_after: Stage::Build,
                    backend,
                    optimization,
                    output: Some(root.join("program")),
                    emit_dir: Some(root.join("phases")),
                    ..CompileOptions::default()
                },
            );
            fs::write(root.join("compile.json"), report.to_json()).unwrap();
            assert_eq!(
                report.status,
                ResultCategory::Success,
                "{}",
                report.to_json()
            );
            let machine = report.native_codegen.as_ref().unwrap();
            assert_eq!(machine.backend, backend);
            assert!(machine.fallback.is_none());
            if backend == NativeBackend::LlvmApi {
                assert!(!machine.object_reports.is_empty());
                assert!(
                    machine
                        .object_reports
                        .iter()
                        .all(|r| r.contains("\"input_kind\":\"builder\"")
                            && r.contains("\"verify_before_after\":true"))
                );
            }
            let output = Command::new(&report.artifacts["executable"])
                .output()
                .unwrap();
            fs::write(root.join("stdout"), &output.stdout).unwrap();
            fs::write(root.join("stderr"), &output.stderr).unwrap();
            assert_eq!(
                output.status.code(),
                Some(0),
                "{name}/{backend:?}/{optimization:?}: {output:?}"
            );
            assert_eq!(output.stdout, trace, "{name}/{backend:?}/{optimization:?}");
            assert!(output.stderr.is_empty());
        }
    }
}
#[test]
fn complete_package_manager_chain() {
    pipeline(
        include_str!("../../../tests/fixtures/nested-store/packages.zil"),
        "packages",
        b"AR",
    );
}
#[test]
fn complete_drain_elements() {
    pipeline(
        include_str!("../../../tests/fixtures/nested-store/drains.zil"),
        "drains",
        b"AB",
    );
}
#[test]
fn complete_direct_store_owners() {
    pipeline(
        include_str!("../../../tests/fixtures/nested-store/direct-store.zil"),
        "direct-store",
        b"A",
    );
}
#[test]
fn complete_direct_user_resource_owners() {
    pipeline(
        include_str!("../../../tests/fixtures/nested-store/direct-ticket.zil"),
        "direct-ticket",
        b"BA",
    );
}
#[test]
fn complete_direct_byte_buffer_owners() {
    pipeline(
        include_str!("../../../tests/fixtures/nested-store/direct-byte-buffer.zil"),
        "direct-byte-buffer",
        b"",
    );
}

#[test]
fn complete_saved_some_keeps_both_heap_layers_through_helpers() {
    pipeline(
        include_str!("../../../tests/fixtures/nested-store/lending.zil"),
        "lending",
        b"AR",
    );
}

#[test]
fn ordinary_store_descriptions_share_returned_environment_source_lane() {
    pipeline(
        include_str!("../../../tests/fixtures/nested-store/ordinary-environment.zil"),
        "ordinary-environment",
        b"A",
    );
}

#[test]
fn saved_some_rejects_actual_manager_mutation_and_retirement_raw_wire() {
    use zil_ir::*;
    let source = include_str!("../../../tests/fixtures/nested-store/lending.zil")
        .replace("catalog:&'a Catalog", "catalog:&'a mut Catalog")
        .replace("let catalog=", "var catalog=")
        .replace("forward(&catalog,0)", "forward(&mut catalog,0)");
    let module = typed(&source);
    let limits = Limits::default();
    let plan = zil_analyze::analyze(&module, &limits).unwrap();
    zil_verify::verify_module(module.clone(), plan.clone(), &module.target, &limits).unwrap();
    let (function, block, at, argument) = module
        .functions
        .iter()
        .enumerate()
        .find_map(|(fi, f)| {
            f.blocks.iter().enumerate().find_map(|(bi, block)| {
                block.instructions.iter().enumerate().find_map(|(ii, i)| {
                    let InstructionKind::Call {
                        function,
                        arguments,
                    } = &i.kind
                    else {
                        return None;
                    };
                    (module.functions[function.0 as usize]
                        .name
                        .contains("forward"))
                    .then_some((fi, bi, ii, arguments[0]))
                })
            })
        })
        .unwrap();
    let manager = plan.functions[function].holds[argument.0 as usize]
        .roots
        .clone();
    let [HoldRoot::Local(local)] = manager.as_slice() else {
        panic!("{manager:?}")
    };
    for retire in [false, true] {
        let mut changed = module.clone();
        let mut claim = plan.clone();
        let f = &mut changed.functions[function];
        let span = f.span;
        let mut injected = Vec::new();
        if retire {
            // Move the one real cleanup earlier; retain its later StorageDead.
            // An extra duplicate drop would hide the actual live-child check.
            let mut removed = 0;
            for block in &mut f.blocks {
                block.instructions.retain(|i| {
                    let selected=matches!(i.kind,InstructionKind::DropPackage {local:target} if target==*local);
                    removed+=usize::from(selected);
                    !selected
                });
            }
            assert!(removed > 0);
            for block in &mut claim.functions[function].blocks {
                block.initialized_in.retain(|slot| slot != local);
                block.initialized_out.retain(|slot| slot != local);
            }
            injected.push(Instruction {
                result: None,
                kind: InstructionKind::DropPackage { local: *local },
                span,
            });
        } else {
            let Type::Address { inner, .. } = &f.values[argument.0 as usize] else {
                panic!()
            };
            let Type::Package { fields, .. } = inner.as_ref() else {
                panic!()
            };
            let header = fields[0].1.clone();
            let Type::Managed(payload) = &header else {
                panic!()
            };
            let payload = payload.clone();
            for (ty, kind, roots) in [
                (
                    Type::Address {
                        mutable: true,
                        inner: Box::new(header),
                    },
                    InstructionKind::AddressField {
                        address: argument,
                        index: 0,
                    },
                    manager.clone(),
                ),
                (
                    Type::Desc(payload),
                    InstructionKind::DescribeAddress {
                        address: ValueId(f.values.len() as u32),
                    },
                    manager.clone(),
                ),
                (
                    Type::I64,
                    InstructionKind::Constant(Constant::I64(4)),
                    Vec::new(),
                ),
            ] {
                let value = ValueId(f.values.len() as u32);
                f.values.push(ty);
                claim.functions[function]
                    .holds
                    .push(ValueHold { value, roots });
                injected.push(Instruction {
                    result: Some(value),
                    kind,
                    span,
                });
            }
            let capacity = ValueId(f.values.len() as u32 - 1);
            let store = ValueId(f.values.len() as u32 - 2);
            let value = ValueId(f.values.len() as u32);
            f.values.push(Type::Bool);
            claim.functions[function].holds.push(ValueHold {
                value,
                roots: Vec::new(),
            });
            injected.push(Instruction {
                result: Some(value),
                kind: InstructionKind::OwnedStoreReserve { store, capacity },
                span,
            });
        }
        f.blocks[block]
            .instructions
            .splice(at + 1..at + 1, injected);
        claim.module_fingerprint = changed.fingerprint();
        let wire = zil_ir::wire::encode_module_candidate(&changed, &claim, &limits).unwrap();
        for result in [
            zil_verify::verify_module(changed.clone(), claim, &changed.target, &limits),
            zil_verify::verify_module_bytes(&wire, &changed.target, &limits),
        ] {
            let errors = result.unwrap_err();
            assert!(
                errors.iter().all(|error| {
                    error.code
                        == if retire {
                            "E-BORROW-CONFLICT"
                        } else {
                            "E-VERIFY-STORED"
                        }
                }),
                "{errors:?}"
            );
            assert!(
                errors
                    .iter()
                    .all(|error| error.category == ResultCategory::SafeReject),
                "{errors:?}"
            );
        }
    }
}

#[test]
fn identical_catalog_inputs_do_not_substitute_actual_manager_roots_raw_wire() {
    use zil_ir::*;
    let source=include_str!("../../../tests/fixtures/nested-store/lending.zil").replace(
        "fn main()->i32{",
        "fn choose<life a>(first:&'a Catalog,second:&'a Catalog)->Option<&'a Item>{return forward(first,0)}\nfn main()->i32{",
    );
    let module = typed(&source);
    let limits = Limits::default();
    let mut plan = zil_analyze::analyze(&module, &limits).unwrap();
    zil_verify::verify_module(module.clone(), plan.clone(), &module.target, &limits).unwrap();
    let function = module
        .functions
        .iter()
        .position(|f| f.name.contains("choose"))
        .unwrap();
    let f = &module.functions[function];
    assert_eq!(
        f.locals[f.parameters[0].0 as usize].ty,
        f.locals[f.parameters[1].0 as usize].ty
    );
    let mut forged = 0;
    for hold in &mut plan.functions[function].holds {
        for root in &mut hold.roots {
            if *root == HoldRoot::Parameter(0) {
                *root = HoldRoot::Parameter(1);
                forged += 1;
            }
        }
    }
    assert!(forged > 0);
    let wire = zil_ir::wire::encode_module_candidate(&module, &plan, &limits).unwrap();
    for result in [
        zil_verify::verify_module(module.clone(), plan, &module.target, &limits),
        zil_verify::verify_module_bytes(&wire, &module.target, &limits),
    ] {
        let errors = result.unwrap_err();
        assert!(
            errors
                .iter()
                .all(|e| e.category == ResultCategory::SafeReject),
            "{errors:?}"
        );
        assert!(
            errors
                .iter()
                .any(|e| e.code == "E-VERIFY-STORED" || e.code == "E-VERIFY-HOLD"),
            "{errors:?}"
        );
    }
}

#[test]
fn actual_owner_replacement_preserves_unrelated_fields_and_borrowed_environment() {
    pipeline(
        include_str!("../../../tests/fixtures/nested-store/replacement-borrowed-environment.zil"),
        "replacement-borrowed-environment",
        b"AB",
    );
}

#[test]
fn whole_package_replacement_keeps_real_cleanup_and_current_fields() {
    let source =
        include_str!("../../../tests/fixtures/nested-store/replacement-borrowed-environment.zil")
            .replace(
                "return packet.n+read()-84",
                "packet=Packet{owner:Token{code:67},n:43}\n return packet.n+read()-85",
            );
    pipeline(&source, "replacement-whole-package", b"ABC");
}

#[test]
fn owning_error_union_replacement_keeps_both_actual_arm_responsibilities() {
    let source = "extern \"C\" fn putchar(n:i32)->i32;\nresource Token{code:i32}\nimpl Drop for Token{fn drop(self:Self){unsafe{putchar(.code)}}}\npackage Packet{manages owner:Token;n:i32}\nfn make(value:&i32)->impl Fn()->i32{return move()=>*value}\nfn build(code:i32)->Packet!i32{return Packet{owner:Token{code:code},n:42}}\nfn main()->i32{let expected=42;let read=make(&expected);var result=build(65);result=build(66);match result{.err(packet)=>{return packet.n+read()-84},.ok(_)=>{return 1}}}";
    pipeline(source, "replacement-owned-union", b"AB");
}
