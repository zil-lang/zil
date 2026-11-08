//! Every fixed address-array element retains its own actual external target.
use super::*;
const BATCH: &str = include_str!("fixed-shared-address-array.zil");
const MAPPING: &str = include_str!("fixed-shared-address-array-mapping.zil");
const WINDOW: &str = include_str!("fixed-shared-address-array-window.zil");
const HELPER: &str = include_str!("fixed-shared-address-array-helper.zil");

#[test]
fn source_raw_and_wire_preserve_each_array_element() {
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    for (name, program) in [
        ("array-batch", BATCH),
        ("array-helper", HELPER),
        ("array-mapping", MAPPING),
        ("array-window", WINDOW),
    ] {
        let report = compile_source(
            &source(name, program),
            &CompileOptions {
                stop_after: Stage::Verify,
                ..CompileOptions::default()
            },
        );
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1);
        let raw = typed_source(name, program, &target, &limits);
        let plan = zil_analyze::analyze(&raw, &limits).unwrap();
        zil_verify::verify_module(raw.clone(), plan.clone(), &target, &limits).unwrap();
        let wire = zil_ir::wire::encode_module_candidate(&raw, &plan, &limits).unwrap();
        zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap();
        println!("{name}: Source/raw/Wire actual element proof passed");
    }
}

#[test]
fn actual_array_element_sources_and_bounds_cannot_be_extended() {
    let local_escape = BATCH.replacen(
        "return Batch::<'a>{items:[first,second]}",
        "let local=12\n    return Batch::<'a>{items:[first,&local]}",
        1,
    );
    let outer_escape = "fn dangling<life a>(x:&'a i32,y:&'a i32)->&'a [&'a i32;2]{let items=[x,y];return &items}fn main()->i32{return 0}";
    let parent_write = WINDOW
        .replacen(
            "let items=[&first,&second]",
            "var items=[&first,&second]",
            1,
        )
        .replacen(
            "let copied=window",
            "items=[&first,&first]\n    let copied=window",
            1,
        );
    for (label, program) in [
        (
            "actual index1 remains borrowed",
            BATCH.replacen("first=11", "second=11", 1),
        ),
        (
            "same upper different ordinal",
            BATCH.replacen("batch.items[1]", "batch.items[0]", 1),
        ),
        ("short-lived array element escapes", local_escape),
        ("outer array cell escapes", outer_escape.to_string()),
        ("outer window pins the actual parent", parent_write),
        ("actual Array field window keeps element ancestors", BATCH.replacen("let pointer=select(&batch)", "let window=&batch.items;second=11;let pointer=window[1]",1)),
        ("helper returns the actual window leaf ancestors", WINDOW.replacen("let copied=window", "second=11;let copied=window",1)),
        ("copied direct window retains actual external element", "fn main()->i32{var first=7;var second=9;let items=[&first,&second];let window=&items;let copied=window;second=11;return *copied[1]}".to_string()),
        ("explicitly reborrowed window retains actual external element", "fn main()->i32{var first=7;var second=9;let items=[&first,&second];let window=&items;let copied=&*window;second=11;return *copied[1]}".to_string()),
        ("direct window retains actual external element",
            "fn main()->i32{var first=7;var second=9;let items=[&first,&second];let window=&items;second=11;return *window[1]}".to_string()),
        (
            "two lifetime mappings retain their real source",
            MAPPING.replacen("w=9", "z=9", 1),
        ),
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
    }
}

#[test]
fn direct_copied_and_explicitly_reborrowed_windows_are_ordinary_shared_values() {
    for selected in ["window", "copied", "reborrowed"] {
        let program = format!(
            "fn main()->i32{{let first=7;let second=9;let items=[&first,&second];let window=&items;let copied=window;let reborrowed=&*copied;return *{selected}[1]-9}}"
        );
        let report = compile_source(
            &source("array-view", &program),
            &CompileOptions {
                stop_after: Stage::Verify,
                ..CompileOptions::default()
            },
        );
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
    }
}

