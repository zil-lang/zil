//! Dynamic selection conservatively retains every actual fixed-array source.
use super::*;
const WINDOW: &str = include_str!("dynamic-shared-address-array.zil");
const CLEANUP_SNAPSHOT: &str = "extern \"C\" fn putchar(c:i32)->i32;resource Mark{code:i32}impl Drop for Mark{fn drop(self:Self){unsafe{putchar(.code)}}}package Job<life a>{manages mark:Mark;items:[&'a i32;2]}fn snapshot<life a>(first:&'a i32,second:&'a i32)->[&'a i32;2]{let job=Job::<'a>{mark:Mark{code:82},items:[first,second]};let window=&job;return window.items}fn main()->i32{let first=7;let second=9;let items=snapshot(&first,&second);unsafe{putchar(86)}let index=1;return *items[index]-9}";
const MIXED_COPY: &str = "struct Refs<life a>{pointer:&'a i32}fn make<life a>(pointer:&'a i32)->Refs<'a>{return Refs::<'a>{pointer:pointer}}fn observe(pointer:&i32)->i32{return *pointer}fn read<life a,life b>(left:Refs<'a>,right:Refs<'b>,index:i32)->i32{let items=[left.pointer,right.pointer];let pointer=items[index];let copied=pointer;let reborrowed=&*copied;return observe(reborrowed)}fn main()->i32{var first=7;var second=9;let value=read(make(&first),make(&second),1);first=11;second=12;return value-9}";

fn verify_source(label: &str, program: &str) {
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    let report = compile_source(
        &source(label, program),
        &CompileOptions {
            stop_after: Stage::Verify,
            ..CompileOptions::default()
        },
    );
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{label}: {}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1);
    let raw = typed_source(label, program, &target, &limits);
    let plan = zil_analyze::analyze(&raw, &limits).unwrap();
    zil_verify::verify_module(raw.clone(), plan.clone(), &target, &limits).unwrap();
    let wire = zil_ir::wire::encode_module_candidate(&raw, &plan, &limits).unwrap();
    zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap();
    println!("{label}: Source/raw/Wire Success");
}

#[test]
fn dynamic_indices_keep_all_actual_element_sources() {
    let bare = "fn main()->i32{var first=7;var second=9;let items=[&first,&second];let index=1;let pointer=items[index];return *pointer-9}";
    let record = "struct Batch<life a>{items:[&'a i32;2]}fn make<life a>(first:&'a i32,second:&'a i32)->Batch<'a>{return Batch::<'a>{items:[first,second]}}fn main()->i32{let first=7;let second=9;let batch=make(&first,&second);let index=1;return *batch.items[index]-9}";
    let package = "package Job<life a>{manages bytes:ByteBuffer;items:[&'a i32;2]}fn make<life a>(first:&'a i32,second:&'a i32)->Job<'a>{var bytes=bytes_new();return Job::<'a>{bytes:give bytes,items:[first,second]}}fn main()->i32{let first=7;let second=9;let job=make(&first,&second);let window=&job;let index=1;return *window.items[index]-9}";
    for (label, program) in [
        ("bare", bare),
        ("window-helper", WINDOW),
        ("record", record),
        ("owning-package", package),
        ("snapshot after manager cleanup", CLEANUP_SNAPSHOT),
    ] {
        verify_source(label, program);
    }
}

#[test]
fn distinct_rigid_element_uppers_are_valid_local_dynamic_candidates() {
    let prefix = "struct Refs<life a>{pointer:&'a i32}fn read<life a,life b>(left:Refs<'a>,right:Refs<'b>,index:i32)->i32{let items=[left.pointer,right.pointer];return *items[";
    for index in ["0", "1", "index"] {
        let program = format!("{prefix}{index}]}}fn main()->i32{{return 0}}");
        let report = compile_source(
            &source("rigid-upper-qualification", &program),
            &CompileOptions {
                stop_after: Stage::Verify,
                ..CompileOptions::default()
            },
        );
        println!("rich distinct rigid index={index}: {}", report.to_json());
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1);
        verify_source("distinct rigid upper candidates", &program);
    }
}

#[test]
fn dynamic_selection_keeps_both_sources_live_until_its_last_use() {
    for target in ["first", "second"] {
        let program = format!(
            "fn main()->i32{{var first=7;var second=9;let items=[&first,&second];let index=1;let pointer=items[index];{target}=11;return *pointer-9}}"
        );
        let report = compile_source(
            &source("dynamic-live-source-write", &program),
            &CompileOptions {
                stop_after: Stage::Verify,
                ..CompileOptions::default()
            },
        );
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
                .any(|d| d.message.contains("actual stored target/parent loan")),
            "{}",
            report.to_json()
        );
    }
    verify_source(
        "after last dynamic use",
        "fn main()->i32{var first=7;var second=9;let items=[&first,&second];let index=1;let pointer=items[index];let value=*pointer;first=11;second=12;return value-9}",
    );
    verify_source(
        "constant selection keeps precision",
        "fn main()->i32{var first=7;var second=9;let items=[&first,&second];let pointer=items[1];first=11;return *pointer-9}",
    );
}

