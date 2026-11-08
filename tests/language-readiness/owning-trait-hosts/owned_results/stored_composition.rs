//! Ordinary owner observations retain their independent lifetime and loans.
use super::*;
use zil_base::{Diagnostic, Phase};
use zil_ir::TypedModule;

const MODEL: &str = include_str!("stored_composition/model.zil");
const LIBRARY: &str = include_str!("stored_composition/library.zil");
const PROTOCOL: &str = include_str!("stored_composition/protocol.zil");
const MAIN: &str = include_str!("stored_composition/main.zil");

fn files() -> [(&'static str, &'static str); 4] {
    [
        ("main", MAIN),
        ("model", MODEL),
        ("protocol", PROTOCOL),
        ("library", LIBRARY),
    ]
}

// Raw attacks additionally use the same provider bodies in one actual source
// module. Only imports and their module qualifiers are removed; the delivered
// positive program retains its original four-module bytes above. The raw
// kernel fixture uses that complete run body as main, so the independently
// tracked scalar-domain bridge is exercised by the four-module tests instead.
fn flat(model: &str) -> String {
    format!("{PROTOCOL}\n{LIBRARY}\n{model}")
        .replace("pub fn run()->i32", "fn main()->i32")
        .replace("import protocol", "")
        .replace("import library", "")
        .replace("import model", "")
        .replace("protocol.", "")
        .replace("library.", "")
        .replace("model.", "")
}

fn kernel_model() -> String {
    let providers = MODEL.split("pub fn run()->i32").next().unwrap();
    format!(
        "{providers}pub fn run()->i32{{let items:Store<Item>=store_new();if !store_reserve(items,1){{return 1}}let bytes=bytes_new();if !bytes_reserve(bytes,1)||!bytes_push(bytes,65u8){{return 2}}store_push(items,Item{{data:give bytes,tag:20}});var cursor=Cursor{{items:give items,index:0}};match library.next_tagged(&mut cursor,7){{Option.None()=>{{return 3}},Option.Some(original)=>{{let copy:Item=original.duplicate();var spare:Item=original.duplicate();let moved:Item=original.duplicate();let consumed=consume(moved);let exclusive=&mut spare;let observed=copy.tag;return i32(observed+exclusive.tag+consumed+original.tag-83)}}}}}}"
    )
}

fn typed(text: &str) -> TypedModule {
    let source = SourceUnit::new(FileId(0), ModuleId(0), "ordinary-package-fields.zil", text);
    let syntax = zil_syntax::parse(&source).into_result().unwrap();
    let core = zil_expand::expand_with_map_limits_target_context(
        &source,
        &syntax,
        &Limits::default(),
        &TargetProfile::host_llvm(),
        zil_expand::source_context::SourceIdentityContext::ClosedSnapshot {
            root: source.module_id,
            aliases: &Default::default(),
            declarations: zil_expand::source_context::DeclarationPlacement::Original,
        },
    )
    .unwrap()
    .core;
    zil_types::check(&core, &TargetProfile::host_llvm(), &Limits::default()).unwrap()
}

fn control(module: &TypedModule) -> zil_ir::CandidatePlan {
    let limits = Limits::default();
    let plan = zil_analyze::analyze(module, &limits).unwrap();
    zil_verify::verify_module(module.clone(), plan.clone(), &module.target, &limits).unwrap();
    let wire = zil_ir::wire::encode_module_candidate(module, &plan, &limits).unwrap();
    zil_verify::verify_module_bytes(&wire, &module.target, &limits).unwrap();
    plan
}

fn rejected(result: Result<impl Sized, Vec<Diagnostic>>, label: &str) -> Vec<Diagnostic> {
    let errors = result.err().unwrap_or_else(|| panic!("{label}: accepted"));
    eprintln!("{label}: {errors:?}");
    assert!(
        errors.iter().all(|d| matches!(
            d.category,
            ResultCategory::SafeReject | ResultCategory::Unsupported
        )),
        "{label}: {errors:?}"
    );
    assert!(
        !errors
            .iter()
            .any(|d| d.code.contains("FINGERPRINT") || d.code.ends_with("CLAIM")),
        "{label}: metadata is not a semantic rejection: {errors:?}"
    );
    errors
}