#[test]
fn dynamic_exclusive_and_empty_address_arrays_keep_their_boundary() {
    let dynamic = BATCH
        .replacen(
            "fn select<life a>(batch:&Batch<'a>)",
            "fn select<life a>(batch:&Batch<'a>,index:i32)",
            1,
        )
        .replacen("batch.items[1]", "batch.items[index]", 1)
        .replacen("select(&batch)", "select(&batch,1)", 1);
    let exclusive = HELPER
        .replace("&'a i32", "&'a mut i32")
        .replace("&first", "&mut first")
        .replace("&second", "&mut second");
    let empty =
        "fn relay<life a>(items:[&'a i32;0])->[&'a i32;0]{return items}fn main()->i32{return 0}";
    for (label, program) in [
        ("dynamic index", dynamic),
        ("exclusive Array copy", exclusive),
        ("empty physical address array", empty.to_string()),
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
            ResultCategory::Unsupported,
            "{label}: {}",
            report.to_json()
        );
    }
    let report = compile_source(
        &source(
            "constant out of range",
            &BATCH.replacen("batch.items[1]", "batch.items[2]", 1),
        ),
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
}

#[test]
fn native_array_ordinals_and_shared_copies_match_both_backends() {
    for backend in [NativeBackend::Clang, NativeBackend::LlvmApi] {
        for optimization in [Optimization::O0, Optimization::O2] {
            for (label, program) in [
                ("batch", BATCH),
                ("helper", HELPER),
                ("mapping", MAPPING),
                ("window", WINDOW),
            ] {
                let dir = std::env::temp_dir().join(format!(
                    "zil-address-array-{}-{backend:?}-{optimization:?}-{label}",
                    std::process::id()
                ));
                std::fs::create_dir_all(&dir).unwrap();
                let executable = dir.join("program");
                let report = compile_source(
                    &source(label, program),
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
                    ResultCategory::Success,
                    "{backend:?}/{optimization:?}/{label}: {}",
                    report.to_json()
                );
                assert_eq!(report.verification_runs, 1);
                let codegen = report.native_codegen.as_ref().unwrap();
                assert_eq!(codegen.backend, backend);
                assert!(codegen.fallback.is_none());
                let output = std::process::Command::new(executable).output().unwrap();
                assert_eq!(
                    output.status.code(),
                    Some(0),
                    "{backend:?}/{optimization:?}/{label}: {output:?}"
                );
                assert!(
                    output.stdout.is_empty() && output.stderr.is_empty(),
                    "{output:?}"
                );
                std::fs::remove_dir_all(dir).unwrap();
            }
        }
    }
}