#[test]
fn raw_and_wire_reject_live_writes_to_each_dynamic_alternative() {
    let program = "fn main()->i32{var first=7;var second=9;var spare=0;let items=[&first,&second];let index=1;let pointer=items[index];spare=11;return *pointer-9}";
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    let good = typed_source("dynamic-actual-source", program, &target, &limits);
    let prior = zil_analyze::analyze(&good, &limits).unwrap();
    zil_verify::verify_module(good.clone(), prior.clone(), &target, &limits).unwrap();
    for target_name in ["first", "second"] {
        let mut bad = good.clone();
        let f = bad.functions.iter_mut().find(|f| f.name == "main").unwrap();
        let spare = f.locals.iter().find(|l| l.name == "spare").unwrap().id;
        let selected = f.locals.iter().find(|l| l.name == target_name).unwrap().id;
        let store = f
            .blocks
            .iter_mut()
            .flat_map(|b| &mut b.instructions)
            .filter(|i| matches!(i.kind,zil_ir::InstructionKind::Store{local,..}if local==spare))
            .last()
            .unwrap();
        let zil_ir::InstructionKind::Store { local, .. } = &mut store.kind else {
            unreachable!()
        };
        *local = selected;
        let analyzed = zil_analyze::analyze(&bad, &limits).unwrap_err();
        assert!(
            analyzed
                .iter()
                .any(|d| d.message.contains("actual stored target/parent loan")),
            "{target_name}: {analyzed:?}"
        );
        let mut candidate = prior.clone();
        candidate.module_fingerprint = bad.fingerprint();
        let errors = zil_verify::verify_module(bad.clone(), candidate.clone(), &target, &limits)
            .unwrap_err();
        assert!(
            errors
                .iter()
                .any(|d| d.message.contains("live incompatible loan")),
            "{target_name}: {errors:?}"
        );
        let wire = zil_ir::wire::encode_module_candidate(&bad, &candidate, &limits).unwrap();
        let errors = zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap_err();
        assert!(
            errors
                .iter()
                .any(|d| d.message.contains("live incompatible loan")),
            "{target_name}: {errors:?}"
        );
        println!(
            "dynamic alternative {target_name}: actual live-loan rejection in Analyze/Verify/Wire"
        );
    }
}

#[test]
fn index_side_effects_cannot_write_a_snapshot_referent() {
    for target in ["first", "second"] {
        let program = format!(
            "fn next(counter:&mut i32)->i32{{*counter=11;return 1}}fn main()->i32{{var first=7;var second=9;let items=[&first,&second];let pointer=items[next(&mut {target})];return *pointer-9}}"
        );
        let report = compile_source(
            &source("snapshot-index-gap", &program),
            &CompileOptions {
                stop_after: Stage::Verify,
                ..CompileOptions::default()
            },
        );
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
                .any(|d| d.message.contains("actual stored target/parent loan")),
            "{}",
            report.to_json()
        );
    }
}

#[test]
fn each_integer_index_type_uses_the_actual_shared_array() {
    for suffix in ["u8", "u16", "i32", "i64", "u64", "usize"] {
        let program = format!(
            "fn main()->i32{{let first=7;let second=9;let items=[&first,&second];let index:{suffix}=1;return *items[index]-9}}"
        );
        verify_source(suffix, &program);
    }
}

fn snapshot_then_dynamic_extract(refill: bool) -> zil_ir::TypedModule {
    let body = "let initial=&left;let unused=initial.items[0];let selected=if flag{&left}else{&right};let snapshot=selected.items;return *snapshot[1]";
    let mut raw = super::shared_array_snapshot_initialization::local_snapshot_body(body, refill);
    let f = raw.functions.iter_mut().find(|f| f.name == "read").unwrap();
    let aggregate = f
        .blocks
        .iter()
        .flat_map(|b| &b.instructions)
        .find(|i| {
            matches!(i.kind, zil_ir::InstructionKind::ReadAddress { .. })
                && i.result
                    .is_some_and(|v| matches!(f.values[v.0 as usize], Type::Array { .. }))
        })
        .unwrap()
        .result
        .unwrap();
    let (block,at)=f.blocks.iter().enumerate().find_map(|(b,block)|block.instructions.iter().rposition(|i|matches!(&i.kind,zil_ir::InstructionKind::LoadStoredBorrow{path,mode:zil_ir::StoredBorrowLoad::CopyShared,..}if path==&[zil_ir::PlaceProjection::Field(1)])).map(|i|(b,i))).unwrap();
    let index = zil_ir::ValueId(f.values.len() as u32);
    f.values.push(Type::I32);
    let span = f.blocks[block].instructions[at].span;
    f.blocks[block].instructions[at].kind =
        zil_ir::InstructionKind::ExtractIndex { aggregate, index };
    f.blocks[block].instructions.insert(
        at,
        zil_ir::Instruction {
            result: Some(index),
            kind: zil_ir::InstructionKind::Constant(zil_ir::Constant::I32(1)),
            span,
        },
    );
    raw
}

