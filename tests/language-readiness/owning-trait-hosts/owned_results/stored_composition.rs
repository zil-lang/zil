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

#[test]
fn ordinary_owned_pipeline_keeps_callbacks_and_error_paths() {
    let files = [
        ("main", include_str!("../owned_pipeline/main.zil")),
        ("model", include_str!("../owned_pipeline/model.zil")),
        ("protocol", include_str!("../owned_pipeline/protocol.zil")),
        ("library", include_str!("../owned_pipeline/library.zil")),
    ];
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
fn ordinary_owned_pipeline_borrowed_inputs_reach_analysis() {
    let files = [
        ("main", include_str!("../owned_pipeline/main.zil")),
        ("model", include_str!("../owned_pipeline/model.zil")),
        ("protocol", include_str!("../owned_pipeline/protocol.zil")),
        ("library", include_str!("../owned_pipeline/library.zil")),
    ];
    let report = compile_project(
        &project(&files),
        &CompileOptions {
            stop_after: Stage::Analyze,
            ..CompileOptions::default()
        },
    );
    eprintln!("complete owning callback analysis: {}", report.to_json());
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.phase, Phase::Analyze);
    assert_eq!(report.verification_runs, 0);
}

fn borrowed_callback_kernel_model() -> String {
    let body = kernel_model()
        .replace("let copy:Item", "var copy:Item")
        .replace(
            "let exclusive=&mut spare;let observed=copy.tag;",
            "let shared=&copy;var calls:i64=0;var handler=move(item:&mut Item)->Reply=>{calls+=1;return Reply{data:bytes_new(),tag:item.tag+calls}};let reply=apply(&mut handler,&mut spare);let observed=shared.tag;",
        )
        .replace("exclusive.tag", "spare.tag");
    format!(
        "fn apply<F:FnMut(&mut Item)->Reply>(handler:&mut F,item:&mut Item)->Reply{{return handler(item)}}{body}"
    )
}

#[test]
fn stored_borrowed_callbacks_keep_environment_modes_and_actual_loans() {
    use zil_ir::{CallableKind, HoldRoot, InstructionKind, LocalId, Type};
    let source = flat(&borrowed_callback_kernel_model());
    let module = typed(&source);
    let dir =
        std::env::temp_dir().join(format!("zil-stored-callback-kernel-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("source.zil"), &source).unwrap();
    fs::write(dir.join("typed.txt"), format!("{module:#?}")).unwrap();
    eprintln!("stored callback raw control: {}", dir.display());
    let plan = control(&module);
    for attack in [
        "fn-write",
        "captured-stored-source",
        "retarget-input-loan",
        "fabricated-empty-contract",
    ] {
        let mut raw = module.clone();
        if attack == "fabricated-empty-contract" {
            let empty = zil_ir::StorageLifeContract {
                fixed_binder_count: 0,
                invocation_binder_count: 0,
                bindings: vec![],
                classes: vec![],
            };
            let entry = raw.closures[0].function.0 as usize;
            raw.closures[0].storage_life = Some(empty.clone());
            raw.functions[entry].storage_life = Some(empty);
        }
        let function = raw.functions.iter_mut().find(|f| f.name == "main").unwrap();
        let function_id = function.id.0 as usize;
        let copy = LocalId(
            function
                .locals
                .iter()
                .position(|local| local.name == "copy")
                .unwrap() as u32,
        );
        let spare = LocalId(
            function
                .locals
                .iter()
                .position(|local| local.name == "spare")
                .unwrap() as u32,
        );
        if attack == "fn-write" {
            raw.closures[0].kind = CallableKind::Fn;
        } else if attack == "captured-stored-source" {
            let shared = function
                .blocks
                .iter()
                .flat_map(|b| &b.instructions)
                .find_map(|i| {
                    matches!(i.kind, InstructionKind::BorrowLocal { local } if local == copy)
                        .then_some(i.result)
                        .flatten()
                })
                .unwrap();
            assert!(
                matches!(&function.values[shared.0 as usize], Type::Address { mutable: false, inner } if matches!(inner.as_ref(), Type::Package { .. }))
            );
            raw.closures[0]
                .captures
                .push(function.values[shared.0 as usize].clone());
            let mut changed = 0;
            for i in function.blocks.iter_mut().flat_map(|b| &mut b.instructions) {
                if let InstructionKind::MakeClosure { captures, .. } = &mut i.kind {
                    captures.push(shared);
                    changed += 1;
                }
            }
            assert_eq!(changed, 1);
        } else if attack == "retarget-input-loan" {
            let mut changed = 0;
            for i in function.blocks.iter_mut().flat_map(|b| &mut b.instructions) {
                if let InstructionKind::BorrowLocalMut { local } = &mut i.kind
                    && *local == spare
                {
                    *local = copy;
                    changed += 1;
                }
            }
            assert_eq!(changed, 1);
        }
        let analyzed = rejected(zil_analyze::analyze(&raw, &Limits::default()), attack);
        if attack == "retarget-input-loan" {
            assert!(
                analyzed.iter().any(|d| d.message.contains(
                    "simultaneously live access overlaps an actual stored target/parent loan"
                )),
                "{analyzed:?}"
            );
        }
        let mut claimed = plan.clone();
        if attack == "retarget-input-loan" {
            // Retarget the candidate roots truthfully, so independent Verify
            // must reject the actual loan rather than an old Hold claim.
            for row in &mut claimed.functions[function_id].holds {
                for root in &mut row.roots {
                    if *root == HoldRoot::Local(spare) {
                        *root = HoldRoot::Local(copy);
                    }
                }
                row.roots.sort_unstable();
                row.roots.dedup();
            }
        }
        claimed.module_fingerprint = raw.fingerprint();
        let bytes =
            zil_ir::wire::encode_module_candidate(&raw, &claimed, &Limits::default()).unwrap();
        let verified = rejected(
            zil_verify::verify_module(raw.clone(), claimed, &raw.target, &Limits::default()),
            attack,
        );
        let decoded = rejected(
            zil_verify::verify_module_bytes(&bytes, &raw.target, &Limits::default()),
            attack,
        );
        if attack == "retarget-input-loan" {
            for errors in [&verified, &decoded] {
                assert!(
                    errors.iter().any(|d| d.code == "E-VERIFY-STORED"
                        && d.message
                            .contains("actual target intersects a live incompatible loan")),
                    "{errors:?}"
                );
            }
        }
    }
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