#[test]
fn ordinary_package_fields_preserve_complete_lending_and_owned_results() {
    let report = compile_project(&project(&files()), &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1);
}

#[test]
fn ordinary_package_fields_execute_the_complete_project() {
    for backend in [NativeBackend::Clang, NativeBackend::LlvmApi] {
        if backend == NativeBackend::LlvmApi && !cfg!(feature = "llvm-api") {
            continue;
        }
        for optimization in [Optimization::O0, Optimization::O2] {
            let dir = std::env::temp_dir().join(format!(
                "zil-owner-observation-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&dir).unwrap();
            let report = compile_project(
                &project(&files()),
                &CompileOptions {
                    backend,
                    optimization,
                    stop_after: Stage::Build,
                    output: Some(dir.join("program")),
                    emit_dir: Some(dir.join("phases")),
                    ..CompileOptions::default()
                },
            );
            fs::write(dir.join("report.json"), report.to_json()).unwrap();
            assert_eq!(
                report.status,
                ResultCategory::Success,
                "{}",
                report.to_json()
            );
            assert_eq!(report.verification_runs, 1);
            let native = report.native_codegen.as_ref().unwrap();
            assert_eq!(native.backend, backend);
            assert!(native.fallback.is_none());
            let output = Command::new(&report.artifacts["executable"])
                .output()
                .unwrap();
            fs::write(dir.join("execution.txt"), format!("{output:?}")).unwrap();
            assert_eq!(output.status.code(), Some(0), "{output:?}");
            assert!(
                output.stdout.is_empty() && output.stderr.is_empty(),
                "{output:?}"
            );
            eprintln!(
                "ordinary Package observations {backend:?}/{optimization:?}: {}",
                dir.display()
            );
        }
    }
}

#[test]
fn ordinary_package_fields_raw_and_wire_keep_actual_shell_child_and_loans() {
    let guarded = kernel_model();
    let module = typed(&flat(&guarded));
    let plan = control(&module);
    for attack in [
        "exclusive-read",
        "owner-field",
        "field-index",
        "result-type",
        "moved-shell",
    ] {
        let mut raw = module.clone();
        let function = raw.functions.iter_mut().find(|f| f.name == "main").unwrap();
        let copies = function
            .locals
            .iter()
            .filter(|l| l.name == "copy")
            .map(|l| l.id)
            .collect::<Vec<_>>();
        let spare = function
            .locals
            .iter()
            .find(|l| l.name == "spare")
            .unwrap()
            .id;
        let moved = function
            .locals
            .iter()
            .find(|l| l.name == "moved")
            .unwrap()
            .id;
        assert!(!copies.is_empty());
        let target = function.blocks.iter_mut().flat_map(|b| &mut b.instructions).find(|i|
            matches!(i.kind, InstructionKind::ReadPackageField { local, .. } if function.locals[local.0 as usize].name == "copy")
        ).unwrap();
        let InstructionKind::ReadPackageField { local, index } = &mut target.kind else {
            unreachable!()
        };
        match attack {
            "exclusive-read" => *local = spare,
            "owner-field" => *index = 0,
            "field-index" => *index = u32::MAX,
            "result-type" => {
                function.values[target.result.unwrap().0 as usize] = zil_ir::Type::Bool
            }
            "moved-shell" => *local = moved,
            _ => unreachable!(),
        }
        let mut claim = plan.clone();
        assert_ne!(claim.module_fingerprint, raw.fingerprint());
        claim.module_fingerprint = raw.fingerprint();
        let a = rejected(
            zil_analyze::analyze(&raw, &Limits::default()),
            &format!("{attack}/Analyze"),
        );
        let wire = zil_ir::wire::encode_module_candidate(&raw, &claim, &Limits::default()).unwrap();
        for errors in [
            rejected(
                zil_verify::verify_module(raw.clone(), claim, &raw.target, &Limits::default()),
                &format!("{attack}/Verify"),
            ),
            rejected(
                zil_verify::verify_module_bytes(&wire, &raw.target, &Limits::default()),
                &format!("{attack}/Wire"),
            ),
        ] {
            if attack == "exclusive-read" {
                assert!(
                    a.iter().any(|d| d.code == "E-ANALYZE-IR"
                        && d.message.contains("actual stored target/parent loan")),
                    "{a:?}"
                );
                assert!(
                    errors
                        .iter()
                        .any(|d| d.code == "E-BORROW-CONFLICT" || d.code == "E-VERIFY-STORED"),
                    "{errors:?}"
                );
            }
        }
    }
}

#[test]
fn ordinary_package_fields_do_not_replace_qualified_leaf_transport() {
    let source = include_str!("../../../fixtures/resource-packages/qualified-record-getter.zil");
    let source = source.replace(
        "if read(&job)==7{return 0}",
        "let child=&job.refs;if *child.pointer==7{return 0}",
    );
    let module = typed(&source);
    let plan = control(&module);
    let mut raw = module.clone();
    let mut changed = false;
    for function in &mut raw.functions {
        let Some(local) = function.locals.iter().find(|l|
            matches!(&l.ty, zil_ir::Type::BorrowedStorage { storage, .. } if matches!(storage.as_ref(), zil_ir::Type::Package { .. })))
            .map(|l| l.id) else { continue; };
        for i in function.blocks.iter_mut().flat_map(|b| &mut b.instructions) {
            if let InstructionKind::AddressField { address, index } = i.kind
                && matches!(&function.values[address.0 as usize], zil_ir::Type::Address { inner, .. }
                    if matches!(inner.as_ref(), zil_ir::Type::BorrowedStorage { storage, .. }
                        if matches!(storage.as_ref(), zil_ir::Type::Package { .. })))
            {
                i.kind = InstructionKind::ReadPackageField { local, index };
                changed = true;
                break;
            }
        }
        if changed {
            break;
        }
    }
    assert!(changed, "qualified field producer is required");
    let mut claim = plan;
    claim.module_fingerprint = raw.fingerprint();
    rejected(
        zil_analyze::analyze(&raw, &Limits::default()),
        "qualified field/Analyze",
    );
    let wire = zil_ir::wire::encode_module_candidate(&raw, &claim, &Limits::default()).unwrap();
    rejected(
        zil_verify::verify_module(raw.clone(), claim, &raw.target, &Limits::default()),
        "qualified field/Verify",
    );
    rejected(
        zil_verify::verify_module_bytes(&wire, &raw.target, &Limits::default()),
        "qualified field/Wire",
    );
}

#[test]
fn ordinary_package_fields_have_exact_fresh_analysis_and_verification_work() {
    let module = typed(&flat(&kernel_model()));
    let plan = control(&module);
    for phase in [Phase::Analyze, Phase::Verify] {
        let run = |n| {
            let limits = Limits {
                max_analysis_steps: n,
                ..Limits::default()
            };
            if phase == Phase::Analyze {
                zil_analyze::analyze(&module, &limits).map(|actual| assert_eq!(actual, plan))
            } else {
                zil_verify::verify_module(module.clone(), plan.clone(), &module.target, &limits)
                    .map(|_| ())
            }
        };
        let (mut low, mut high) = (0, Limits::default().max_analysis_steps);
        run(high).unwrap();
        while low + 1 < high {
            let mid = low + (high - low) / 2;
            match run(mid) {
                Ok(()) => high = mid,
                Err(errors) => {
                    assert!(
                        errors
                            .iter()
                            .all(|d| d.category == ResultCategory::ResourceLimit),
                        "{errors:?}"
                    );
                    low = mid;
                }
            }
        }
        run(high).unwrap();
        assert!(
            run(high - 1)
                .unwrap_err()
                .iter()
                .all(|d| d.category == ResultCategory::ResourceLimit)
        );
        run(high).unwrap();
        eprintln!("ordinary Package Copy observation {phase:?} N={high}; N-1/fresh agree");
    }
}

fn error_place_program() -> String {
    let providers = MODEL.split("pub fn run()->i32").next().unwrap();
    let body = r#"pub fn run()->i32{
 let items:Store<Item>=store_new();if !store_reserve(items,1){return 1}
 let bytes=bytes_new();if !bytes_reserve(bytes,1)||!bytes_push(bytes,65u8){return 2}
 store_push(items,Item{data:give bytes,tag:20});var cursor=Cursor{items:give items,index:0};
 match library.next_tagged(&mut cursor,7){
  Option.None()=>{return 3},
  Option.Some(original)=>{let outcome:protocol.Error!Item=original.try_duplicate(true);
   match outcome{.ok(owner)=>{return 4},.err(error)=>{match error{
    protocol.Error.Allocation()=>{return 5},protocol.Error.Requested()=>{return i32(original.tag-20)}
   }}}
  }
 }
}"#;
    flat(&format!("{providers}{body}"))
}

#[test]
fn ordinary_error_places_keep_real_tag_initialization_and_live_loans() {
    let module = typed(&error_place_program());
    let plan = control(&module);
    let caller = module
        .functions
        .iter()
        .position(|f| f.name == "main")
        .unwrap();
    let (block, borrow, local, pointer) = module.functions[caller]
        .blocks
        .iter()
        .enumerate()
        .find_map(|(block, b)| {
            b.instructions
                .iter()
                .enumerate()
                .find_map(|(index, i)| match &i.kind {
                    InstructionKind::BorrowPlaceLocal {
                        local,
                        path,
                        mutable: false,
                    } if matches!(path.as_slice(), [zil_ir::PlaceProjection::ErrorArm(true)]) => {
                        Some((block, index, *local, i.result.unwrap()))
                    }
                    _ => None,
                })
        })
        .unwrap();
    for attack in [
        "drop-before-read",
        "take-before-read",
        "wrong-tag",
        "missing-initialization",
    ] {
        let mut raw = module.clone();
        let function = &mut raw.functions[caller];
        match attack {
            "drop-before-read" => {
                let instructions = &mut function.blocks[block].instructions;
                let read = instructions.iter().position(|i| matches!(i.kind, InstructionKind::ReadAddress { address } if address==pointer)).unwrap();
                let drop = instructions.iter().position(|i| matches!(i.kind, InstructionKind::DropErrorUnion { local: l } if l==local)).unwrap();
                assert!(borrow < read && read < drop);
                instructions.swap(read, drop);
            }
            "take-before-read" => {
                let instructions = &mut function.blocks[block].instructions;
                let read = instructions.iter().position(|i| matches!(i.kind, InstructionKind::ReadAddress { address } if address==pointer)).unwrap();
                let ty = function.values[instructions[read].result.unwrap().0 as usize].clone();
                let later = instructions
                    .iter()
                    .enumerate()
                    .skip(read + 1)
                    .find_map(|(index, i)| {
                        if matches!(i.kind, InstructionKind::Load(_))
                            && i.result
                                .is_some_and(|v| function.values[v.0 as usize] == ty)
                        {
                            Some(index)
                        } else {
                            None
                        }
                    })
                    .unwrap();
                instructions[read].kind = InstructionKind::TakeVariantField {
                    local,
                    path: vec![zil_ir::PlaceProjection::ErrorArm(true)],
                };
                instructions[later].kind = InstructionKind::ReadAddress { address: pointer };
            }
            "wrong-tag" => {
                let tags = function.blocks.iter().flat_map(|b| &b.instructions).filter_map(|i|
                    if matches!(&i.kind,InstructionKind::PlaceTagLocal {local:l,path} if *l==local && path.is_empty()) {i.result} else {None}
                ).collect::<Vec<_>>();
                let conditions = function
                    .blocks
                    .iter()
                    .filter_map(|b| {
                        if let zil_ir::Terminator::Branch { condition, .. } = b.terminator {
                            Some(condition)
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<_>>();
                let mut literals = function
                    .blocks
                    .iter()
                    .flat_map(|b| &b.instructions)
                    .filter_map(|i| {
                        if !i.result.is_some_and(|v| conditions.contains(&v)) {
                            return None;
                        }
                        if let InstructionKind::Binary { left, right, .. } = i.kind {
                            if tags.contains(&left) {
                                return Some(right);
                            }
                            if tags.contains(&right) {
                                return Some(left);
                            }
                        }
                        None
                    })
                    .collect::<Vec<_>>();
                literals.sort_unstable();
                literals.dedup();
                let mut changed = 0;
                for i in function.blocks.iter_mut().flat_map(|b| &mut b.instructions) {
                    if i.result.is_some_and(|v| literals.contains(&v))
                        && let InstructionKind::Constant(zil_ir::Constant::I32(value)) = &mut i.kind
                        && (*value == 0 || *value == 1)
                    {
                        *value = 1 - *value;
                        changed += 1;
                    }
                }
                assert_eq!(
                    changed, 2,
                    "both real arm guards must change; making just one case unreachable is legal"
                );
            }
            "missing-initialization" => {
                let mut changed = 0;
                for block in &mut function.blocks {
                    block.instructions.retain(|i| {
                        let remove =
                            matches!(i.kind, InstructionKind::Store { local:l, .. } if l==local);
                        changed += usize::from(remove);
                        !remove
                    });
                }
                assert!(changed > 0);
            }
            _ => unreachable!(),
        }
        let mut claim = plan.clone();
        if attack == "missing-initialization" {
            // Recompute exactly the removed incoming initialization fact;
            // stale public claims cannot count as the safety rejection.
            for block in &mut claim.functions[caller].blocks {
                block.initialized_in.retain(|l| *l != local);
                block.initialized_out.retain(|l| *l != local);
            }
        }
        claim.module_fingerprint = raw.fingerprint();
        let a = rejected(
            zil_analyze::analyze(&raw, &Limits::default()),
            &format!("{attack}/Analyze"),
        );
        let wire = zil_ir::wire::encode_module_candidate(&raw, &claim, &Limits::default()).unwrap();
        for errors in [
            rejected(
                zil_verify::verify_module(raw.clone(), claim, &raw.target, &Limits::default()),
                &format!("{attack}/Verify"),
            ),
            rejected(
                zil_verify::verify_module_bytes(&wire, &raw.target, &Limits::default()),
                &format!("{attack}/Wire"),
            ),
        ] {
            if attack == "drop-before-read" || attack == "take-before-read" {
                assert!(
                    a.iter()
                        .any(|d| d.message.contains("actual stored target/parent loan")),
                    "{a:?}"
                );
                assert!(
                    errors
                        .iter()
                        .any(|d| d.message.contains("live incompatible loan")),
                    "{errors:?}"
                );
            }
        }
    }
}

fn pipeline_files() -> [(&'static str, &'static str); 4] {
    [
        ("main", include_str!("../owned_pipeline/main.zil")),
        ("model", include_str!("../owned_pipeline/model.zil")),
        ("protocol", include_str!("../owned_pipeline/protocol.zil")),
        ("library", include_str!("../owned_pipeline/library.zil")),
    ]
}

#[test]
fn ordinary_owned_pipeline_keeps_callbacks_and_error_paths() {
    let files = pipeline_files();
    let dir = std::env::temp_dir().join(format!(
        "zil-owned-callback-pipeline-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&dir).unwrap();
    let types = compile_project(
        &project(&files),
        &CompileOptions {
            stop_after: Stage::Types,
            emit_dir: Some(dir.join("types")),
            ..CompileOptions::default()
        },
    );
    fs::write(dir.join("types.json"), types.to_json()).unwrap();
    assert_eq!(types.status, ResultCategory::Success, "{}", types.to_json());
    let report = compile_project(&project(&files), &CompileOptions::default());
    fs::write(dir.join("report.json"), report.to_json()).unwrap();
    eprintln!("complete owning callback pipeline: {}", dir.display());
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1);
}

#[test]
fn ordinary_package_fields_complete_project_has_exact_fresh_analysis_work() {
    let input = project(&files());
    let run = |steps| {
        compile_project(
            &input,
            &CompileOptions {
                stop_after: Stage::Analyze,
                limits: Limits {
                    max_analysis_steps: steps,
                    ..Limits::default()
                },
                ..CompileOptions::default()
            },
        )
    };
    let (mut low, mut high) = (0, Limits::default().max_analysis_steps);
    let initial = run(high);
    assert_eq!(
        initial.status,
        ResultCategory::Success,
        "{}",
        initial.to_json()
    );
    let fingerprint = initial.input_fingerprint;
    while low + 1 < high {
        let middle = low + (high - low) / 2;
        let report = run(middle);
        assert_eq!(report.input_fingerprint, fingerprint);
        match report.status {
            ResultCategory::Success => high = middle,
            ResultCategory::ResourceLimit => low = middle,
            _ => panic!("{}", report.to_json()),
        }
    }
    for steps in [high, high - 1, high] {
        let report = run(steps);
        assert_eq!(report.input_fingerprint, fingerprint);
        assert_eq!(
            report.status,
            if steps == high {
                ResultCategory::Success
            } else {
                ResultCategory::ResourceLimit
            },
            "{}",
            report.to_json()
        );
        if steps != high {
            assert_eq!(report.phase, Phase::Analyze, "{}", report.to_json());
        }
    }
    eprintln!("ordinary Package four-module analysis N={high}; N-1/fresh agree");
}

#[test]
#[cfg(target_os = "linux")]
fn ordinary_owned_pipeline_executes_callbacks_and_allocation_cleanup_paths() {
    owning_trait_native_cleanup(
        &pipeline_files(),
        &[
            (None, 0, b"11:11:11\n"),
            (Some("1"), 1, b"1:0:0\n"),
            (Some("2"), 2, b"2:1:1\n"),
            (Some("3"), 3, b"3:2:2\n"),
            (Some("4"), 10, b"4:3:3\n"),
            (Some("5"), 14, b"5:4:4\n"),
            (Some("6"), 15, b"6:5:5\n"),
            (Some("7"), 30, b"7:6:6\n"),
            (Some("8"), 20, b"8:7:7\n"),
            (Some("9"), 24, b"9:8:8\n"),
            (Some("10"), 25, b"10:9:9\n"),
            (Some("11"), 40, b"11:10:10\n"),
        ],
    );
}

#[test]
fn ordinary_error_places_have_exact_fresh_independent_resource_work() {
    let module = typed(&error_place_program());
    let plan = control(&module);
    for phase in [Phase::Analyze, Phase::Verify] {
        let run = |steps| {
            let limits = Limits {
                max_analysis_steps: steps,
                ..Limits::default()
            };
            if phase == Phase::Analyze {
                zil_analyze::analyze(&module, &limits).map(|actual| assert_eq!(actual, plan))
            } else {
                zil_verify::verify_module(module.clone(), plan.clone(), &module.target, &limits)
                    .map(|_| ())
            }
        };
        let (mut low, mut high) = (0, Limits::default().max_analysis_steps);
        run(high).unwrap();
        while low + 1 < high {
            let middle = low + (high - low) / 2;
            match run(middle) {
                Ok(()) => high = middle,
                Err(errors) => {
                    assert!(
                        errors
                            .iter()
                            .all(|d| d.category == ResultCategory::ResourceLimit)
                    );
                    low = middle;
                }
            }
        }
        run(high).unwrap();
        assert!(
            run(high - 1)
                .unwrap_err()
                .iter()
                .all(|d| d.phase == phase && d.category == ResultCategory::ResourceLimit)
        );
        run(high).unwrap();
        eprintln!("ordinary Error/Option resource {phase:?} N={high}; N-1/fresh agree");
    }
}

#[test]
fn ordinary_error_places_execute_the_original_byte_and_package_consumers() {
    let source = error_place_program();
    for backend in [NativeBackend::Clang, NativeBackend::LlvmApi] {
        if backend == NativeBackend::LlvmApi && !cfg!(feature = "llvm-api") {
            continue;
        }
        for optimization in [Optimization::O0, Optimization::O2] {
            let dir = std::env::temp_dir().join(format!(
                "zil-error-place-consumer-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&dir).unwrap();
            fs::write(dir.join("source.zil"), &source).unwrap();
            let report = zil_driver::compile_source(
                &SourceUnit::new(FileId(0), ModuleId(0), "ordinary-error-place.zil", &source),
                &CompileOptions {
                    backend,
                    optimization,
                    stop_after: Stage::Build,
                    output: Some(dir.join("program")),
                    emit_dir: Some(dir.join("phases")),
                    ..CompileOptions::default()
                },
            );
            fs::write(dir.join("report.json"), report.to_json()).unwrap();
            assert_eq!(
                report.status,
                ResultCategory::Success,
                "{}",
                report.to_json()
            );
            assert_eq!(report.verification_runs, 1);
            let native = report.native_codegen.as_ref().unwrap();
            assert_eq!(native.backend, backend);
            assert!(native.fallback.is_none());
            let output = Command::new(&report.artifacts["executable"])
                .output()
                .unwrap();
            fs::write(dir.join("execution.txt"), format!("{output:?}")).unwrap();
            assert_eq!(output.status.code(), Some(0), "{output:?}");
            assert!(
                output.stdout.is_empty() && output.stderr.is_empty(),
                "{output:?}"
            );
        }
    }
}