#[test]
fn dynamic_extract_cannot_hide_an_incomplete_whole_snapshot() {
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    let good = snapshot_then_dynamic_extract(true);
    let plan = zil_analyze::analyze(&good, &limits).unwrap();
    zil_verify::verify_module(good.clone(), plan.clone(), &target, &limits).unwrap();
    let wire = zil_ir::wire::encode_module_candidate(&good, &plan, &limits).unwrap();
    zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap();
    let bad = snapshot_then_dynamic_extract(false);
    let errors = zil_analyze::analyze(&bad, &limits).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|d| d.message.contains("Array snapshot reads a vacant slot")),
        "{errors:?}"
    );
    let mut prior = plan;
    prior.module_fingerprint = bad.fingerprint();
    let errors =
        zil_verify::verify_module(bad.clone(), prior.clone(), &target, &limits).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|d| d.message.contains("Array snapshot reads a vacant slot")),
        "{errors:?}"
    );
    let wire = zil_ir::wire::encode_module_candidate(&bad, &prior, &limits).unwrap();
    let errors = zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|d| d.message.contains("Array snapshot reads a vacant slot")),
        "{errors:?}"
    );
    println!(
        "dynamic Extract after partial two-root snapshot: Analyze/Verify/Wire actual vacancy rejection"
    );
}

#[test]
fn native_object_and_index_evaluate_once_in_order_and_bounds_trap() {
    use std::os::unix::process::ExitStatusExt;
    let ordered = "extern \"C\" fn putchar(c:i32)->i32;fn make<life a>(first:&'a i32,second:&'a i32)->[&'a i32;2]{unsafe{putchar(65)}return [first,second]}fn next(counter:&mut i32)->i32{*counter=*counter+1;unsafe{putchar(73)}return 1}fn main()->i32{let first=7;let second=9;var counter=0;let pointer=make(&first,&second)[next(&mut counter)];if counter!=1{return 1}return *pointer-9}";
    let mut runs = 0;
    let mut traps = 0;
    for backend in [NativeBackend::Clang, NativeBackend::LlvmApi] {
        if backend == NativeBackend::LlvmApi && !cfg!(feature = "llvm-api") {
            continue;
        }
        for optimization in [Optimization::O0, Optimization::O2] {
            for (label,program) in [("ordered",ordered.to_string()),("cleanup",CLEANUP_SNAPSHOT.to_string()),("mixed",MIXED_COPY.to_string()),("negative","fn main()->i32{let first=7;let second=9;let items=[&first,&second];let index=-1;return *items[index]}".to_string()),("past-end","fn main()->i32{let first=7;let second=9;let items=[&first,&second];let index=2;return *items[index]}".to_string())] {
                let dir=std::env::temp_dir().join(format!("zil-dynamic-shared-array-{}-{backend:?}-{optimization:?}-{label}",std::process::id()));std::fs::create_dir_all(&dir).unwrap();
                let executable=dir.join("program");
                let report=compile_source(&source(label,&program),&CompileOptions{stop_after:Stage::Build,backend,optimization,output:Some(executable.clone()),..CompileOptions::default()});
                assert_eq!(report.status,ResultCategory::Success,"{label}: {}",report.to_json());
                assert_eq!(report.verification_runs,1);
                let native=report.native_codegen.as_ref().unwrap();assert_eq!(native.backend,backend);assert!(native.fallback.is_none());
                let result=std::process::Command::new(&executable).output().unwrap();
                if label=="ordered" || label=="cleanup" || label=="mixed" { assert_eq!(result.status.code(),Some(0),"{result:?}");let expected: &[u8]=if label=="ordered"{b"AI"}else if label=="cleanup"{b"RV"}else{b""};assert_eq!(result.stdout,expected);assert!(result.stderr.is_empty());runs+=1; }
                else { assert_eq!(result.status.signal(),Some(4),"{label}: {result:?}");assert!(result.stdout.is_empty());traps+=1; }
                std::fs::remove_dir_all(dir).unwrap();
            }
        }
    }
    assert_eq!(runs, 6 + 6 * usize::from(cfg!(feature = "llvm-api")));
    assert_eq!(traps, 4 + 4 * usize::from(cfg!(feature = "llvm-api")));
    println!(
        "dynamic Array: {runs} native once/order and cleanup successes and {traps} actual bounds traps"
    );
}