#[test]
fn raw_and_wire_reject_forged_ordinals_and_missing_element_bindings() {
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    let original = typed_source("array-raw", BATCH, &target, &limits);
    let plan = zil_analyze::analyze(&original, &limits).unwrap();
    for (label, ordinal) in [
        ("same upper wrong source", 0),
        ("out of physical bounds", 2),
    ] {
        let mut raw = original.clone();
        let getter = raw
            .functions
            .iter_mut()
            .find(|f| f.name == "select")
            .unwrap();
        let mut changed = 0;
        for i in getter.blocks.iter_mut().flat_map(|b| &mut b.instructions) {
            if let zil_ir::InstructionKind::LoadStoredBorrow { path, .. } = &mut i.kind {
                assert_eq!(path.as_slice(), &[zil_ir::PlaceProjection::Field(1)]);
                path[0] = zil_ir::PlaceProjection::Field(ordinal);
                changed += 1;
            }
        }
        assert_eq!(changed, 1);
        let mut candidate = plan.clone();
        candidate.module_fingerprint = raw.fingerprint();
        assert!(
            zil_analyze::analyze(&raw, &limits).is_err(),
            "{label}: Analyze"
        );
        assert!(
            zil_verify::verify_module(raw.clone(), candidate.clone(), &target, &limits).is_err(),
            "{label}: Verify"
        );
        let wire = zil_ir::wire::encode_module_candidate(&raw, &candidate, &limits).unwrap();
        assert!(
            zil_verify::verify_module_bytes(&wire, &target, &limits).is_err(),
            "{label}: Wire"
        );
    }
    let original = typed_source("array-helper-raw", HELPER, &target, &limits);
    let plan = zil_analyze::analyze(&original, &limits).unwrap();
    for metadata_element in [false, true] {
        let mut raw = original.clone();
        let relay = raw
            .functions
            .iter_mut()
            .find(|f| f.name == "relay")
            .unwrap();
        let life = relay.storage_life.as_mut().unwrap();
        let row = life
            .bindings
            .iter()
            .position(|r| {
                r.position.path
                    == vec![zil_ir::ContractProjection::Place(
                        zil_ir::PlaceProjection::Field(1),
                    )]
            })
            .unwrap();
        if metadata_element {
            // An empty physical path cannot substitute the missing ordinal,
            // even though every element has the same type and upper bound.
            life.bindings[row].position.path.clear();
        } else {
            life.bindings.remove(row);
        }
        let mut candidate = plan.clone();
        candidate.module_fingerprint = raw.fingerprint();
        assert!(zil_analyze::analyze(&raw, &limits).is_err());
        assert!(
            zil_verify::verify_module(raw.clone(), candidate.clone(), &target, &limits).is_err()
        );
        let wire = zil_ir::wire::encode_module_candidate(&raw, &candidate, &limits).unwrap();
        assert!(zil_verify::verify_module_bytes(&wire, &target, &limits).is_err());
    }
    let mut raw = original;
    let relay = raw
        .functions
        .iter_mut()
        .find(|f| f.name == "relay")
        .unwrap();
    relay
        .storage_life
        .as_mut()
        .unwrap()
        .classes
        .push(zil_ir::SignatureLifeClassBinding {
            position: zil_ir::SignatureLifeClass {
                base: zil_ir::StorageBase::Parameter(0),
                path: vec![zil_ir::LifeTypeProjection::Element],
                class: 0,
            },
            binder: zil_ir::StorageBinder::Invocation(0),
        });
    let mut candidate = plan;
    candidate.module_fingerprint = raw.fingerprint();
    assert!(zil_analyze::analyze(&raw, &limits).is_err());
    assert!(zil_verify::verify_module(raw.clone(), candidate.clone(), &target, &limits).is_err());
    let wire = zil_ir::wire::encode_module_candidate(&raw, &candidate, &limits).unwrap();
    assert!(zil_verify::verify_module_bytes(&wire, &target, &limits).is_err());
    let mut raw = typed_source("array-mapping-raw", MAPPING, &target, &limits);
    let mut candidate = zil_analyze::analyze(&raw, &limits).unwrap();
    let make = raw.functions.iter_mut().find(|f| f.name == "make").unwrap();
    let row = make
        .storage_life
        .as_mut()
        .unwrap()
        .classes
        .iter_mut()
        .find(|r| r.position.base == zil_ir::StorageBase::Result)
        .unwrap();
    let zil_ir::StorageBinder::Invocation(actual) = row.binder else {
        panic!("expected invocation lifetime class")
    };
    assert!(actual < 2);
    row.binder = zil_ir::StorageBinder::Invocation(1 - actual);
    candidate.module_fingerprint = raw.fingerprint();
    assert!(zil_analyze::analyze(&raw, &limits).is_err());
    assert!(zil_verify::verify_module(raw.clone(), candidate.clone(), &target, &limits).is_err());
    let wire = zil_ir::wire::encode_module_candidate(&raw, &candidate, &limits).unwrap();
    assert!(zil_verify::verify_module_bytes(&wire, &target, &limits).is_err());
    let mut raw = typed_source("array-missing-constructor-source", HELPER, &target, &limits);
    let mut candidate = zil_analyze::analyze(&raw, &limits).unwrap();
    let make = raw.functions.iter_mut().find(|f| f.name == "make").unwrap();
    let constructor = make
        .blocks
        .iter_mut()
        .flat_map(|b| &mut b.instructions)
        .find(|i| matches!(i.kind, zil_ir::InstructionKind::MakeAggregate { .. }))
        .unwrap();
    let zil_ir::InstructionKind::MakeAggregate { fields } = &mut constructor.kind else {
        unreachable!()
    };
    assert_eq!(fields.len(), 2);
    fields.pop();
    candidate.module_fingerprint = raw.fingerprint();
    assert!(zil_analyze::analyze(&raw, &limits).is_err());
    assert!(zil_verify::verify_module(raw.clone(), candidate.clone(), &target, &limits).is_err());
    let wire = zil_ir::wire::encode_module_candidate(&raw, &candidate, &limits).unwrap();
    assert!(zil_verify::verify_module_bytes(&wire, &target, &limits).is_err());
    println!(
        "array raw/Wire: wrong actual ordinal, out-of-range, missing binding/source, false root, metadata-only Element, wrong nonidentity lifetime mapping rejected"
    );
}

