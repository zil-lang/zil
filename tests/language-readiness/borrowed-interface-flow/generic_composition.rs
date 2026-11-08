//! Ordinary lending helpers retain independent type and life binders.
use super::*;

const HELPER: &str = "import api;pub fn next_tagged<life a,I:api.LendingIterator+api.Tag,T:Copy>(cursor:&'a mut I,tag:T)->Option<&'a I.Item>{let kind=cursor.tag();let copied=tag;let again=tag;return cursor.next()}";

fn declarations() -> (String, String) {
    let api = format!("{API}\npub trait Tag{{fn tag(self:&Self)->i32;}}");
    let cursor = format!(
        "{CURSOR}\nimpl<T:Copy> api.Tag for ArrayCursor<T>{{fn tag(self:&Self)->i32{{return 7}}}}"
    );
    (api, cursor)
}

fn composition(consumer: &str, main: &str) -> zil_driver::CompileReport {
    let (api, cursor) = declarations();
    source_report(main, &api, &cursor, consumer, OTHER)
}

const COMPOSED_MAIN: &str = "import api;import cursor;import consumer;fn main()->i32{var c=cursor.make(20,22);var sum=0;match consumer.next_tagged(&mut c,7){Option.Some(v)=>{sum+=*v},Option.None()=>{return 1}}match consumer.next_tagged(&mut c,true){Option.Some(v)=>{sum+=*v},Option.None()=>{return 2}}return sum}";

#[test]
fn ordinary_extra_copy_binder_and_multiple_nominal_bounds() {
    let report = composition(HELPER, COMPOSED_MAIN);
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1);
    let shared_spelling = HELPER.replace("life a", "life T").replace("'a", "'T");
    let report = composition(&shared_spelling, COMPOSED_MAIN);
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1);
    let explicit = COMPOSED_MAIN.replace(
        "consumer.next_tagged(&mut c,7)",
        "consumer.next_tagged::<cursor.ArrayCursor<i32>,i32>(&mut c,7)",
    );
    let report = composition(HELPER, &explicit);
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1);
    let reversed = HELPER.replace("api.LendingIterator+api.Tag", "api.Tag+api.LendingIterator");
    let report = composition(&reversed, COMPOSED_MAIN);
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1);
    let reordered = HELPER.replace(
        "life a,I:api.LendingIterator+api.Tag,T:Copy",
        "T:Copy,life a,I:api.LendingIterator+api.Tag",
    );
    let report = composition(&reordered, COMPOSED_MAIN);
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1);
}

#[test]
fn unused_composed_helper_cannot_merge_distinct_life_sources() {
    let helper = "import api;pub fn next_tagged<life a,life b,I:api.LendingIterator+api.Tag,T:Copy>(cursor:&'b mut I,tag:T,wrong:&'a I.Item)->Option<&'a I.Item>{return cursor.next()}";
    let good = composition(HELPER, "fn main(){}");
    assert_eq!(good.status, ResultCategory::Success, "{}", good.to_json());
    let report = composition(helper, "fn main(){}");
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
            .any(|d| d.code == "E-GENERIC-ARGUMENT"
                && d.message.contains("distinct declared lifetime classes")),
        "{}",
        report.to_json()
    );
}

#[test]
fn composed_helper_live_result_keeps_the_actual_receiver_loan() {
    let good = composition(HELPER, COMPOSED_MAIN);
    assert_eq!(good.status, ResultCategory::Success, "{}", good.to_json());
    let main = "import api;import cursor;import consumer;fn main()->i32{var c=cursor.make(20,22);let held=consumer.next_tagged(&mut c,7);let other=consumer.next_tagged(&mut c,8);match held{Option.Some(v)=>{return *v},Option.None()=>{return 1}}}";
    let report = composition(HELPER, main);
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
    assert_eq!(report.phase, Phase::Analyze, "{}", report.to_json());
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.message.contains("actual stored target/parent loan")),
        "{}",
        report.to_json()
    );
}