#[test]
fn changing_a_valid_dynamic_index_preserves_the_same_conservative_proof() {
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    let mut raw = typed_source("dynamic-valid-index-change", WINDOW, &target, &limits);
    let mut plan = zil_analyze::analyze(&raw, &limits).unwrap();
    let f = raw.functions.iter_mut().find(|f| f.name == "main").unwrap();
    let call = f
        .blocks
        .iter()
        .flat_map(|b| &b.instructions)
        .find(
            |i| matches!(&i.kind,zil_ir::InstructionKind::Call{arguments,..}if arguments.len()==2),
        )
        .unwrap();
    let zil_ir::InstructionKind::Call { arguments, .. } = &call.kind else {
        unreachable!()
    };
    let index = arguments[1];
    let source = f
        .blocks
        .iter_mut()
        .flat_map(|b| &mut b.instructions)
        .find(|i| i.result == Some(index))
        .unwrap();
    assert!(matches!(
        source.kind,
        zil_ir::InstructionKind::Constant(zil_ir::Constant::I32(1))
    ));
    source.kind = zil_ir::InstructionKind::Constant(zil_ir::Constant::I32(0));
    plan.module_fingerprint = raw.fingerprint();
    zil_analyze::analyze(&raw, &limits).unwrap();
    zil_verify::verify_module(raw.clone(), plan.clone(), &target, &limits).unwrap();
    let wire = zil_ir::wire::encode_module_candidate(&raw, &plan, &limits).unwrap();
    zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap();
    println!(
        "valid dynamic 1->0: Analyze and unchanged conservative candidate Verify/Wire Success"
    );
}

#[test]
fn dynamic_snapshot_and_alternative_budgets_are_exact_and_fresh() {
    let default = Limits::default();
    let target = TargetProfile::host_llvm();
    let mut boundaries = Vec::new();
    for (qualified, length, width, depth) in [
        (false, 2, 1, 0),
        (false, 8, 1, 0),
        (false, 2, 8, 0),
        (false, 2, 1, 3),
        (true, 2, 1, 0),
        (true, 8, 1, 0),
        (true, 2, 8, 0),
        (true, 2, 1, 3),
    ] {
        let program = array_budget_model(length, width, depth, qualified);
        let program = program
            .replace(
                &format!("let pointer=items[{}]", length - 1),
                &format!("let index={};let pointer=items[index]", length - 1),
            )
            .replace(
                &format!("let pointer=window[{}]", length - 1),
                &format!("let index={};let pointer=window[index]", length - 1),
            );
        let input = source("dynamic-array-budget", &program);
        let syntax = zil_syntax::parse(&input).into_result().unwrap();
        let core = zil_expand::expand_with_map_limits_target_context(
            &input,
            &syntax,
            &default,
            &target,
            zil_expand::source_context::SourceIdentityContext::ClosedSnapshot {
                root: input.module_id,
                aliases: &Default::default(),
                declarations: zil_expand::source_context::DeclarationPlacement::Original,
            },
        )
        .unwrap()
        .core;
        let raw = zil_types::check(&core, &target, &default).unwrap();
        let plan = zil_analyze::analyze(&raw, &default).unwrap();
        let wire = zil_ir::wire::encode_module_candidate(&raw, &plan, &default).unwrap();
        let at = |n| Limits {
            max_analysis_steps: n,
            ..default.clone()
        };
        let label =
            format!("dynamic Array qualified={qualified} len={length} width={width} depth={depth}");
        let types = exact_array_budget(&format!("{label} Types"), |n| {
            array_budget_category(zil_types::check(&core, &target, &at(n)))
        });
        let analyze = exact_array_budget(&format!("{label} Analyze"), |n| {
            array_budget_category(zil_analyze::analyze(&raw, &at(n)))
        });
        let verify = exact_array_budget(&format!("{label} Verify"), |n| {
            array_budget_category(zil_verify::verify_module(
                raw.clone(),
                plan.clone(),
                &target,
                &at(n),
            ))
        });
        let decoded = exact_array_budget(&format!("{label} Wire"), |n| {
            array_budget_category(zil_verify::verify_module_bytes(&wire, &target, &at(n)))
        });
        let whole = exact_array_budget(&format!("{label} Source"), |n| {
            compile_source(
                &input,
                &CompileOptions {
                    stop_after: Stage::Verify,
                    limits: at(n),
                    ..CompileOptions::default()
                },
            )
            .status
        });
        let mut session = zil_driver::CompilationSession::default();
        for (n, expected) in [
            (whole, ResultCategory::Success),
            (whole - 1, ResultCategory::ResourceLimit),
            (whole, ResultCategory::Success),
        ] {
            let report = session.compile_source(
                &input,
                &CompileOptions {
                    stop_after: Stage::Verify,
                    limits: at(n),
                    ..CompileOptions::default()
                },
            );
            assert_eq!(report.status, expected, "{label}: {}", report.to_json());
        }
        boundaries.push([types, analyze, verify, decoded, whole]);
    }
    for base in [0, 4] {
        for change in [1, 2, 3] {
            for stage in 0..5 {
                assert!(boundaries[base + change][stage] > boundaries[base][stage]);
            }
        }
    }
    assert_eq!(
        boundaries,
        vec![
            [1844, 28930, 35093, 35093, 35093],
            [2480, 90070, 123350, 123350, 123350],
            [4420, 54955, 64969, 64969, 64969],
            [6467, 58503, 70655, 70655, 70655],
            [3622, 48038, 63653, 63653, 63653],
            [4846, 122864, 188093, 188093, 188093],
            [6576, 77990, 102944, 102944, 102944],
            [9109, 87013, 116429, 116429, 116429],
        ]
    );
    println!("dynamic Array complete boundaries: {boundaries:?}");
}