fn array_budget_model(length: usize, width: usize, depth: usize, qualified: bool) -> String {
    let fields = (0..width)
        .map(|i| format!("v{i}:i32"))
        .collect::<Vec<_>>()
        .join(";");
    let values = (0..width)
        .map(|i| format!("v{i}:7"))
        .collect::<Vec<_>>()
        .join(",");
    let mut text = format!("struct Leaf{{{fields}}}");
    let mut ty = "Leaf".to_string();
    let mut value = format!("Leaf{{{values}}}");
    let mut access = "pointer".to_string();
    for i in 0..depth {
        let next = format!("Layer{i}");
        text.push_str(&format!("struct {next}{{inner:{ty}}}"));
        value = format!("{next}{{inner:{value}}}");
        ty = next;
        access.push_str(".inner");
    }
    let elements = std::iter::repeat_n("pointer", length)
        .collect::<Vec<_>>()
        .join(",");
    if qualified {
        text.push_str(&format!("struct Batch<life a>{{items:[&'a {ty};{length}]}}fn make<life a>(pointer:&'a {ty})->Batch<'a>{{return Batch::<'a>{{items:[{elements}]}}}}fn relay<life a>(items:Batch<'a>)->Batch<'a>{{let copied=items;return copied}}fn read<life a>(items:&Batch<'a>)->i32{{let window=&items.items;let pointer=window[{}];return {access}.v0}}fn main()->i32{{let value={value};let items=relay(make(&value));let window=&items;let copied=window;return read(copied)-7}}",length-1));
    } else {
        text.push_str(&format!("fn make<life a>(pointer:&'a {ty})->[&'a {ty};{length}]{{return [{elements}]}}fn relay<life a>(items:[&'a {ty};{length}])->[&'a {ty};{length}]{{let copied=items;return copied}}fn read<life a>(items:&[&'a {ty};{length}])->i32{{let pointer=items[{}];return {access}.v0}}fn main()->i32{{let value={value};let items=relay(make(&value));let window=&items;let copied=window;return read(copied)-7}}",length-1));
    }
    text
}