#[test]
fn composed_helper_requires_every_definition_and_instance_bound() {
    let good = composition(HELPER, COMPOSED_MAIN);
    assert_eq!(good.status, ResultCategory::Success, "{}", good.to_json());
    let missing_copy = HELPER.replace("T:Copy", "T:api.Tag");
    let report = composition(&missing_copy, "fn main(){}");
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
    assert_eq!(report.phase, Phase::Expand, "{}", report.to_json());
    let missing_nominal = HELPER.replace("api.LendingIterator+api.Tag", "api.LendingIterator");
    let report = composition(&missing_nominal, "fn main(){}");
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
    assert_eq!(report.phase, Phase::Expand, "{}", report.to_json());
    let (api, cursor) = declarations();
    let report = source_report(COMPOSED_MAIN, &api, CURSOR, HELPER, OTHER);
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
    assert_eq!(report.phase, Phase::Expand, "{}", report.to_json());
    let ambiguous = api.replace(
        "fn tag(self:&Self)->i32;",
        "fn tag(self:&Self)->i32;fn next<life x>(self:&'x mut Self)->Option<&'x i32>;",
    );
    let report = source_report("fn main(){}", &ambiguous, &cursor, HELPER, OTHER);
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
    assert_eq!(report.phase, Phase::Expand, "{}", report.to_json());
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.message.contains("ambiguous")),
        "{}",
        report.to_json()
    );
    let ambiguous_item = format!("{api}\npub trait Alternate{{type Item:Copy;}}");
    let helper = HELPER.replace("api.Tag", "api.Tag+api.Alternate");
    let report = source_report("fn main(){}", &ambiguous_item, &cursor, &helper, OTHER);
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
    assert_eq!(report.phase, Phase::Expand, "{}", report.to_json());
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.message.contains("ambiguous")),
        "{}",
        report.to_json()
    );
}