#[test]
fn raw_and_wire_reject_dynamic_index_type_and_incomplete_actual_array() {
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    let good = typed_source("dynamic-shape-boundary", WINDOW, &target, &limits);
    let prior = zil_analyze::analyze(&good, &limits).unwrap();
    for case in ["boolean index", "missing array operand"] {
        let mut bad = good.clone();
        if case == "boolean index" {
            let f = bad
                .functions
                .iter_mut()
                .find(|f| f.name == "choose")
                .unwrap();
            let value = zil_ir::ValueId(f.values.len() as u32);
            f.values.push(Type::Bool);
            let (block, at) = f
                .blocks
                .iter()
                .enumerate()
                .find_map(|(b, block)| {
                    block
                        .instructions
                        .iter()
                        .position(|i| {
                            matches!(i.kind, zil_ir::InstructionKind::ExtractIndex { .. })
                        })
                        .map(|i| (b, i))
                })
                .unwrap();
            let span = f.blocks[block].instructions[at].span;
            let zil_ir::InstructionKind::ExtractIndex { index, .. } =
                &mut f.blocks[block].instructions[at].kind
            else {
                unreachable!()
            };
            *index = value;
            f.blocks[block].instructions.insert(
                at,
                zil_ir::Instruction {
                    result: Some(value),
                    kind: zil_ir::InstructionKind::Constant(zil_ir::Constant::Bool(true)),
                    span,
                },
            );
        } else {
            let f = bad.functions.iter_mut().find(|f| f.name == "main").unwrap();
            let fields = f
                .blocks
                .iter_mut()
                .flat_map(|b| &mut b.instructions)
                .find_map(|i| {
                    if let zil_ir::InstructionKind::MakeAggregate { fields } = &mut i.kind {
                        Some(fields)
                    } else {
                        None
                    }
                })
                .unwrap();
            assert_eq!(fields.len(), 2);
            fields.pop();
        }
        let errors = zil_analyze::analyze(&bad, &limits).unwrap_err();
        assert!(
            errors
                .iter()
                .all(|d| d.category == ResultCategory::SafeReject),
            "{case}: {errors:?}"
        );
        let mut candidate = prior.clone();
        candidate.module_fingerprint = bad.fingerprint();
        let errors = zil_verify::verify_module(bad.clone(), candidate.clone(), &target, &limits)
            .unwrap_err();
        assert!(
            errors
                .iter()
                .all(|d| d.category == ResultCategory::SafeReject),
            "{case}: {errors:?}"
        );
        let wire = zil_ir::wire::encode_module_candidate(&bad, &candidate, &limits).unwrap();
        let errors = zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap_err();
        assert!(
            errors
                .iter()
                .all(|d| d.category == ResultCategory::SafeReject),
            "{case}: {errors:?}"
        );
        println!("{case}: Analyze/Verify/Wire independent actual type/shape rejection");
    }
}

#[test]
fn distinct_uppers_survive_local_copies_reborrows_and_scalar_observers() {
    let program = "struct Refs<life a>{pointer:&'a i32}fn observe(pointer:&i32)->i32{return *pointer}fn read<life a,life b>(left:Refs<'a>,right:Refs<'b>,index:i32)->i32{let items=[left.pointer,right.pointer];let pointer=items[index];let copied=pointer;let reborrowed=&*copied;return observe(reborrowed)}fn main()->i32{return 0}";
    verify_source("mixed upper copy/reborrow/observer", program);
}