fn array_budget_category<T>(result: Result<T, Vec<zil_base::Diagnostic>>) -> ResultCategory {
    match result {
        Ok(_) => ResultCategory::Success,
        Err(e)
            if e.iter()
                .all(|d| d.category == ResultCategory::ResourceLimit) =>
        {
            ResultCategory::ResourceLimit
        }
        Err(e) => panic!("unexpected array budget rejection: {e:?}"),
    }
}
fn exact_array_budget(label: &str, mut run: impl FnMut(usize) -> ResultCategory) -> usize {
    let mut low = 1;
    let mut high = Limits::default().max_analysis_steps;
    assert_eq!(run(high), ResultCategory::Success, "{label}: default");
    while low < high {
        let mid = low + (high - low) / 2;
        match run(mid) {
            ResultCategory::Success => high = mid,
            ResultCategory::ResourceLimit => low = mid + 1,
            other => panic!("{label}: {other:?}"),
        }
    }
    for _ in 0..2 {
        assert_eq!(run(low), ResultCategory::Success);
        assert_eq!(run(low - 1), ResultCategory::ResourceLimit);
        assert_eq!(run(low), ResultCategory::Success);
    }
    println!("{label}: N={low}, N-1=ResourceLimit, fresh/retry=Pass");
    low
}
#[test]
fn array_slots_and_actual_pointee_snapshot_budgets_are_exact_and_fresh() {
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
        let input = source("array-budget", &program);
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
        let limits = |n| Limits {
            max_analysis_steps: n,
            ..default.clone()
        };
        let label = format!("Array qualified={qualified} len={length} width={width} depth={depth}");
        let types = exact_array_budget(&format!("{label} Types"), |n| {
            array_budget_category(zil_types::check(&core, &target, &limits(n)))
        });
        let analyze = exact_array_budget(&format!("{label} Analyze"), |n| {
            array_budget_category(zil_analyze::analyze(&raw, &limits(n)))
        });
        let verify = exact_array_budget(&format!("{label} Verify"), |n| {
            array_budget_category(zil_verify::verify_module(
                raw.clone(),
                plan.clone(),
                &target,
                &limits(n),
            ))
        });
        let decoded = exact_array_budget(&format!("{label} Wire"), |n| {
            array_budget_category(zil_verify::verify_module_bytes(&wire, &target, &limits(n)))
        });
        let whole = exact_array_budget(&format!("{label} Source"), |n| {
            compile_source(
                &input,
                &CompileOptions {
                    stop_after: Stage::Verify,
                    limits: limits(n),
                    ..CompileOptions::default()
                },
            )
            .status
        });
        let mut session = zil_driver::CompilationSession::default();
        for (n, status) in [
            (whole, ResultCategory::Success),
            (whole - 1, ResultCategory::ResourceLimit),
            (whole, ResultCategory::Success),
        ] {
            let options = CompileOptions {
                stop_after: Stage::Verify,
                limits: limits(n),
                ..CompileOptions::default()
            };
            assert_eq!(session.compile_source(&input, &options).status, status);
            assert_eq!(
                zil_driver::CompilationSession::default()
                    .compile_source(&input, &options)
                    .status,
                status
            );
            assert_eq!(session.active_query_count(), 0);
        }
        boundaries.push([types, analyze, verify, decoded, whole]);
    }

    assert_eq!(
        boundaries,
        vec![
            [1702, 24787, 29335, 29335, 29335],
            [2326, 73489, 86110, 86110, 86110],
            [4229, 49818, 57237, 57237, 57237],
            [6250, 50740, 58972, 58972, 58972],
            [3576, 40233, 53885, 53885, 53885],
            [4788, 89149, 125858, 125858, 125858],
            [6481, 69191, 91202, 91202, 91202],
            [8988, 75237, 100190, 100190, 100190],
        ]
    );
    println!("array complete boundaries: {boundaries:?}");
    for kernel in 0..5 {
        for base in [0, 4] {
            for i in 1..4 {
                assert!(
                    boundaries[base + i][kernel] > boundaries[base][kernel],
                    "Array actual slots/pointee width/depth must cost more: {boundaries:?}"
                );
            }
        }
    }
}

#[test]
fn large_array_growth_keeps_the_unchanged_default_resource_boundary() {
    let elements = std::iter::repeat_n("pointer", 512)
        .collect::<Vec<_>>()
        .join(",");
    let program = format!(
        "fn main()->i32{{let value=7;let pointer=&value;let original=[{elements}];let copied=original;let selected=copied[511];return *selected-7}}"
    );
    let report = compile_source(
        &source("array-default-growth", &program),
        &CompileOptions {
            stop_after: Stage::Verify,
            ..CompileOptions::default()
        },
    );
    assert_eq!(
        report.status,
        ResultCategory::ResourceLimit,
        "{}",
        report.to_json()
    );
    assert!(
        report
            .diagnostics
            .iter()
            .all(|d| d.category == ResultCategory::ResourceLimit)
    );
    println!("512-slot Source growth: unchanged default ResourceLimit, no executable emitted");
}