#[test]
fn composed_helper_executes_on_both_native_backends() {
    let (api, cursor) = declarations();
    let mut backends = vec![NativeBackend::Clang];
    if cfg!(feature = "llvm-api") {
        backends.push(NativeBackend::LlvmApi);
    }
    for backend in backends {
        for optimization in [Optimization::O0, Optimization::O2] {
            let dir = std::env::temp_dir().join(format!(
                "lending-generic-native-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&dir).unwrap();
            let executable = dir.join("program");
            let report = compile_project(
                &project(&files(COMPOSED_MAIN, &api, &cursor, HELPER, OTHER)),
                &CompileOptions {
                    backend,
                    optimization,
                    stop_after: Stage::Build,
                    entry: Some("main".into()),
                    output: Some(executable.clone()),
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
            let codegen = report.native_codegen.as_ref().unwrap();
            assert_eq!(codegen.backend, backend);
            assert!(codegen.fallback.is_none());
            if backend == NativeBackend::LlvmApi {
                assert!(
                    codegen
                        .object_reports
                        .iter()
                        .all(|row| row.contains("\"input_kind\":\"builder\""))
                );
            }
            let output = Command::new(&executable).output().unwrap();
            assert_eq!(output.status.code(), Some(0), "{output:?}");
            assert_eq!(output.stdout, b"42\n", "{output:?}");
            assert!(output.stderr.is_empty(), "{output:?}");
            fs::remove_dir_all(dir).unwrap();
        }
    }
}

fn raw_source() -> String {
    let (api, cursor) = declarations();
    format!(
        "{}\n{}\n{}\nfn main()->i32{{var first=make(20,22);var second=make(3,4);let held=next_tagged(&mut first,7);let other=next_tagged(&mut second,8);match held{{Option.Some(a)=>{{match other{{Option.Some(b)=>{{return *a+*b}},Option.None()=>{{return 1}}}}}},Option.None()=>{{return 2}}}}}}",
        api,
        cursor.replace("import api", "").replace("api.", ""),
        HELPER.replace("import api;", "").replace("api.", "")
    )
}

#[test]
fn composed_helper_class_scans_have_exact_fresh_expand_budget() {
    use zil_base::{FileId, Limits, ModuleId, SourceUnit};
    let unit = SourceUnit::new(
        FileId(0),
        ModuleId(0),
        "lending-generic-budget.zil",
        raw_source(),
    );
    let ast = zil_syntax::parse(&unit).into_result().unwrap();
    let defaults = Limits::default();
    let expand = |work| {
        zil_expand::expand_with_map_limits(
            &unit,
            &ast,
            &Limits {
                max_analysis_steps: work,
                ..defaults.clone()
            },
        )
    };
    let expected = expand(defaults.max_analysis_steps).unwrap().core;
    let (mut low, mut high) = (1, defaults.max_analysis_steps);
    while low < high {
        let middle = low + (high - low) / 2;
        match expand(middle) {
            Ok(_) => high = middle,
            Err(errors)
                if errors
                    .iter()
                    .any(|d| d.category == ResultCategory::ResourceLimit) =>
            {
                low = middle + 1
            }
            Err(errors) => panic!("unexpected Expand result: {errors:?}"),
        }
    }
    assert!(low > 1);
    for (work, success) in [(low, true), (low - 1, false), (low, true)] {
        match expand(work) {
            Ok(expanded) if success => assert_eq!(expanded.core, expected),
            Err(errors) if !success => assert!(
                errors.iter().any(
                    |d| d.phase == Phase::Expand && d.category == ResultCategory::ResourceLimit
                ),
                "{errors:?}"
            ),
            other => panic!("unexpected Expand boundary {work}: {other:?}"),
        }
    }
    eprintln!(
        "ordinary lending composition Expand work N={low}; N-1 ResourceLimit; fresh Core equality"
    );
}

#[test]
fn composed_helper_actual_receiver_is_independently_checked_in_raw_and_wire() {
    use zil_base::{FileId, Limits, ModuleId, SourceUnit, TargetProfile};
    use zil_ir::InstructionKind;
    let text = raw_source();
    let unit = SourceUnit::new(
        FileId(0),
        ModuleId(0),
        "lending-generic-composition.zil",
        text.clone(),
    );
    let ast = zil_syntax::parse(&unit).into_result().unwrap();
    let core = zil_expand::expand(&unit, &ast).unwrap();
    let mut module =
        zil_types::check(&core, &TargetProfile::host_llvm(), &Limits::default()).unwrap();
    let mut plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    zil_verify::verify_module(
        module.clone(),
        plan.clone(),
        &module.target,
        &Limits::default(),
    )
    .unwrap();
    let main = module
        .functions
        .iter()
        .position(|f| f.name == "main")
        .unwrap();
    let text_ref = text.as_str();
    let calls = module.functions[main]
        .blocks
        .iter()
        .enumerate()
        .flat_map(|(block, b)| {
            b.instructions
                .iter()
                .enumerate()
                .filter_map(move |(index, i)| match &i.kind {
                    InstructionKind::Call {
                        function,
                        arguments,
                    } if arguments.len() == 2
                        && text_ref
                            .get(i.span.start..i.span.end)
                            .is_some_and(|s| s.starts_with("next_tagged(")) =>
                    {
                        Some((block, index, *function, arguments[0], i.span))
                    }
                    _ => None,
                })
        })
        .collect::<Vec<_>>();
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0].2, calls[1].2);
    for (call, marker) in calls
        .iter()
        .zip(["next_tagged(&mut first", "next_tagged(&mut second"])
    {
        assert_eq!(call.4.start, text.find(marker).unwrap());
    }
    let InstructionKind::Call { arguments, .. } =
        &mut module.functions[main].blocks[calls[1].0].instructions[calls[1].1].kind
    else {
        unreachable!()
    };
    arguments[0] = calls[0].3;
    let errors = zil_analyze::analyze(&module, &Limits::default()).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|d| d.phase == Phase::Analyze && d.category == ResultCategory::SafeReject),
        "{errors:?}"
    );
    plan.module_fingerprint = module.fingerprint();
    let wire = zil_ir::wire::encode_module_candidate(&module, &plan, &Limits::default()).unwrap();
    assert_eq!(
        zil_ir::wire::decode_module_candidate(&wire, &Limits::default()).unwrap(),
        (module.clone(), plan.clone())
    );
    for errors in [
        zil_verify::verify_module(module.clone(), plan, &module.target, &Limits::default())
            .unwrap_err(),
        zil_verify::verify_module_bytes(&wire, &module.target, &Limits::default()).unwrap_err(),
    ] {
        assert!(
            errors.iter().any(|d| d.phase == Phase::Verify
                && d.category == ResultCategory::SafeReject
                && d.message.contains("live incompatible loan")),
            "{errors:?}"
        );
    }
}