#[test]
fn dynamic_uppers_are_checked_at_real_return_and_local_slot_boundaries() {
    let declarations = "struct Refs<life a>{pointer:&'a i32}";
    let bad_return = format!(
        "{declarations}fn wrong<life a,life b>(left:Refs<'a>,right:Refs<'b>,index:i32)->&'a i32{{let items=[left.pointer,right.pointer];return items[index]}}fn main()->i32{{return 0}}"
    );
    let good_store = format!(
        "{declarations}fn write<life a,life b>(left:Refs<'a>,right:Refs<'b>,index:i32)->i32{{let items=[left.pointer,left.pointer];let pointer=items[index];var holder=Refs::<'a>{{pointer:pointer}};holder.pointer=left.pointer;return *holder.pointer}}fn main()->i32{{return 0}}"
    );
    verify_source(
        "dynamic upper actual slot compatible replacement",
        &good_store,
    );
    let bad_store = good_store.replace(
        "holder.pointer=left.pointer",
        "holder.pointer=right.pointer",
    );
    for (label, program) in [
        ("mixed upper return claims a", bad_return),
        ("single b replaces actual a slot", bad_store),
    ] {
        let report = compile_source(
            &source(label, &program),
            &CompileOptions {
                stop_after: Stage::Verify,
                ..CompileOptions::default()
            },
        );
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
                .any(|d| d.message.contains("retained class")),
            "{label}: {}",
            report.to_json()
        );
        println!("{label}: actual expected class rejects incompatible source");
    }
}

#[test]
fn raw_and_wire_cannot_replace_a_declared_return_source_by_another_upper() {
    let program = "struct Refs<life a>{pointer:&'a i32}fn choose<life a,life b>(left:Refs<'a>,right:Refs<'b>,index:i32)->&'a i32{let other=right.pointer;let items=[left.pointer,left.pointer];return items[index]}fn main()->i32{return 0}";
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    let mut raw = typed_source("dynamic-upper-real-source", program, &target, &limits);
    let mut plan = zil_analyze::analyze(&raw, &limits).unwrap();
    zil_verify::verify_module(raw.clone(), plan.clone(), &target, &limits).unwrap();
    let f = raw
        .functions
        .iter_mut()
        .find(|f| f.name == "choose")
        .unwrap();
    let other = f.locals.iter().find(|l| l.name == "other").unwrap().id;
    let right = f
        .blocks
        .iter()
        .flat_map(|b| &b.instructions)
        .find_map(|i| {
            if let zil_ir::InstructionKind::Store { local, value } = i.kind {
                (local == other).then_some(value)
            } else {
                None
            }
        })
        .unwrap();
    let fields = f
        .blocks
        .iter_mut()
        .flat_map(|b| &mut b.instructions)
        .find_map(|i| {
            if let zil_ir::InstructionKind::MakeAggregate { fields } = &mut i.kind {
                Some(fields)
            } else {
                None
            }
        })
        .unwrap();
    assert_eq!(fields.len(), 2);
    fields[1] = right;
    let errors = zil_analyze::analyze(&raw, &limits).unwrap_err();
    assert!(
        errors.iter().any(|d| d.message.contains("retained class")),
        "{errors:?}"
    );
    plan.module_fingerprint = raw.fingerprint();
    let errors =
        zil_verify::verify_module(raw.clone(), plan.clone(), &target, &limits).unwrap_err();
    assert!(
        errors.iter().any(|d| d.message.contains("retained class")),
        "{errors:?}"
    );
    let wire = zil_ir::wire::encode_module_candidate(&raw, &plan, &limits).unwrap();
    let errors = zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap_err();
    assert!(
        errors.iter().any(|d| d.message.contains("retained class")),
        "{errors:?}"
    );
    println!(
        "actual b source inserted in a-returning dynamic selector: Analyze/Verify/Wire independent class rejection"
    );
}

#[test]
fn raw_and_wire_check_a_single_upper_against_the_real_stored_destination() {
    let program = "struct Refs<life a>{pointer:&'a i32}fn write<life a,life b>(left:Refs<'a>,right:Refs<'b>,index:i32)->i32{let other=right.pointer;let items=[left.pointer,left.pointer];let pointer=items[index];var holder=Refs::<'a>{pointer:pointer};holder.pointer=left.pointer;return *holder.pointer}fn main()->i32{return 0}";
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    let mut raw = typed_source("dynamic-actual-slot-upper", program, &target, &limits);
    let mut plan = zil_analyze::analyze(&raw, &limits).unwrap();
    zil_verify::verify_module(raw.clone(), plan.clone(), &target, &limits).unwrap();
    let f = raw
        .functions
        .iter_mut()
        .find(|f| f.name == "write")
        .unwrap();
    let other = f.locals.iter().find(|l| l.name == "other").unwrap().id;
    let source = f
        .blocks
        .iter()
        .flat_map(|b| &b.instructions)
        .find_map(|i| {
            if let zil_ir::InstructionKind::Store { local, value } = i.kind {
                (local == other).then_some(value)
            } else {
                None
            }
        })
        .unwrap();
    let stored = f
        .blocks
        .iter_mut()
        .flat_map(|b| &mut b.instructions)
        .find(|i| matches!(i.kind, zil_ir::InstructionKind::StoreStoredBorrow { .. }))
        .unwrap();
    let zil_ir::InstructionKind::StoreStoredBorrow { value, .. } = &mut stored.kind else {
        unreachable!()
    };
    *value = source;
    let errors = zil_analyze::analyze(&raw, &limits).unwrap_err();
    assert!(
        errors.iter().any(|d| d.message.contains("retained class")),
        "{errors:?}"
    );
    plan.module_fingerprint = raw.fingerprint();
    let errors =
        zil_verify::verify_module(raw.clone(), plan.clone(), &target, &limits).unwrap_err();
    assert!(
        errors.iter().any(|d| d.message.contains("retained class")),
        "{errors:?}"
    );
    let wire = zil_ir::wire::encode_module_candidate(&raw, &plan, &limits).unwrap();
    let errors = zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap_err();
    assert!(
        errors.iter().any(|d| d.message.contains("retained class")),
        "{errors:?}"
    );
    println!(
        "single actual b replaces actual a destination: Analyze/Verify/Wire independent expected-class rejection"
    );
}