#[test]
fn raw_and_wire_independently_reject_writing_a_live_array_window_source() {
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    for expression in ["&items", "identity(&items)", "&*(&items)", "&batch.items"] {
        let program = format!(
            "fn identity<life a,life b>(items:&'a [&'b i32;2])->&'a [&'b i32;2]{{return items}}fn main()->i32{{var second=9;var spare=0;let items=[&second,&second];let window={expression};spare=11;return *window[1]-9}}"
        );
        let program = if expression == "&batch.items" {
            program
                .replace(
                    "fn main()",
                    "struct Batch<life a>{items:[&'a i32;2]}fn make<life a>(items:[&'a i32;2])->Batch<'a>{return Batch::<'a>{items:items}}fn main()",
                )
                .replace("let window=", "let batch=make(items);let window=")
        } else {
            program
        };
        let mut raw = typed_source("array-window-source", &program, &target, &limits);
        let mut plan = zil_analyze::analyze(&raw, &limits).unwrap();
        zil_verify::verify_module(raw.clone(), plan.clone(), &target, &limits).unwrap();
        let main = raw.functions.iter_mut().find(|f| f.name == "main").unwrap();
        let second =
            zil_ir::LocalId(main.locals.iter().position(|l| l.name == "second").unwrap() as u32);
        let spare =
            zil_ir::LocalId(main.locals.iter().position(|l| l.name == "spare").unwrap() as u32);
        let store = main
            .blocks
            .iter_mut()
            .flat_map(|b| &mut b.instructions)
            .filter(|i| matches!(i.kind,zil_ir::InstructionKind::Store{local,..} if local==spare))
            .last()
            .unwrap();
        let zil_ir::InstructionKind::Store { local, .. } = &mut store.kind else {
            unreachable!()
        };
        *local = second;
        plan.module_fingerprint = raw.fingerprint();
        let analyzed = zil_analyze::analyze(&raw, &limits).unwrap_err();
        assert!(
            analyzed
                .iter()
                .any(|d| d.message.contains("actual stored target/parent loan")),
            "{expression}: {analyzed:?}"
        );
        let verified =
            zil_verify::verify_module(raw.clone(), plan.clone(), &target, &limits).unwrap_err();
        assert!(
            verified
                .iter()
                .any(|d| d.message.contains("live incompatible loan")),
            "{expression}: {verified:?}"
        );
        let wire = zil_ir::wire::encode_module_candidate(&raw, &plan, &limits).unwrap();
        let decoded = zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap_err();
        assert!(
            decoded
                .iter()
                .any(|d| d.message.contains("live incompatible loan")),
            "{expression}: {decoded:?}"
        );
    }
}

#[test]
fn raw_and_wire_reject_nonclosed_array_element_pointees() {
    let target = TargetProfile::host_llvm();
    let limits = Limits::default();
    let original = typed_source("array-finite-leaf", HELPER, &target, &limits);
    let plan = zil_analyze::analyze(&original, &limits).unwrap();
    for pointee in [
        Type::Slice {
            element: Box::new(Type::I32),
        },
        Type::FunctionPointer {
            parameters: vec![Type::I32],
            result: Box::new(Type::I32),
        },
    ] {
        let mut raw = original.clone();
        let main = raw.functions.iter_mut().find(|f| f.name == "main").unwrap();
        main.locals.push(zil_ir::LocalDecl {
            id: zil_ir::LocalId(main.locals.len() as u32),
            name: "unsupported_element_profile".into(),
            ty: Type::Array {
                element: Box::new(Type::Address {
                    mutable: false,
                    inner: Box::new(pointee),
                }),
                length: 2,
            },
            mutable: false,
            span: main.span,
        });
        let mut candidate = plan.clone();
        candidate.module_fingerprint = raw.fingerprint();
        let errors = zil_analyze::analyze(&raw, &limits).unwrap_err();
        assert!(
            errors.iter().any(|d| d
                .message
                .contains("shared array actual pointee is outside its finite profile")),
            "{errors:?}"
        );
        let errors = zil_verify::verify_module(raw.clone(), candidate.clone(), &target, &limits)
            .unwrap_err();
        assert!(
            errors.iter().any(|d| d
                .message
                .contains("shared array actual pointee is outside its finite profile")),
            "{errors:?}"
        );
        let wire = zil_ir::wire::encode_module_candidate(&raw, &candidate, &limits).unwrap();
        let errors = zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap_err();
        assert!(
            errors.iter().any(|d| d
                .message
                .contains("shared array actual pointee is outside its finite profile")),
            "{errors:?}"
        );
    }
}

