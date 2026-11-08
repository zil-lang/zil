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
    println!(
        "array raw/Wire: wrong actual ordinal, out-of-range, missing leaf, false root, metadata-only Element rejected"
    );
}

fn array_budget_model(length: usize, width: usize, depth: usize) -> String {
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
    text.push_str(&format!("fn make<life a>(pointer:&'a {ty})->[&'a {ty};{length}]{{return [{elements}]}}fn relay<life a>(items:[&'a {ty};{length}])->[&'a {ty};{length}]{{let copied=items;return copied}}fn read<life a>(items:&[&'a {ty};{length}])->i32{{let pointer=items[{}];return {access}.v0}}fn main()->i32{{let value={value};let items=relay(make(&value));let window=&items;let copied=window;return read(copied)-7}}",length-1));
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
    for (length, width, depth) in [(2, 1, 0), (8, 1, 0), (2, 8, 0), (2, 1, 3)] {
        let program = array_budget_model(length, width, depth);
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
        let label = format!("Array len={length} width={width} depth={depth}");
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
    println!("array complete boundaries: {boundaries:?}");
    for kernel in 0..5 {
        for i in 1..4 {
            assert!(
                boundaries[i][kernel] > boundaries[0][kernel],
                "Array actual slots/pointee width/depth must cost more: {boundaries:?}"
            );
        }
    }
}

#[test]
fn large_array_physical_snapshots_preserve_shared_values() {
    let elements = std::iter::repeat_n("pointer", 512)
        .collect::<Vec<_>>()
        .join(",");
    let program = format!(
        "fn main()->i32{{let value=7;let pointer=&value;let original=[{elements}];let copied=original;let pointer=copied[511];return *pointer-7}}"
    );
    for backend in [NativeBackend::Clang, NativeBackend::LlvmApi] {
        for optimization in [Optimization::O0, Optimization::O2] {
            let dir = std::env::temp_dir().join(format!(
                "zil-large-address-array-{}-{backend:?}-{optimization:?}",
                std::process::id()
            ));
            std::fs::create_dir_all(&dir).unwrap();
            let executable = dir.join("program");
            let report = compile_source(
                &source("large-array-snapshot", &program),
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
                "{backend:?}/{optimization:?}: {}",
                report.to_json()
            );
            assert_eq!(report.verification_runs, 1);
            let actual = report.native_codegen.as_ref().unwrap();
            assert_eq!(actual.backend, backend);
            assert!(actual.fallback.is_none());
            let output = std::process::Command::new(executable).output().unwrap();
            assert_eq!(output.status.code(), Some(0), "{output:?}");
            assert!(output.stdout.is_empty() && output.stderr.is_empty());
            std::fs::remove_dir_all(dir).unwrap();
        }
    }
}

#[test]
fn raw_and_wire_independently_reject_writing_a_live_array_window_source(){
    let limits=Limits::default();let target=TargetProfile::host_llvm();
    for expression in ["&items", "identity(&items)", "&*(&items)"]{
        let program=format!("fn identity<life a,life b>(items:&'a [&'b i32;2])->&'a [&'b i32;2]{{return items}}fn main()->i32{{var second=9;var spare=0;let items=[&second,&second];let window={expression};spare=11;return *window[1]-9}}");
        let mut raw=typed_source("array-window-source",&program,&target,&limits);let mut plan=zil_analyze::analyze(&raw,&limits).unwrap();
        zil_verify::verify_module(raw.clone(),plan.clone(),&target,&limits).unwrap();
        let main=raw.functions.iter_mut().find(|f|f.name=="main").unwrap();
        let second=zil_ir::LocalId(main.locals.iter().position(|l|l.name=="second").unwrap() as u32);let spare=zil_ir::LocalId(main.locals.iter().position(|l|l.name=="spare").unwrap() as u32);
        let store=main.blocks.iter_mut().flat_map(|b| &mut b.instructions).filter(|i|matches!(i.kind,zil_ir::InstructionKind::Store{local,..} if local==spare)).last().unwrap();
        let zil_ir::InstructionKind::Store{local,..}=&mut store.kind else{unreachable!()};*local=second;
        plan.module_fingerprint=raw.fingerprint();
        assert!(zil_analyze::analyze(&raw,&limits).is_err(),"{expression}: Analyze");assert!(zil_verify::verify_module(raw.clone(),plan.clone(),&target,&limits).is_err(),"{expression}: Verify");
        let wire=zil_ir::wire::encode_module_candidate(&raw,&plan,&limits).unwrap();assert!(zil_verify::verify_module_bytes(&wire,&target,&limits).is_err(),"{expression}: Wire");
    }
}