#[test]
fn unknown_legacy_upper_sources_cannot_be_masked_by_a_known_branch() {
    let mut reports = Vec::new();
    for (label, prefix, pointer) in [
        (
            "legacy right",
            "",
            "if flag{items[index]}else{id(right.pointer)}",
        ),
        (
            "legacy left",
            "",
            "if flag{items[index]}else{id(left.pointer)}",
        ),
        (
            "direct right",
            "",
            "if flag{items[index]}else{right.pointer}",
        ),
        ("direct left", "", "if flag{items[index]}else{left.pointer}"),
        (
            "two-level unknown",
            "let q=if flag{left.pointer}else{id(right.pointer)};",
            "if flag{items[index]}else{q}",
        ),
        (
            "rich wrapper unknown",
            "let boxed=make(id(right.pointer));",
            "if flag{items[index]}else{boxed.pointer}",
        ),
        ("legacy without dynamic", "", "id(right.pointer)"),
    ] {
        let program = format!(
            "struct Refs<life a>{{pointer:&'a i32}}fn make<life a>(pointer:&'a i32)->Refs<'a>{{return Refs::<'a>{{pointer:pointer}}}}fn id<life p>(pointer:&'p i32)->&'p i32{{return pointer}}fn read<life a,life b>(left:Refs<'a>,right:Refs<'b>,index:i32,flag:bool)->i32{{{prefix}let items=[left.pointer,left.pointer];let pointer={pointer};let holder=Refs::<'a>{{pointer:pointer}};return *holder.pointer}}fn main()->i32{{let first=7;let second=9;return read(make(&first),make(&second),1,false)-9}}"
        );
        let report = compile_source(
            &source(label, &program),
            &CompileOptions {
                stop_after: Stage::Verify,
                ..CompileOptions::default()
            },
        );
        println!("unknown-upper branch probe {label}: {}", report.to_json());
        reports.push((label, report));
    }
    for (label, report) in reports {
        let expected = match label {
            "direct left" => ResultCategory::Success,
            "direct right" => ResultCategory::SafeReject,
            "legacy without dynamic" => continue,
            _ => ResultCategory::Unsupported,
        };
        assert_eq!(report.status, expected, "{label}: {}", report.to_json());
    }
}

#[test]
fn seeded_parameter_upper_still_requires_every_store_incoming_source() {
    let program = "struct Refs<life a>{pointer:&'a i32}fn id<life p>(pointer:&'p i32)->&'p i32{return pointer}fn read<life a,life b>(left:Refs<'a>,right:Refs<'b>,supplied:&'a i32,index:i32,flag:bool)->i32{var local=supplied;local=id(right.pointer);let items=[left.pointer,left.pointer];let pointer=if flag{items[index]}else{local};let holder=Refs::<'a>{pointer:pointer};return *holder.pointer}fn main()->i32{return 0}";
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    let mut raw = typed_source("seeded-parameter-unknown-store", program, &target, &limits);
    let f = raw.functions.iter_mut().find(|f| f.name == "read").unwrap();
    let local = f.locals.iter().find(|l| l.name == "local").unwrap().id;
    let supplied = f.locals.iter().find(|l| l.name == "supplied").unwrap().id;
    f.locals[supplied.0 as usize].mutable = true;
    for i in f.blocks.iter_mut().flat_map(|b| &mut b.instructions) {
        match &mut i.kind {
            zil_ir::InstructionKind::Load(id) if *id == local => *id = supplied,
            zil_ir::InstructionKind::Store { local: id, .. } if *id == local => *id = supplied,
            _ => {}
        }
    }
    let errors = zil_analyze::analyze(&raw, &limits).unwrap_err();
    assert!(
        errors.iter().any(|d| d
            .message
            .contains("upper dependency has no complete actual source")),
        "{errors:?}"
    );
    println!(
        "seed parameter plus unknown Store: required closure refuses to stop at its initial seed"
    );
}