#[test]
fn array_billing_mode_is_consistent_across_scalar_reuse_and_function_order() {
    let scalar_before = "fn before(x:i32)->i32{if x<10{return x+1}else{return x*2}}";
    let scalar_after = "fn after(x:i32)->i32{if x<10{return x+2}else{return x*3}}";
    let array =
        "fn array_value()->i32{let n=7;let items=[&n,&n];let window=&items;return *window[1]}";
    let scalar = "fn array_value()->i32{if true{return 7}else{return 0}}";
    for (label, first, middle, last, expected_reuse) in [
        ("scalar", scalar_before, scalar, scalar_after, 3),
        ("array-middle", scalar_before, array, scalar_after, 1),
        ("array-first", array, scalar_before, scalar_after, 0),
    ] {
        let dir =
            std::env::temp_dir().join(format!("zil-array-reuse-{}-{label}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("main.zil");
        let make_project = |literal| {
            let text = format!(
                "{first}{middle}{last}fn main()->i32{{if true{{return before(1)+array_value()+after(1)-{literal}}}else{{return 0}}}}"
            );
            std::fs::write(&path, &text).unwrap();
            zil_driver::ProjectSources {
                root: ModuleId(0),
                modules: vec![SourceUnit::new(
                    FileId(0),
                    ModuleId(0),
                    path.to_string_lossy(),
                    text,
                )],
                paths: Default::default(),
            }
        };
        let initial = make_project(12);
        let changed = make_project(13);
        let options = CompileOptions {
            stop_after: Stage::Verify,
            ..CompileOptions::default()
        };
        let mut session = zil_driver::CompilationSession::default();
        assert_eq!(
            session.compile_project(&initial, &options).status,
            ResultCategory::Success
        );
        let warm = session.compile_project(&changed, &options);
        assert_eq!(warm.status, ResultCategory::Success, "{}", warm.to_json());
        let stats = session.function_type_stats();
        assert_eq!(stats.reused_functions, expected_reuse, "{label}: {stats:?}");
        let fresh = zil_driver::CompilationSession::default().compile_project(&changed, &options);
        assert_eq!(warm.status, fresh.status);
        assert_eq!(warm.diagnostics, fresh.diagnostics);
        assert_eq!(warm.scalar_interfaces, fresh.scalar_interfaces);
        let n = exact_array_budget(&format!("Array billing project {label}"), |n| {
            zil_driver::CompilationSession::default()
                .compile_project(
                    &changed,
                    &CompileOptions {
                        limits: Limits {
                            max_analysis_steps: n,
                            ..Limits::default()
                        },
                        ..options.clone()
                    },
                )
                .status
        });
        assert_eq!(n, if label == "scalar" { 1543 } else { 16587 });
        let mut warm = zil_driver::CompilationSession::default();
        for (limit, status) in [
            (n, ResultCategory::Success),
            (n - 1, ResultCategory::ResourceLimit),
            (n, ResultCategory::Success),
        ] {
            let report = warm.compile_project(
                &changed,
                &CompileOptions {
                    limits: Limits {
                        max_analysis_steps: limit,
                        ..Limits::default()
                    },
                    ..options.clone()
                },
            );
            assert_eq!(report.status, status, "{label}: {}", report.to_json());
            assert_eq!(warm.active_query_count(), 0);
        }
        println!(
            "{label}: actual scalar function reuse {stats:?}, exact N={n}, cold/warm/fresh agree"
        );
        std::fs::remove_dir_all(dir).unwrap();
    }
}

#[test]
fn ordinary_array_assignment_and_if_results_keep_actual_elements() {
    let program = "fn relay<life a>(items:[&'a i32;2],flag:bool)->[&'a i32;2]{var copied=items;copied=if flag{items}else{items};return copied}fn main()->i32{let first=7;let second=9;let values=relay([&first,&second],first==7);return *values[1]-9}";
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    let report = compile_source(
        &source("array-ordinary-assignment", program),
        &CompileOptions {
            stop_after: Stage::Verify,
            ..CompileOptions::default()
        },
    );
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    let raw = typed_source("array-ordinary-assignment", program, &target, &limits);
    let plan = zil_analyze::analyze(&raw, &limits).unwrap();
    zil_verify::verify_module(raw.clone(), plan.clone(), &target, &limits).unwrap();
    let wire = zil_ir::wire::encode_module_candidate(&raw, &plan, &limits).unwrap();
    zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap();
}

#[test]
fn whole_array_reference_patterns_preserve_real_sources() {
    for body in [
        "let ref whole=items;return *whole[1]-9",
        "match items{ref whole=>{return *whole[1]-9}}",
        "let window=&items;match *window{ref whole=>{return *whole[1]-9}}",
    ] {
        let program =
            format!("fn main()->i32{{var first=7;var second=9;let items=[&first,&second];{body}}}");
        let limits = Limits::default();
        let target = TargetProfile::host_llvm();
        let report = compile_source(
            &source("array-reference-pattern", &program),
            &CompileOptions {
                stop_after: Stage::Verify,
                ..CompileOptions::default()
            },
        );
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        let raw = typed_source("array-reference-pattern", &program, &target, &limits);
        let plan = zil_analyze::analyze(&raw, &limits).unwrap();
        zil_verify::verify_module(raw.clone(), plan.clone(), &target, &limits).unwrap();
        let wire = zil_ir::wire::encode_module_candidate(&raw, &plan, &limits).unwrap();
        zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap();
        let invalid = program.replace("return *whole[1]-9", "second=11;return *whole[1]-9");
        let report = compile_source(
            &source("array-reference-source-write", &invalid),
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
    }
    let program = "struct Batch<life a>{items:[&'a i32;2]}fn read<life a>(first:&'a i32,second:&'a i32)->i32{let ref whole=Batch::<'a>{items:[first,second]};return *whole.items[1]}fn main()->i32{let first=7;let second=9;return read(&first,&second)-9}";
    let report = compile_source(
        &source("array-reference-temporary", program),
        &CompileOptions {
            stop_after: Stage::Verify,
            ..CompileOptions::default()
        },
    );
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
}

#[test]
fn array_outer_scope_and_whole_reference_snapshot_costs_grow_before_cloning() {
    let default = Limits::default();
    let target = TargetProfile::host_llvm();
    let mut boundaries = Vec::new();
    for (length, width, depth) in [(2, 1, 0), (8, 8, 1)] {
        let fields = (0..width)
            .map(|i| format!("v{i}:i32"))
            .collect::<Vec<_>>()
            .join(";");
        let values = (0..width)
            .map(|i| format!("v{i}:7"))
            .collect::<Vec<_>>()
            .join(",");
        let mut definitions = format!("struct Leaf{{{fields}}}");
        let mut ty = "Leaf".to_string();
        let mut value = format!("Leaf{{{values}}}");
        let mut path = String::new();
        for level in 0..depth {
            let next = format!("Layer{level}");
            definitions.push_str(&format!("struct {next}{{inner:{ty}}}"));
            value = format!("{next}{{inner:{value}}}");
            ty = next;
            path.push_str(".inner");
        }
        let elements = std::iter::repeat_n("pointer", length)
            .collect::<Vec<_>>()
            .join(",");
        // All headers stay scalar. Array creation in the first ordinary
        // function enables the shared request ledger; the following closure
        // has only a POD outer scope and captures nothing.
        let program = format!(
            "{definitions}fn array_before()->i32{{let value={value};let pointer=&value;let items=[{elements}];let ref window=items;let selected=window[0];return selected{path}.v0}}fn scoped()->i32{{let value={value};let unused=()=>0;return value{path}.v0}}fn main()->i32{{return 0}}"
        );
        let input = source("array-outer-snapshot", &program);
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
        let n = exact_array_budget(
            &format!(
                "Array whole reference and uncaptured outer scope len={length} width={width} depth={depth}"
            ),
            |n| {
                array_budget_category(zil_types::check(
                    &core,
                    &target,
                    &Limits {
                        max_analysis_steps: n,
                        ..default.clone()
                    },
                ))
            },
        );
        boundaries.push(n);
    }
    assert!(boundaries[1] > boundaries[0]);
    println!("Array auxiliary Types snapshot boundaries: {boundaries:?}");
}

#[test]
fn array_closure_inference_keeps_its_existing_boundary() {
    for program in [
        "fn main()->i32{let n=7;let items=[&n,&n];let unused=()=>0;return *items[1]-7}",
        "struct Batch<life a>{items:[&'a i32;2]}fn inspect<life a>(pointer:&'a i32)->i32{let items=Batch::<'a>{items:[pointer,pointer]};let unused=()=>0;return *items.items[1]}fn main()->i32{return 0}",
    ] {
        let report = compile_source(
            &source("array-closure-frontier", program),
            &CompileOptions {
                stop_after: Stage::Types,
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
                .any(|d| d.message.contains("shared address and scalar types differ")),
            "{}",
            report.to_json()
        );
    }
}