#[test]
fn raw_and_wire_reject_unknown_incoming_upper_before_identity_binding() {
    let program = "struct Refs<life a>{pointer:&'a i32}fn id<life p>(pointer:&'p i32)->&'p i32{return pointer}fn read<life a,life b>(left:Refs<'a>,right:Refs<'b>,index:i32,flag:bool)->i32{let other=id(right.pointer);let items=[left.pointer,left.pointer];let pointer=if flag{items[index]}else{left.pointer};let holder=Refs::<'a>{pointer:pointer};return *holder.pointer}fn main()->i32{return 0}";
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    let mut raw = typed_source("unknown-upper-real-incoming", program, &target, &limits);
    let mut prior = zil_analyze::analyze(&raw, &limits).unwrap();
    zil_verify::verify_module(raw.clone(), prior.clone(), &target, &limits).unwrap();
    let f = raw.functions.iter_mut().find(|f| f.name == "read").unwrap();
    let other = f.locals.iter().find(|l| l.name == "other").unwrap().id;
    let pointer = f.locals.iter().find(|l| l.name == "pointer").unwrap().id;
    let stored = |local| {
        f.blocks
            .iter()
            .flat_map(|b| &b.instructions)
            .find_map(|i| {
                if let zil_ir::InstructionKind::Store { local: l, value } = i.kind {
                    (l == local).then_some(value)
                } else {
                    None
                }
            })
            .unwrap()
    };
    let unknown = stored(other);
    let selected = stored(pointer);
    let definition = f
        .blocks
        .iter()
        .flat_map(|b| &b.instructions)
        .find(|i| i.result == Some(selected))
        .unwrap();
    let zil_ir::InstructionKind::Load(phi) = definition.kind else {
        panic!("actual conditional local")
    };
    let dynamic = f
        .blocks
        .iter()
        .flat_map(|b| &b.instructions)
        .find(|i| matches!(i.kind, zil_ir::InstructionKind::ExtractIndex { .. }))
        .unwrap()
        .result
        .unwrap();
    let branch=f.blocks.iter_mut().flat_map(|b|&mut b.instructions).find(|i|matches!(i.kind,zil_ir::InstructionKind::Store{local,value}if local==phi&&value!=dynamic)).unwrap();
    let zil_ir::InstructionKind::Store { value, .. } = &mut branch.kind else {
        unreachable!()
    };
    *value = unknown;
    let errors = zil_analyze::analyze(&raw, &limits).unwrap_err();
    assert!(
        errors.iter().any(|d| d
            .message
            .contains("upper dependency has no complete actual source")),
        "{errors:?}"
    );
    prior.module_fingerprint = raw.fingerprint();
    let errors =
        zil_verify::verify_module(raw.clone(), prior.clone(), &target, &limits).unwrap_err();
    assert!(
        errors.iter().any(|d| d
            .message
            .contains("upper dependency has no complete actual source")),
        "{errors:?}"
    );
    let wire = zil_ir::wire::encode_module_candidate(&raw, &prior, &limits).unwrap();
    let errors = zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap_err();
    assert!(
        errors.iter().any(|d| d
            .message
            .contains("upper dependency has no complete actual source")),
        "{errors:?}"
    );
    println!(
        "unknown incoming copy branch: Analyze/Verify/Wire independent completeness rejection"
    );
}

#[test]
fn rich_call_completeness_costs_grow_with_actual_input_and_output_ports() {
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    let mut boundaries = Vec::new();
    for length in [2, 8] {
        let program = array_budget_model(length, 1, 0, false).replace(
            &format!("let pointer=items[{}]", length - 1),
            &format!("let index={};let pointer=relay(*items)[index]", length - 1),
        );
        let raw = typed_source("rich-call-upper-dependencies", &program, &target, &limits);
        let plan = zil_analyze::analyze(&raw, &limits).unwrap();
        let wire = zil_ir::wire::encode_module_candidate(&raw, &plan, &limits).unwrap();
        let at = |n| Limits {
            max_analysis_steps: n,
            ..limits.clone()
        };
        let a = exact_array_budget(
            &format!("rich-call upper requirements len={length} Analyze"),
            |n| array_budget_category(zil_analyze::analyze(&raw, &at(n))),
        );
        let v = exact_array_budget(
            &format!("rich-call upper requirements len={length} Verify"),
            |n| {
                array_budget_category(zil_verify::verify_module(
                    raw.clone(),
                    plan.clone(),
                    &target,
                    &at(n),
                ))
            },
        );
        let w = exact_array_budget(
            &format!("rich-call upper requirements len={length} Wire"),
            |n| array_budget_category(zil_verify::verify_module_bytes(&wire, &target, &at(n))),
        );
        boundaries.push([a, v, w]);
    }
    for phase in 0..3 {
        assert!(boundaries[1][phase] > boundaries[0][phase]);
    }
    println!("rich-call complete input/output requirement boundaries: {boundaries:?}");
}
