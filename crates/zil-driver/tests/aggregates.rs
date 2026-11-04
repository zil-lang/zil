//! Real Copy aggregate programs, LLVM bounds checks, and forged raw controls.
use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};
use zil_base::{FileId, Limits, ModuleId, Phase, ResultCategory, SourceUnit, TargetProfile};
use zil_driver::{CompileOptions, Stage, compile_source};
use zil_ir::{InstructionKind, Type};
static NEXT: AtomicU64 = AtomicU64::new(0);

#[test]
fn terminal340_fnptr_record_enum_result_inference() {
    let text = "enum E{Data{n:i32},Empty}fn keep(x:E)->E{return x}fn result()->i32{let pass:fn(E)->E=keep;let value=pass(E.Data{n:42});match value{E.Data{n:x}=>{return x},E.Empty()=>{return 0}}}";
    review329_native(&[("main", text)], 42);
    let wrong = text.replace("E.Data{n:x}", "Other.Data{n:x}");
    let report = compile_source(&source(&wrong), &CompileOptions::default());
    assert_ne!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );

    let input = source(text);
    let syntax = zil_syntax::parse(&input).into_result().unwrap();
    let core = zil_expand::expand(&input, &syntax).unwrap();
    let target = TargetProfile::default();
    let limits = Limits::default();
    let typed = zil_types::check(&core, &target, &limits).unwrap();
    let plan = zil_analyze::analyze(&typed, &limits).unwrap();
    let wire = zil_ir::wire::encode_module_candidate(&typed, &plan, &limits).unwrap();
    assert_eq!(
        zil_verify::verify_module_bytes(&wire, &target, &limits)
            .unwrap()
            .module(),
        &typed
    );

    let original = include_str!("../../../tests/language-readiness/batch336/component.zil");
    let report = compile_source(&source(original), &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
}

#[test]
fn terminal340_unsigned_native_and_ct() {
    let text = include_str!("../../../tests/language-readiness/terminal/340/unsigned.zil");
    review329_native(&[("main", text)], 42);
    for text in [
        "fn result()->i32{let x:u64=18446744073709551616u64;return 0}",
        "fn result()->i32{let x:u64=-1u64;return 0}",
        "fn result()->i32{let x:u64=1usize;return 0}",
        "fn unused(x:usize)->u64{return x}fn result()->i32{return 0}",
        "fn result()->i32{let x:u64=#(18446744073709551615u64+1u64);return 0}",
    ] {
        let report = compile_source(&source(text), &CompileOptions::default());
        assert_ne!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
    }
    for text in [
        "fn result()->i32{let x=18446744073709551615u64;let y=x+1u64;return 0}",
        "fn result()->i32{let x=0u64;let y=x-1u64;return 0}",
        "fn result()->i32{let x=18446744073709551615u64;let y=i64(x);return 0}",
        "fn result()->i32{let x:i64=-1;let y=usize(x);return 0}",
        "fn result()->i32{let x=1usize;let y=x<<64usize;return 0}",
    ] {
        native(text, None);
    }
}

#[test]
fn terminal340_unsigned_target_width_raw_wire_and_type_ct() {
    let text = "fn select()->Type{let high=0usize-%1usize;if high>4294967295usize{return bool}return usize}fn result()->i32{let value:#(select())=7;return 0}";
    let mut target = TargetProfile::default();
    target.id = "terminal340-32".into();
    target.triple = "i686-unknown-linux-gnu".into();
    target.pointer_width = 32;
    let report = compile_source(
        &source(text),
        &CompileOptions {
            target: target.clone(),
            ..CompileOptions::default()
        },
    );
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    let wide = compile_source(&source(text), &CompileOptions::default());
    assert_eq!(
        wide.status,
        ResultCategory::SafeReject,
        "{}",
        wide.to_json()
    );

    let limits = Limits::default();
    let input = source("fn result()->u64{return 18446744073709551615u64}");
    let syntax = zil_syntax::parse(&input).into_result().unwrap();
    let core = zil_expand::expand(&input, &syntax).unwrap();
    let typed = zil_types::check(&core, &TargetProfile::default(), &limits).unwrap();
    let plan = zil_analyze::analyze(&typed, &limits).unwrap();
    let bytes = zil_ir::wire::encode_module_candidate(&typed, &plan, &limits).unwrap();
    assert_eq!(
        zil_verify::verify_module_bytes(&bytes, &typed.target, &limits)
            .unwrap()
            .module(),
        &typed
    );
    let mut broken = typed.clone();
    broken.functions[0].return_type = Type::Usize;
    let mut claim = plan.clone();
    claim.module_fingerprint = broken.fingerprint();
    assert!(
        zil_verify::verify_module(broken.clone(), claim.clone(), &typed.target, &limits).is_err()
    );
    let bytes = zil_ir::wire::encode_module_candidate(&broken, &claim, &limits).unwrap();
    assert!(zil_verify::verify_module_bytes(&bytes, &typed.target, &limits).is_err());
}

#[test]
fn terminal340_unsigned_budget_boundaries() {
    use zil_driver::CompilationSession;
    let input = source(include_str!(
        "../../../tests/language-readiness/terminal/340/unsigned.zil"
    ));
    let options = |work| CompileOptions {
        limits: Limits {
            max_analysis_steps: work,
            ..Limits::default()
        },
        ..CompileOptions::default()
    };
    let maximum = Limits::default().max_analysis_steps;
    let mut low = 1;
    let mut high = maximum;
    while low < high {
        let middle = low + (high - low) / 2;
        match compile_source(&input, &options(middle)).status {
            ResultCategory::Success => high = middle,
            ResultCategory::ResourceLimit => low = middle + 1,
            status => panic!("unexpected budget result {status:?}"),
        }
    }
    let mut warm = CompilationSession::default();
    for (work, expected) in [
        (low, ResultCategory::Success),
        (low - 1, ResultCategory::ResourceLimit),
        (low, ResultCategory::Success),
    ] {
        for report in [
            warm.compile_source(&input, &options(work)),
            CompilationSession::default().compile_source(&input, &options(work)),
        ] {
            assert_eq!(report.status, expected, "{}", report.to_json());
        }
        assert_eq!(warm.active_query_count(), 0);
    }
    println!("340 unsigned full-request analysis N={low}, N-1 rejects, fresh/warm agree");
}

#[test]
fn terminal340_option_budget_boundaries() {
    use zil_driver::CompilationSession;
    let input = source(include_str!(
        "../../../tests/language-readiness/terminal/340/option.zil"
    ));
    let options = |work| CompileOptions {
        limits: Limits {
            max_analysis_steps: work,
            ..Limits::default()
        },
        ..CompileOptions::default()
    };
    let maximum = Limits::default().max_analysis_steps;
    let mut low = 1;
    let mut high = maximum;
    while low < high {
        let middle = low + (high - low) / 2;
        match compile_source(&input, &options(middle)).status {
            ResultCategory::Success => high = middle,
            ResultCategory::ResourceLimit => low = middle + 1,
            status => panic!("unexpected budget result {status:?}"),
        }
    }
    let mut warm = CompilationSession::default();
    for (work, expected) in [
        (low, ResultCategory::Success),
        (low - 1, ResultCategory::ResourceLimit),
        (low, ResultCategory::Success),
    ] {
        for report in [
            warm.compile_source(&input, &options(work)),
            CompilationSession::default().compile_source(&input, &options(work)),
        ] {
            assert_eq!(report.status, expected, "{}", report.to_json());
        }
        assert_eq!(warm.active_query_count(), 0);
    }
    println!("340 Option full-request analysis N={low}, N-1 rejects, fresh/warm agree");
}

#[test]
fn terminal340_option_copy_and_owned_native() {
    for text in [
        "fn result()->i32{let value:i32?=Option.Some(42);match value{Option.Some(n)=>{return n},Option.None()=>{return 0}}}",
        "fn make()->Option<i32>{return Option.Some(42)}fn result()->i32{let value=make();match value{Option.Some(n)=>{return n},Option.None()=>{return 0}}}",
        "fn result()->i32{let value:i32?=Option.None;match value{Option.Some(_)=>{return 0},Option.None()=>{return 42}}}",
        "package P{manages data:ByteBuffer;n:i32}fn result()->i32{let value:P?=Option.Some(P{data:bytes_new(),n:42});match value{Option.Some(p)=>{return p.n},Option.None()=>{return 0}}}",
    ] {
        review329_native(&[("main", text)], 42);
    }
}

#[test]
fn terminal340_option_source_identity_and_negative_neighbors() {
    review329_native(
        &[(
            "main",
            include_str!("../../../tests/language-readiness/terminal/340/option.zil"),
        )],
        42,
    );
    review329_native(
        &[
            (
                "main",
                "import model;fn result()->i32{let value:i32?=model.make();match value{Option.Some(n)=>{return n},Option.None()=>{return 0}}}",
            ),
            (
                "model",
                "pub fn make()->Option<i32>{return Option.Some(42)}",
            ),
        ],
        42,
    );
    for text in [
        "enum Option{None,Some(i32)}fn result()->i32{return 0}",
        "type Option=i32;fn result()->i32{return 0}",
        "fn unused<Option>(x:Option)->Option{return x}fn result()->i32{return 0}",
        "fn result()->i32{let Option=1;return 0}",
        "fn result()->i32{let value:i32?=42;return 0}",
        "fn result()->i32{let value=Option.None;return 0}",
        "fn result()->i32{let value:i32?=Option.Some(true);return 0}",
        "fn result()->i32{let value:i32?=Option.None;match value{Option.Some(n)=>{return n}}}",
        "fn unused(value:i32?)->bool?{return value}fn result()->i32{return 0}",
        "package P{manages data:ByteBuffer;n:i32}fn result()->i32{let value:P?=Option.Some(P{data:bytes_new(),n:42});match value{Option.Some(p)=>{let other=p;return p.n},Option.None()=>{return 0}}}",
    ] {
        let report = compile_source(&source(text), &CompileOptions::default());
        assert_ne!(
            report.status,
            ResultCategory::Success,
            "unexpected accepted neighbor: {text}\n{}",
            report.to_json()
        );
    }
    let limits = Limits::default();
    let target = TargetProfile::default();
    let input = source("fn unused(value:i32?)->i32{return 0}fn result()->i32{return 42}");
    let syntax = zil_syntax::parse(&input).into_result().unwrap();
    let core = zil_expand::expand(&input, &syntax).unwrap();
    let typed = zil_types::check(&core, &target, &limits).unwrap();
    let plan = zil_analyze::analyze(&typed, &limits).unwrap();
    let bytes = zil_ir::wire::encode_module_candidate(&typed, &plan, &limits).unwrap();
    assert_eq!(
        zil_verify::verify_module_bytes(&bytes, &target, &limits)
            .unwrap()
            .module(),
        &typed
    );
    for malformed in [false, true] {
        let mut raw = typed.clone();
        let function = raw
            .functions
            .iter_mut()
            .find(|f| f.name == "unused")
            .unwrap();
        let parameter = function.parameters[0];
        let Type::Enum { name, variants } = &mut function.locals[parameter.0 as usize].ty else {
            panic!("Option must be a hygienic Enum")
        };
        if malformed {
            variants[1].1.clear()
        } else {
            name.push('0')
        }
        let mut claim = plan.clone();
        claim.module_fingerprint = raw.fingerprint();
        let errors =
            zil_verify::verify_module(raw.clone(), claim.clone(), &target, &limits).unwrap_err();
        assert!(
            errors.iter().any(|e| e.code == "E-VERIFY-OPTION"),
            "{errors:?}"
        );
        let bytes = zil_ir::wire::encode_module_candidate(&raw, &claim, &limits).unwrap();
        assert!(zil_verify::verify_module_bytes(&bytes, &target, &limits).is_err());
    }
}

#[test]
fn terminal340_reference_places_native() {
    for text in [
        "fn result()->i32{let n=42;let ref p=n;return *p}",
        "fn result()->i32{var n=20;let ref mut p=n;*p+=22;return n}",
        "fn result()->i32{var pair=(20,22);let(ref x,ref y)=pair;return *x+*y}",
        "fn result()->i32{let value:i32?=Option.Some(42);match value{Option.Some(ref p)=>{return *p},Option.None()=>{return 0}}}",
        "fn result()->i32{var value:i32?=Option.Some(20);match value{Option.Some(ref mut p)=>{*p+=22;return *p},Option.None()=>{return 0}}}",
        "enum E{Pair(i32,i32),None}fn result()->i32{var value=E.Pair(20,22);match value{E.Pair(ref mut a,ref mut b)=>{*a+=*b;return *a},E.None()=>{return 0}}}",
    ] { review329_native(&[("main",text)],42); }
}

fn review329_project(files: &[(&str, &str)]) -> zil_driver::ProjectSources {
    zil_driver::ProjectSources {
        root: ModuleId(0),
        modules: files
            .iter()
            .enumerate()
            .map(|(index, (name, text))| {
                SourceUnit::new(
                    FileId(index as u32),
                    ModuleId(index as u32),
                    format!("{name}.zil"),
                    *text,
                )
            })
            .collect(),
        paths: files
            .iter()
            .enumerate()
            .skip(1)
            .map(|(index, (name, _))| (vec![(*name).to_owned()], ModuleId(index as u32)))
            .collect(),
    }
}

fn review329_native(files: &[(&str, &str)], expected: i64) {
    use zil_driver::{NativeBackend, Optimization};
    let mut backends = vec![NativeBackend::Clang];
    if cfg!(feature = "llvm-api") {
        backends.push(NativeBackend::LlvmApi);
    }
    for backend in backends {
        for optimization in [Optimization::O0, Optimization::O2] {
            let directory = Temp::new();
            let report = zil_driver::compile_project(
                &review329_project(files),
                &CompileOptions {
                    backend,
                    optimization,
                    stop_after: Stage::Build,
                    entry: Some("result".into()),
                    output: Some(directory.0.join("program")),
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
                assert!(!codegen.object_reports.is_empty());
                assert!(
                    codegen
                        .object_reports
                        .iter()
                        .all(|object| object.contains("\"input_kind\":\"builder\""))
                );
            }
            let observed = Command::new(&report.artifacts["executable"])
                .output()
                .unwrap();
            assert!(
                observed.status.success(),
                "{backend:?}/{optimization:?}: {observed:?}"
            );
            assert_eq!(observed.stdout, format!("{expected}\n").as_bytes());
            assert!(observed.stderr.is_empty());
        }
    }
}

#[test]
fn review329_field_evaluation_binding_labels_and_i64_context_are_native() {
    for (text, expected) in [
        (
            "enum Pair{Item{a:i32,b:i32}}fn mark(trace:&mut i32,n:i32)->i32{*trace=*trace*10+n;return n}fn result()->i32{var trace=0;let p=Pair.Item{b:mark(&mut trace,2),a:mark(&mut trace,1)};match p{Pair.Item{b:right,a:left}=>{return trace*100+left*10+right}}}",
            2112,
        ),
        (
            "enum Event<T:Copy>{Data{code:T,tail:i32}}fn result()->i64{let e:Event<i64>=Event.Data{tail:2,code:1};match e{Event.Data{tail:y,code:x}=>{return x+i64(y)}}}",
            3,
        ),
        (
            "enum Color{N{value:i32}}enum Packet{P{left:Color,right:i32}}fn result()->i32{let value=100;let p=Packet.P{right:2,left:Color.N{value:7}};match p{Packet.P{right:left,left:Color.N{value:right}}=>{return right*10+left+value}}}",
            172,
        ),
        (
            "enum E{V{x:i32}}fn result()->i32{let x=7;let v=E.V{x};match v{E.V{x:n}=>{return n}}}",
            7,
        ),
        (
            "enum E{V{},U}fn result()->i32{let v=E.V{};match v{E.V{}=>{return 7},E.U()=>{return 0}}}",
            7,
        ),
    ] {
        review329_native(&[("main", text)], expected);
    }
}

#[test]
fn review329_field_sets_forms_and_uninstantiated_copy_aliases_reject() {
    for text in [
        "enum E{V{x:i32,y:i32}}fn result()->i32{let e=E.V{x:1};return 0}",
        "enum E{V{x:i32,y:i32}}fn result()->i32{let e=E.V{x:1,y:2,z:3};return 0}",
        "enum E{V{x:i32,y:i32}}fn result()->i32{let e=E.V{x:1,x:2};return 0}",
        "enum E{V{x:i32,y:i32}}fn result()->i32{let e=E.V(1,2);return 0}",
        "enum E{V(i32,i32)}fn result()->i32{let e=E.V{x:1,y:2};return 0}",
        "enum E{V{x:i32,y:i32}}fn result()->i32{let e=E.V{x:1,y:2};match e{E.V{x:n,y:n}=>{return n}}}",
        "enum E{V{x:i32,y:i32}}fn result()->i32{let e=E.V{x:1,y:2};match e{E.V{x:n}=>{return n}}}",
        "enum E{V{x:i32,y:i32}}fn result()->i32{let e=E.V{x:1,y:2};match e{E.V(x,y)=>{return x+y}}}",
        "enum E{V{},U}fn result()->i32{let e=E.V();return 0}",
        "enum E{V{},U}fn result()->i32{let e=E.U{};return 0}",
        "enum E{V{x:i32}}fn result()->i32{let e=E.V{x:1};match e{E.V{x}=>{return x}}}",
        "type Bad=Token<&i32>;enum Token<T:Copy>{Data{n:i32}}fn result()->i32{return 0}",
        "enum Token<T:Copy>{Data{n:i32}}type Bad=Token<&i32>;fn result()->i32{return 0}",
        "type Borrow=&i32;type Bad=Token<Borrow>;enum Token<T:Copy>{Data{n:i32}}fn result()->i32{return 0}",
        "enum E<T:Copy>{V{x:T}}fn bad<T:Copy>(x:T)->i64{let e:E<i64>=E.V{x:x};return 0}fn result()->i32{return 0}",
    ] {
        let report = compile_source(&source(text), &CompileOptions::default());
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{text}: {}",
            report.to_json()
        );
        assert!(!report.diagnostics.is_empty());
        assert!(!report.artifacts.contains_key("executable"));
    }
    let bad = "enum E<T:Copy>{V{x:T}}fn bad<T:Copy>(x:T)->E<T>{return E.V{x:7}}fn result()->i32{return 0}";
    let report = compile_source(&source(bad), &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::Unsupported,
        "{}",
        report.to_json()
    );
    assert_eq!(report.phase, Phase::Expand);
    assert!(report.diagnostics.iter().any(|d| d.code == "E-UNSUPPORTED"));
    let report = compile_source(
        &source(
            "enum E<T:Copy>{V{x:T}}fn good<T:Copy>(x:T)->E<T>{return E.V{x:x}}fn result()->i32{return 0}",
        ),
        &CompileOptions::default(),
    );
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1);
}

#[test]
fn review329_module_nominals_and_visibility_remain_distinct() {
    let files = [
        (
            "main",
            "import left;import right;fn result()->i32{return left.read(left.make())+right.read(right.make())}",
        ),
        (
            "left",
            "pub enum E{V{x:i32}}pub fn make()->E{return E.V{x:7}}pub fn read(e:E)->i32{match e{E.V{x:n}=>{return n}}}",
        ),
        (
            "right",
            "pub enum E{V{x:i32}}pub fn make()->E{return E.V{x:9}}pub fn read(e:E)->i32{match e{E.V{x:n}=>{return n}}}",
        ),
    ];
    review329_native(&files, 16);
    let mut confused = files;
    confused[0].1 = "import left;import right;fn result()->i32{return right.read(left.make())}";
    let report =
        zil_driver::compile_project(&review329_project(&confused), &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
    let report = zil_driver::compile_project(
        &review329_project(&[
            (
                "main",
                "import data;fn result()->i32{let x=data.Hidden.V{x:1};return 0}",
            ),
            ("data", "enum Hidden{V{x:i32}}"),
        ]),
        &CompileOptions::default(),
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
            .any(|d| d.code == "E-MODULE-PRIVATE")
    );
}

#[test]
fn review329_ct_quote_keeps_labels_separate_from_hygienic_bindings() {
    review329_native(
        &[(
            "main",
            "enum E{V{x:i32}}fn generated()->Syntax<StmtList>{return quote<StmtList>{let x=7;let e=E.V{x};match e{E.V{x:y}=>{return y}}}}fn result()->i32{let x=100;#emit generated()}",
        )],
        7,
    );
    review329_native(
        &[(
            "main",
            "enum E{V{Type:i32,left:i32}}fn generated()->Syntax<StmtList>{return quote<StmtList>{match (E.V{Type:1,left:2}){E.V{left:Type,Type:left}=>{return left*10+Type}}}}fn result()->i32{let Type=100;#emit generated()}",
        )],
        12,
    );
    review329_native(
        &[(
            "main",
            "enum E{V{left:i32,right:i32}}fn generated()->Syntax<StmtList>{return quote<StmtList>{match (E.V{left:1,right:2}){E.V{right:left,left:right}=>{return right*10+left}}}}fn result()->i32{let left=100;#emit generated()}",
        )],
        12,
    );
}

#[test]
fn review329_shared_record_pun_capture_neighbor() {
    review329_native(
        &[(
            "main",
            "struct P{x:i32}fn generated()->Syntax<StmtList>{return quote<StmtList>{let x=7;let p=P{x};return p.x}}fn result()->i32{let x=100;#emit generated()}",
        )],
        7,
    );
}

#[test]
fn review329_unused_variant_attributes_do_not_disappear() {
    let report = compile_source(
        &source("enum E{#[cfg(true)] V{x:i32}}fn result()->i32{return 0}"),
        &CompileOptions::default(),
    );
    assert_eq!(
        report.status,
        ResultCategory::Unsupported,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 0);
    assert!(report.diagnostics.iter().any(|d| d.code == "E-UNSUPPORTED"));
}

#[test]
fn review329_symbolic_primitive_spelling_patterns_do_not_gain_scalar_permissions() {
    for text in [
        "enum E<T:Copy>{V{x:T}}fn keep<i64:Copy>(e:E<i64>)->i32{match e{E.V{x:n}=>{return 0}}}fn result()->i32{return 0}",
        "fn scalar()->i64{return 1}fn keep<i64:Copy>(x:i64)->i32{let n=scalar();match n{1=>{return 1},_=>{return 0}}}fn result()->i32{return 0}",
    ] {
        let report = compile_source(&source(text), &CompileOptions::default());
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{text}: {}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1);
    }
    let mut accepted = Vec::new();
    for text in [
        "enum E<T:Copy>{V{x:T}}fn bad<i64:Copy>(e:E<i64>)->i32{match e{E.V{x:1}=>{return 1},E.V{x:_}=>{return 0}}}fn result()->i32{return 0}",
        "enum E<T:Copy>{V{x:T}}fn bad<bool:Copy>(e:E<bool>)->i32{match e{E.V{x:true}=>{return 1},E.V{x:_}=>{return 0}}}fn result()->i32{return 0}",
    ] {
        let report = compile_source(&source(text), &CompileOptions::default());
        eprintln!("review329 symbolic pattern {text}: {}", report.to_json());
        if report.status == ResultCategory::Success {
            accepted.push(text);
        }
    }
    assert!(
        accepted.is_empty(),
        "symbolic primitive-spelling patterns accepted: {accepted:?}"
    );
}

#[test]
fn record_enum329_quote_shorthand_preserves_value_hygiene() {
    // Reviewer 6c6481d08's legal-category source; author regression, not review.
    native(
        "enum E{V{x:i32}}fn generated()->Syntax<StmtList>{return quote<StmtList>{let x=7;let e=E.V{x};match e{E.V{x:y}=>{return y}}}}fn result()->i32{let x=100;#emit generated()}",
        Some(7),
    );
    native(
        "struct P{x:i32}fn generated()->Syntax<StmtList>{return quote<StmtList>{let x=7;let p=P{x};return p.x}}fn result()->i32{let x=100;#emit generated()}",
        Some(7),
    );
}

#[test]
fn record_enum329_unused_variant_attributes_remain_explicit() {
    let report = compile_source(
        &source("enum E<T>{#[cfg(false)] Row{value:T}}fn main(){}"),
        &CompileOptions::default(),
    );
    assert_eq!(
        report.status,
        ResultCategory::Unsupported,
        "{}",
        report.to_json()
    );
    assert_eq!(report.phase, Phase::Expand, "{}", report.to_json());
    assert_eq!(report.diagnostics[0].code, "E-UNSUPPORTED");
}

#[test]
fn record_enum329_named_source_order_and_exact_values() {
    native(
        r#"
enum Message{Data{left:i64,right:i64,flag:bool},Empty{}}
fn mark(log:&mut i64,digit:i64)->i64{*log=*log*10+digit;return digit}
fn result()->i64{
 var log:i64=0;
 let message=Message.Data{right:mark(&mut log,2),flag:true,left:mark(&mut log,1)};
 if log!=21{return -1}
 match message{
  Message.Data{flag:true,left:left,right:right}=>{if left!=1{return -2}if right!=2{return -3}return log*100+left*10+right},
  Message.Data{left:_,right:_,flag:false}=>{return -4},
  Message.Empty{}=>{return -5}
 }
}
"#,
        Some(2112),
    );
    native(
        r#"
enum E{Data{left:i64,right:i64},Empty{},Unit,Tuple(i64)}
fn read(e:E)->i64{match e{E.Data{right:r,left:l}=>{return l*100+r},E.Empty{}=>{return 1},E.Unit()=>{return 2},E.Tuple(n)=>{return n}}}
fn result()->i64{let left:i64=7;let right:i64=9;return read(E.Data{right,left})+read(E.Empty{})+read(E.Unit)+read(E.Tuple(3))}
"#,
        Some(715),
    );
}

#[test]
fn record_enum329_nested_generic_copy_and_phantom_identity() {
    native(
        r#"
struct P{n:i64}
enum E{Named{p:P},Old(P)}
fn result()->i64{let a=E.Named{p:{n:7}};let b=E.Old(P{n:9});match a{E.Named{p:p}=>{match b{E.Old(q)=>{return p.n*10+q.n},E.Named{p:_}=>{return -1}}},E.Old(_)=>{return -2}}}
"#,
        Some(79),
    );
    native(
        r#"
struct Box<T:Copy>{value:T}
enum Message<T:Copy,P:Copy>{Data{boxed:Box<T>,ok:bool}}
fn make<T:Copy,P:Copy>(value:T)->Message<T,P>{return Message::<T,P>.Data{ok:true,boxed:Box::<T>{value:value}}}
fn read<T:Copy,P:Copy>(message:Message<T,P>)->T{match message{Message.Data{ok:_,boxed:box}=>{return box.value}}}
fn result()->i64{let a=make::<i64,bool>(7);let b=make::<i64,i32>(9);return read::<i64,bool>(a)*10+read::<i64,i32>(b)}
"#,
        Some(79),
    );
    native(
        r#"
enum Inner{Only{x:i64,flag:bool}}
enum Outer{Wrap{inner:Inner,pair:(i64,bool)},Other}
fn result()->i64{let value=Outer.Wrap{pair:(7,true),inner:Inner.Only{flag:false,x:5}};match value{Outer.Wrap{pair:(a,true),inner:Inner.Only{x:b,flag:false}}=>{let read=()=>a*10+b;return read()},Outer.Wrap{inner:_,pair:_}=>{return -1},Outer.Other()=>{return -2}}}
"#,
        Some(75),
    );
}

#[test]
fn record_enum329_labels_shapes_and_unused_definitions_reject() {
    for text in [
        "enum E{V{x:i32,y:bool}}fn main(){let x=E.V{x:1};}",
        "enum E{V{x:i32,y:bool}}fn main(){let x=E.V{x:1,y:true,z:2};}",
        "enum E{V{x:i32,y:bool}}fn main(){let x=E.V{x:1,x:2,y:true};}",
        "enum E{V{x:i32,y:bool}}fn main(){let x=E.V{x:true,y:1};}",
        "enum E{V{x:i32,y:bool}}fn main(){let x=E.V(1,true);}",
        "enum E{V(i32,bool)}fn main(){let x=E.V{x:1,y:true};}",
        "enum E{V{x:i32,y:bool}}fn unused(e:E){match e{E.V{x:x}=>{}}}fn main(){}",
        "enum E{V{x:i32,y:bool}}fn unused(e:E){match e{E.V{x:x,y:y,z:_}=>{}}}fn main(){}",
        "enum E{V{x:i32,y:bool}}fn unused(e:E){match e{E.V{x:a,x:b,y:_}=>{}}}fn main(){}",
        "enum E{V{x:i32,y:bool}}fn unused(e:E){match e{E.V{x:n,y:n}=>{}}}fn main(){}",
        "enum E{V{x:i32,y:bool}}fn unused(e:E){match e{E.V(x,y)=>{}}}fn main(){}",
        "enum E{V(i32,bool)}fn unused(e:E){match e{E.V{x:x,y:y}=>{}}}fn main(){}",
        "enum A{V{x:i32}}enum B{V{x:i32}}fn unused(e:A){match e{B.V{x:_}=>{}}}fn main(){}",
        "enum E<T:Copy>{V{x:T}}fn unused<T>(x:T){let value=E.V{x:x};}fn main(){}",
        "enum E<T:Copy,P:Copy>{V{x:T}}fn unused<T:Copy,P>(e:E<T,P>){}fn main(){}",
        "enum E<T:Copy,P:Copy>{V{x:T}}fn only(e:E<i32,bool>){}fn main(){let e:E<i32,i64>=E.V{x:7};only(e)}",
        "enum E<T:Copy,P:Copy>{V{x:T}}type Bad=E<i32,ByteBuffer>;fn main(){}",
        "enum E<T:Copy>{V{x:T}}type A<T>=E<T>;fn unused<T>(x:T)->A<T>{return A::<T>.V{x:x}}fn main(){}",
        "struct Box<T:Copy>{value:T}enum E<T>{V{value:Box<T>}}fn main(){}",
        "enum E{V{x:i32,x:i32}}fn main(){}",
        "enum E{V{x:&i32}}fn main(){}",
        "enum E{V{x:ByteBuffer}}fn main(){}",
        "enum E{V{x:E}}fn main(){}",
        "enum E<T>{V{x:T}}fn unused<T>(x:T){let value=E.V{x:x};let again=x;}fn main(){}",
    ] {
        let report = compile_source(&source(text), &CompileOptions::default());
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{text}: {}",
            report.to_json()
        );
        assert!(!report.diagnostics.is_empty(), "{text}");
        assert!(!report.artifacts.contains_key("executable"));
    }
    for text in [
        "enum E<T>{R{r:T}}fn bad<T:Copy>()->E<T>{return E::<T>.R{r:7}}fn main(){}",
        "enum E{V{x:i32=7}}fn main(){}",
        "enum E{V{pub x:i32}}fn main(){}",
        "resource R{x:i32}enum E{V{manages owner:R}}fn main(){}",
        "resource R{x:i32}enum E{V{x:i32},Other(manages R)}fn main(){}",
        "enum E{V{x:i32}}fn unused(e:E){if let E.V{x:x}=e{}}fn main(){}",
    ] {
        let report = compile_source(&source(text), &CompileOptions::default());
        assert_eq!(
            report.status,
            ResultCategory::Unsupported,
            "{text}: {}",
            report.to_json()
        );
        assert!(!report.diagnostics.is_empty());
    }
    for text in [
        "enum E<T:Copy>{V{x:T}}fn unused<T:Copy>(x:T){let value=E.V{x:x};}fn main(){}",
        "enum E<T>{R{r:T}}fn good<T:Copy>(r:T)->E<T>{return E::<T>.R{r:r}}fn main(){}",
        "enum E<T:Copy>{V{x:T}}type A<T>=E<T>;fn unused<T:Copy>(x:T)->A<T>{return A::<T>.V{x:x}}fn main(){}",
        "enum E<T:Copy,P:Copy>{V{x:T}}fn unused<T:Copy,P:Copy>(e:E<T,P>){}fn main(){}",
        "struct Box<T:Copy>{value:T}enum E<T:Copy>{V{value:Box<T>}}fn main(){}",
    ] {
        let report = compile_source(&source(text), &CompileOptions::default());
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{text}: {}",
            report.to_json()
        );
    }
}

#[test]
fn record_enum329_public_module_alias_and_field_label_dependency() {
    use zil_driver::{CompilationSession, ProjectSources};
    let main = "import model;type Alias=model.Message<i64>;fn main()->i32{let e:Alias=model.Message.Data{value:7,flag:true};match e{Alias.Data{flag:true,value:n}=>{if n!=7{return 1}return 0},Alias.Data{flag:false,value:_}=>{return 2}}}";
    let model = "pub enum Message<T:Copy>{Data{flag:bool,value:T}}";
    let project = |provider: &str| ProjectSources {
        root: ModuleId(0),
        modules: vec![
            source(main),
            SourceUnit::new(FileId(1), ModuleId(1), "model.zil", provider),
        ],
        paths: std::collections::BTreeMap::from([(vec!["model".into()], ModuleId(1))]),
    };
    for optimization in [zil_driver::Optimization::O0, zil_driver::Optimization::O2] {
        let out = Temp::new();
        let report = zil_driver::compile_project(
            &project(model),
            &CompileOptions {
                stop_after: Stage::Build,
                output: Some(out.0.join("program")),
                optimization,
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
        let result = Command::new(&report.artifacts["executable"])
            .output()
            .unwrap();
        assert_eq!(result.status.code(), Some(0), "{result:?}");
        assert!(result.stdout.is_empty() && result.stderr.is_empty());
    }
    let mut session = CompilationSession::default();
    let options = CompileOptions::default();
    assert_eq!(
        session.compile_project(&project(model), &options).status,
        ResultCategory::Success
    );
    // Same payload types/layout, different source member identity. Neither a
    // warm source cache nor positional Enum IR may retain the removed label.
    let renamed = model.replace("value:T", "renamed:T");
    let fresh = CompilationSession::default().compile_project(&project(&renamed), &options);
    assert_eq!(
        fresh.status,
        ResultCategory::SafeReject,
        "{}",
        fresh.to_json()
    );
    let warm = session.compile_project(&project(&renamed), &options);
    assert_eq!(
        warm.status,
        ResultCategory::SafeReject,
        "{}",
        warm.to_json()
    );
    assert_eq!(session.active_query_count(), 0);
    assert_eq!(
        session.compile_project(&project(model), &options).status,
        ResultCategory::Success
    );
}

#[test]
fn record_enum329_request_budgets_are_exact_cold_warm_and_fresh() {
    use zil_driver::CompilationSession;
    let input = source(
        "enum E<T:Copy,P:Copy>{V{value:T,flag:bool}}fn make<T:Copy,P:Copy>(x:T)->E<T,P>{return E::<T,P>.V{flag:true,value:x}}fn read<T:Copy,P:Copy>(e:E<T,P>)->T{match e{E.V{flag:_,value:n}=>{return n}}}fn main()->i32{return read::<i32,bool>(make::<i32,bool>(7))-7}",
    );
    for ledger in ["work", "nodes", "instances"] {
        let options = |n| {
            let mut options = CompileOptions::default();
            match ledger {
                "work" => options.limits.max_analysis_steps = n,
                "nodes" => options.limits.max_nodes = n,
                _ => options.limits.max_functions = n,
            }
            options
        };
        let mut low = 1;
        let mut high = match ledger {
            "work" => Limits::default().max_analysis_steps,
            "nodes" => Limits::default().max_nodes,
            _ => Limits::default().max_functions,
        };
        while low < high {
            let n = low + (high - low) / 2;
            let report = compile_source(&input, &options(n));
            match report.status {
                ResultCategory::Success => high = n,
                ResultCategory::ResourceLimit => low = n + 1,
                _ => panic!("{}", report.to_json()),
            }
        }
        let mut warm = CompilationSession::default();
        for (n, expected) in [
            (low, ResultCategory::Success),
            (low - 1, ResultCategory::ResourceLimit),
            (low, ResultCategory::Success),
        ] {
            assert_eq!(warm.compile_source(&input, &options(n)).status, expected);
            assert_eq!(
                CompilationSession::default()
                    .compile_source(&input, &options(n))
                    .status,
                expected
            );
            assert_eq!(warm.active_query_count(), 0);
        }
        println!("329 full request {ledger} N={low}; N-1 ResourceLimit, cold/warm/fresh agree");
    }
}

#[test]
fn primitive_binder329_request_budget_is_shared_cold_warm_and_fresh() {
    use zil_driver::CompilationSession;
    let input = source(
        "enum E<T:Copy>{V{x:T}}fn identity<i64:Copy>(x:i64)->i64{return x}fn read<i64:Copy>(e:E<i64>)->i64{match e{E.V{x:n}=>{return n}}}fn main()->i32{return read::<i32>(E::<i32>.V{x:identity::<i32>(7)})-7}",
    );
    for ledger in ["work", "nodes"] {
        let options = |n| {
            let mut options = CompileOptions::default();
            if ledger == "work" {
                options.limits.max_analysis_steps = n;
            } else {
                options.limits.max_nodes = n;
            }
            options
        };
        let mut low = 1;
        let mut high = if ledger == "work" {
            Limits::default().max_analysis_steps
        } else {
            Limits::default().max_nodes
        };
        while low < high {
            let n = low + (high - low) / 2;
            let report = compile_source(&input, &options(n));
            match report.status {
                ResultCategory::Success => high = n,
                ResultCategory::ResourceLimit => low = n + 1,
                _ => panic!("{}", report.to_json()),
            }
        }
        assert!(low > 1);
        assert!(
            low < if ledger == "work" {
                Limits::default().max_analysis_steps
            } else {
                Limits::default().max_nodes
            }
        );
        let mut warm = CompilationSession::default();
        for (n, expected) in [
            (low, ResultCategory::Success),
            (low - 1, ResultCategory::ResourceLimit),
            (low, ResultCategory::Success),
        ] {
            let report = warm.compile_source(&input, &options(n));
            assert_eq!(report.status, expected, "{}", report.to_json());
            if expected == ResultCategory::ResourceLimit {
                assert!(!report.diagnostics.is_empty());
            }
            let fresh = CompilationSession::default().compile_source(&input, &options(n));
            assert_eq!(fresh.status, expected, "{}", fresh.to_json());
            assert_eq!(warm.active_query_count(), 0);
        }
        println!(
            "329 primitive binder full request {ledger} N={low}; N-1 ResourceLimit, cold/warm/fresh agree"
        );
    }
}

#[test]
fn record_enum329_raw_and_wire_preserve_typed_enum_integrity() {
    let input = source(
        "enum E{V{number:i64,flag:bool},U}fn result()->i64{let e=E.V{flag:true,number:7};match e{E.V{flag:_,number:n}=>{return n},E.U()=>{return 0}}}",
    );
    let syntax = zil_syntax::parse(&input).into_result().unwrap();
    let core = zil_expand::expand(&input, &syntax).unwrap();
    let target = TargetProfile::default();
    let limits = Limits::default();
    let typed = zil_types::check(&core, &target, &limits).unwrap();
    let plan = zil_analyze::analyze(&typed, &limits).unwrap();
    let wire = zil_ir::wire::encode_module_candidate(&typed, &plan, &limits).unwrap();
    assert_eq!(
        zil_verify::verify_module_bytes(&wire, &target, &limits)
            .unwrap()
            .module(),
        &typed
    );
    for attack in 0..3 {
        let mut damaged = typed.clone();
        let function = &mut damaged.functions[0];
        if attack < 2 {
            let op = function
                .blocks
                .iter_mut()
                .flat_map(|b| &mut b.instructions)
                .find(|i| matches!(i.kind, InstructionKind::MakeVariant { .. }))
                .unwrap();
            let InstructionKind::MakeVariant { variant, fields } = &mut op.kind else {
                unreachable!()
            };
            if attack == 0 {
                fields.swap(0, 1)
            } else {
                *variant = 99
            }
        } else {
            let op = function
                .blocks
                .iter_mut()
                .flat_map(|b| &mut b.instructions)
                .find(|i| matches!(i.kind, InstructionKind::ReadVariant { .. }))
                .unwrap();
            let InstructionKind::ReadVariant { index, .. } = &mut op.kind else {
                unreachable!()
            };
            *index = 99;
        }
        let mut claims = plan.clone();
        claims.module_fingerprint = damaged.fingerprint();
        let raw = zil_verify::verify_module(damaged.clone(), claims.clone(), &target, &limits)
            .unwrap_err();
        let bytes = zil_ir::wire::encode_module_candidate(&damaged, &claims, &limits).unwrap();
        let transported = zil_verify::verify_module_bytes(&bytes, &target, &limits).unwrap_err();
        for errors in [raw, transported] {
            assert!(!errors.is_empty());
            assert_eq!(errors[0].phase, Phase::Verify);
            assert!(errors[0].code.starts_with("E-VERIFY-"));
        }
    }
}

struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "zil-aggregate-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn source(text: &str) -> SourceUnit {
    SourceUnit::new(FileId(0), ModuleId(0), "aggregate.zil", text)
}
fn native(text: &str, expected: Option<i64>) -> String {
    native_entry(text, "result", expected)
}
fn native_entry(text: &str, entry: &str, expected: Option<i64>) -> String {
    let out = Temp::new();
    let report = compile_source(
        &source(text),
        &CompileOptions {
            stop_after: Stage::Build,
            entry: Some(entry.into()),
            output: Some(out.0.join("plain")),
            ..CompileOptions::default()
        },
    );
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    let llvm = fs::read_to_string(&report.artifacts["llvm"]).unwrap();
    for optimized in [false, true] {
        let executable = if optimized {
            let p = out.0.join("optimized");
            let built = Command::new("clang")
                .args(["-x", "ir", "-O2"])
                .arg(&report.artifacts["llvm"])
                .arg("-o")
                .arg(&p)
                .output()
                .unwrap();
            assert!(
                built.status.success(),
                "{}",
                String::from_utf8_lossy(&built.stderr)
            );
            p
        } else {
            report.artifacts["executable"].clone()
        };
        let observed = Command::new(executable).output().unwrap();
        match expected {
            Some(value) => {
                assert!(observed.status.success(), "{observed:?}");
                assert_eq!(observed.stdout, format!("{value}\n").as_bytes());
                assert!(observed.stderr.is_empty());
            }
            None => assert!(!observed.status.success(), "out-of-bounds access must trap"),
        }
    }
    llvm
}
fn reject(text: &str, code: &str) {
    let r = compile_source(&source(text), &CompileOptions::default());
    assert_eq!(r.status, ResultCategory::SafeReject, "{}", r.to_json());
    assert_eq!(r.diagnostics[0].code, code, "{}", r.to_json());
    assert!(!r.artifacts.contains_key("executable"));
}

fn large_copy_program(body: &str) -> String {
    let mut ty = "[u8;1]".to_owned();
    let mut initializers = "let a0=[3u8];".to_owned();
    for level in 1..=12 {
        ty = format!("[{ty};2]");
        initializers.push_str(&format!("let a{level}=[a{},a{}];", level - 1, level - 1));
    }
    let first = "[0]".repeat(13);
    format!(
        "type Big={ty}; fn touch(value:&mut Big)->i32{{(*value){first}+=1u8;return 1}} fn inspect(value:Big,marker:i32)->i32{{return i32(value{first})+marker}} fn result()->i32{{{initializers}{body}}}"
    )
}

#[test]
fn large_copy_snapshot_order_nested_index_and_loop_join() {
    let first = "[0]".repeat(13);
    let rest = "[0]".repeat(12);
    let text = large_copy_program(&format!(
        "var original=a12;let saved=original;var pair=(original,touch(&mut original));var index=1;pair.0[index]{rest}=4u8;let before=pair.0;pair.0[0]{rest}=8u8;let observed=inspect(original,touch(&mut original));var branch=original;var i=0;while i<2{{if i==0{{branch=saved}}else{{branch=before}}i+=1}}return i32(saved{first})*10000+i32(before{first})*1000+i32(pair.0{first})*100+i32(branch[index]{rest})*10+observed+i32(original{first});"
    ));
    let llvm = native(&text, Some(33850));
    assert!(llvm.contains("copy.value."));
    assert!(llvm.contains("llvm.memcpy.p0.p0.i64"));
    assert!(!llvm.contains("@malloc"));
}

#[test]
fn large_copy_read_address_snapshot_and_bounds_trap() {
    let first = "[0]".repeat(13);
    let text = large_copy_program(&format!(
        "var original=a12;let p=&mut original;let before=*p;(*p){first}=7u8;let after=*p;return i32(before{first})*10+i32(after{first});"
    ));
    native(&text, Some(37));
    let text = large_copy_program(&format!(
        "var index=2;return i32(a12[index]{});",
        "[0]".repeat(12)
    ));
    native(&text, None);
}

#[test]
fn large_copy_record_unit_padding_and_tail_snapshot() {
    let first = "[0]".repeat(13);
    let text = large_copy_program(&format!(
        "var record=Envelope{{unit:(),flag:true,body:a12,tail:9}};let before=record;record.body{first}=7u8;record.tail=11;record.flag=false;if before.flag{{return i32(before.body{first})*1000+i32(record.body{first})*100+i32(before.tail)*10+i32(record.tail)}}return 0;"
    ));
    let text = format!("struct Envelope{{unit:();flag:bool;body:Big;tail:i64}}{text}");
    native(&text, Some(3801));
}

#[test]
fn large_copy_current_codegen_budget_is_exact_and_fresh() {
    let src = source(include_str!(
        "../../../tests/language-readiness/codegen-pending/large-array-a18.zil"
    ));
    let ast = zil_syntax::parse(&src).into_result().unwrap();
    let core = zil_expand::expand(&src, &ast).unwrap();
    let target = TargetProfile::default();
    let defaults = Limits::default();
    let typed = zil_types::check(&core, &target, &defaults).unwrap();
    let candidate = zil_analyze::analyze(&typed, &defaults).unwrap();
    let emit = |steps| {
        let limits = Limits {
            max_analysis_steps: steps,
            ..defaults.clone()
        };
        let verified =
            zil_verify::verify_module(typed.clone(), candidate.clone(), &target, &limits)?;
        let lir = zil_lower::lower(&verified)?;
        zil_backend_llvm::emit(&verified, &lir)
    };
    let (mut low, mut high) = (1, defaults.max_analysis_steps);
    assert!(emit(high).is_ok());
    while low < high {
        let middle = low + (high - low) / 2;
        if emit(middle).is_ok() {
            high = middle;
        } else {
            low = middle + 1;
        }
    }
    let failed = emit(low - 1).unwrap_err();
    assert_eq!(failed[0].category, ResultCategory::ResourceLimit);
    assert_eq!(failed[0].phase, Phase::Codegen);
    assert!(emit(low).is_ok());
    assert!(emit(low).is_ok());
    eprintln!("324 original a18 Codegen exact work limit: {low}");
}

#[test]
fn large_copy_a18_native_all_backends() {
    // Isolate LLVM failures and cap each compiler process. This remains one
    // existing Cargo integration target and preserves the original input bytes.
    if let Ok(profile) = std::env::var("ZIL_324_LARGE_COPY_CHILD") {
        use zil_driver::{NativeBackend, Optimization};
        let (backend, optimization) = match profile.as_str() {
            "clang-O0" => (NativeBackend::Clang, Optimization::O0),
            "clang-O2" => (NativeBackend::Clang, Optimization::O2),
            "llvm-api-O0" => (NativeBackend::LlvmApi, Optimization::O0),
            "llvm-api-O2" => (NativeBackend::LlvmApi, Optimization::O2),
            _ => panic!("unknown child profile"),
        };
        let out = Temp::new();
        let report = compile_source(
            &source(include_str!(
                "../../../tests/language-readiness/codegen-pending/large-array-a18.zil"
            )),
            &CompileOptions {
                backend,
                optimization,
                stop_after: Stage::Build,
                cancellation: zil_driver::CancellationToken::with_timeout(
                    std::time::Duration::from_secs(45),
                ),
                output: Some(out.0.join("program")),
                ..CompileOptions::default()
            },
        );
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        let codegen = report.native_codegen.as_ref().unwrap();
        assert_eq!(codegen.backend, backend);
        assert!(codegen.fallback.is_none());
        if backend == NativeBackend::LlvmApi {
            let pipeline = format!("\"pipeline\":\"default<{optimization:?}>\"");
            assert!(!codegen.object_reports.is_empty());
            assert!(codegen.object_reports.iter().all(|r| r.contains(&pipeline)));
        } else {
            let flag = if optimization == Optimization::O0 {
                "-O0"
            } else {
                "-O2"
            };
            assert!(
                report
                    .commands
                    .iter()
                    .any(|command| command.iter().any(|arg| arg == flag))
            );
        }
        let actual = Command::new("timeout")
            .args(["-s", "KILL", "5s"])
            .arg(&report.artifacts["executable"])
            .output()
            .unwrap();
        assert_eq!(actual.status.code(), Some(0), "{actual:?}");
        assert!(actual.stdout.is_empty());
        assert!(actual.stderr.is_empty());
        return;
    }
    for backend in ["clang", "llvm-api"] {
        if backend == "llvm-api" && !cfg!(feature = "llvm-api") {
            continue;
        }
        for optimization in ["O0", "O2"] {
            let actual = Command::new("timeout")
                .args([
                    "-s",
                    "KILL",
                    "55s",
                    "bash",
                    "-c",
                    "ulimit -c 0; ulimit -t 45; ulimit -v 2097152; exec \"$@\"",
                    "--",
                ])
                .arg(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "large_copy_a18_native_all_backends",
                    "--nocapture",
                    "--test-threads=1",
                ])
                .env(
                    "ZIL_324_LARGE_COPY_CHILD",
                    format!("{backend}-{optimization}"),
                )
                .output()
                .unwrap();
            assert!(
                actual.status.success(),
                "{backend}-{optimization}: {} {}",
                String::from_utf8_lossy(&actual.stdout),
                String::from_utf8_lossy(&actual.stderr)
            );
            eprintln!(
                "324 a18 {backend}-{optimization}: backend matched, requested pipeline, fallback=null, native_exit=0"
            );
        }
    }
}
#[test]
fn large_copy_a18_by_value_abi_native_all_backends() {
    // Isolate LLVM failures and cap each compiler process. This remains one
    // existing Cargo integration target and preserves the original input bytes.
    if let Ok(profile) = std::env::var("ZIL_333_LARGE_COPY_CHILD") {
        use zil_driver::{NativeBackend, Optimization};
        let (backend, optimization) = match profile.as_str() {
            "clang-O0" => (NativeBackend::Clang, Optimization::O0),
            "clang-O2" => (NativeBackend::Clang, Optimization::O2),
            "llvm-api-O0" => (NativeBackend::LlvmApi, Optimization::O0),
            "llvm-api-O2" => (NativeBackend::LlvmApi, Optimization::O2),
            _ => panic!("unknown child profile"),
        };
        let out = Temp::new();
        let case = std::env::var("ZIL_333_CASE").unwrap();
        let input = match case.as_str() {
            "argument" => include_str!(
                "../../../tests/language-readiness/codegen-pending/large-array-a18-by-value.zil"
            ),
            "return" => include_str!(
                "../../../tests/language-readiness/codegen-pending/large-array-a18-return.zil"
            ),
            "pointer" => include_str!(
                "../../../tests/language-readiness/codegen-pending/large-array-a18-function-pointer.zil"
            ),
            _ => panic!("unknown ABI case"),
        };
        let report = compile_source(
            &source(input),
            &CompileOptions {
                backend,
                optimization,
                stop_after: Stage::Build,
                cancellation: zil_driver::CancellationToken::with_timeout(
                    std::time::Duration::from_secs(45),
                ),
                output: Some(out.0.join("program")),
                ..CompileOptions::default()
            },
        );
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        let llvm = fs::read_to_string(&report.artifacts["llvm"]).unwrap();
        assert!(llvm.contains(".lc1("), "versioned definition/call absent");
        assert!(llvm.contains("@zil.copy.bytes"));
        let codegen = report.native_codegen.as_ref().unwrap();
        assert_eq!(codegen.backend, backend);
        assert!(codegen.fallback.is_none());
        if backend == NativeBackend::LlvmApi {
            let pipeline = format!("\"pipeline\":\"default<{optimization:?}>\"");
            assert!(!codegen.object_reports.is_empty());
            assert!(codegen.object_reports.iter().all(|r| r.contains(&pipeline)));
        } else {
            let flag = if optimization == Optimization::O0 {
                "-O0"
            } else {
                "-O2"
            };
            assert!(
                report
                    .commands
                    .iter()
                    .any(|command| command.iter().any(|arg| arg == flag))
            );
        }
        let actual = Command::new("timeout")
            .args(["-s", "KILL", "5s"])
            .arg(&report.artifacts["executable"])
            .output()
            .unwrap();
        assert_eq!(actual.status.code(), Some(0), "{actual:?}");
        assert!(actual.stdout.is_empty());
        assert!(actual.stderr.is_empty());
        return;
    }
    for backend in ["clang", "llvm-api"] {
        if backend == "llvm-api" && !cfg!(feature = "llvm-api") {
            continue;
        }
        for optimization in ["O0", "O2"] {
            for case in ["argument", "return", "pointer"] {
                let actual = Command::new("timeout")
                    .args([
                        "-s",
                        "KILL",
                        "55s",
                        "bash",
                        "-c",
                        "ulimit -c 0; ulimit -t 45; ulimit -v 2097152; exec \"$@\"",
                        "--",
                    ])
                    .arg(std::env::current_exe().unwrap())
                    .args([
                        "--exact",
                        "large_copy_a18_by_value_abi_native_all_backends",
                        "--nocapture",
                        "--test-threads=1",
                    ])
                    .env("ZIL_333_CASE", case)
                    .env(
                        "ZIL_333_LARGE_COPY_CHILD",
                        format!("{backend}-{optimization}"),
                    )
                    .output()
                    .unwrap();
                assert!(
                    actual.status.success(),
                    "{backend}-{optimization}: {} {}",
                    String::from_utf8_lossy(&actual.stdout),
                    String::from_utf8_lossy(&actual.stderr)
                );
                eprintln!(
                    "333 a18 ABI {case} {backend}-{optimization}: backend matched, requested pipeline, fallback=null, native_exit=0"
                );
            }
        }
    }
}

#[test]
fn records_nested_update_parameter_and_return() {
    let llvm = native(
        include_str!("../../../examples/aggregates/main.zil"),
        Some(6483),
    );
    assert!(llvm.contains("insertvalue"));
    assert!(llvm.contains("extractvalue"));
    assert!(llvm.contains("{ { i64, i64 }, { i64, i64 }, i64 }"));
}
#[test]
fn arrays_nested_fields_loop_and_copy_return() {
    let llvm = native(
        include_str!("../../../examples/aggregates/arrays.zil"),
        Some(7498),
    );
    assert!(llvm.contains("[3 x { i64, i64 }]"));
    assert!(llvm.contains("icmp ult i64"));
    assert!(llvm.contains("label %zil.trap"));
    let entry = llvm.split("entry:").nth(1).unwrap();
    assert!(
        entry
            .split("bb.0:")
            .next()
            .unwrap()
            .contains("%index.slot.")
    );
}
#[test]
fn empty_unit_bool_tuples_and_nested_arrays() {
    native(
        "struct Empty {} struct Bits { unit: (); flag: bool; } fn id(x: Empty) -> Empty { return x; } fn result() -> i32 { let e = id(Empty {}); let z: [i64; 0] = []; var t = (Bits { unit: (), flag: true }, [3, 7]); t.1[0] = 5; t.0.unit = (); if t.0.flag { return t.1[0] * 10 + t.1[1] }; return 0; }",
        Some(57),
    );
}
#[test]
fn tuple_binding_original_readiness_source() {
    native_entry(
        include_str!("../../../tests/language-readiness/pending/runtime-tuple-binding.zil"),
        "main",
        Some(0),
    );
}

#[test]
fn common313_discard_still_checks_copy_and_evaluates_record_rhs_once() {
    review329_native(
        &[(
            "main",
            "enum Envelope{Value{left:i32,right:i32}}fn mark(trace:&mut i32,n:i32)->i32{*trace=*trace*10+n;return n}fn result()->i32{var trace=0;let(_,_)=(Envelope.Value{right:mark(&mut trace,2),left:mark(&mut trace,1)},());return trace}",
        )],
        21,
    );
    for text in [
        "fn result()->i32{let value=7;let(_,_)=(&value,0);return value}",
        "resource Token{value:i32}impl Drop for Token{fn drop(self:Self){}}fn result()->i32{let(_,_)=(Token{value:7},0);return 0}",
    ] {
        let report = compile_source(&source(text), &CompileOptions::default());
        assert_eq!(
            report.status,
            ResultCategory::Unsupported,
            "{}",
            report.to_json()
        );
        assert_eq!(report.phase, Phase::Types, "{}", report.to_json());
        assert_eq!(report.diagnostics[0].code, "E-UNSUPPORTED");
        assert_eq!(report.verification_runs, 0);
        assert!(!report.artifacts.contains_key("executable"));
    }
}
#[test]
fn review318_tuple_rhs_order_and_ct_traversal() {
    native(
        "fn step(c:&mut i32)->i32{*c+=1;return *c}fn result()->i32{var count=0;let(a,(b,z))=(step(&mut count),(step(&mut count),#(2+3)));return count*1000+a*100+b*10+z}",
        Some(2125),
    );
}
#[test]
fn review318_tuple_defer_capture_and_scoped_initialization() {
    native(
        "fn result()->i32{var(a,b)=(1,2);if true{defer{let(x,y)=(b,1);a+=x+y}}return a*10+b}",
        Some(42),
    );
    native(
        "fn result()->i32{var outer=5;var f=()=>{let(a,b)=(outer,1);outer=a+b;return outer};let first=f();return first*10+f()}",
        Some(67),
    );
    for (text, code) in [
        (
            "fn result()->i32{if true{let(a,b)=(1,2)}return a}",
            "E-UNKNOWN-NAME",
        ),
        (
            "fn result()->i32{let f=()=>{let(a,b)=(missing,1);return a+b};return f()}",
            "E-UNKNOWN-NAME",
        ),
        (
            "fn result()->i32{var(a,b)=(1,2);defer{let(x,y)=(a,b);return x+y}return 0}",
            "E-DEFER-CONTROL",
        ),
    ] {
        let report = compile_source(&source(text), &CompileOptions::default());
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{}",
            report.to_json()
        );
        assert!(
            report.diagnostics.iter().any(|d| d.code == code),
            "{}",
            report.to_json()
        );
        assert!(!report.artifacts.contains_key("executable"));
        eprintln!("review318/walker rejection: {}", report.to_json());
    }
}
#[test]
fn review318_original_generic_gap_and_mixed_copy_record_obligations() {
    native(
        include_str!("../../../tests/language-readiness/recovery/review318-generic-tuple-gap.zil"),
        Some(42),
    );
    let valid = "struct Pair<T:Copy,U>{first:T;second:U}fn hidden<T:Copy,U:Copy>(p:Pair<T,U>)->Pair<T,U>{let(x,_)=(p,());return x}fn result()->i32{return 0}";
    let report = compile_source(&source(valid), &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1);
    for text in [
        "struct T{n:i32}fn hidden<T>(value:T){let(_,_)=(value,());}fn result()->i32{return 0}",
        "struct Pair<T:Copy,U>{first:T;second:U}fn hidden<T:Copy,U>(p:Pair<T,U>){let(_,_)=(p,());}fn result()->i32{return 0}",
    ] {
        let report = compile_source(&source(text), &CompileOptions::default());
        eprintln!("review318/symbolic obligation: {}", report.to_json());
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
                .any(|d| d.code == "E-GENERIC-ARGUMENT")
        );
        assert!(!report.artifacts.contains_key("executable"));
    }
}
#[test]
fn tuple_binding_nested_mutable_typed_discard_and_single_evaluation() {
    let llvm = native(
        "fn pair(count: &mut i32) -> (i32,(i32,i32)) { *count += 1; return (*count,(*count+1,*count+2)); } fn result()->i32 { var count=0; let (a,(b,_)):(i32,(i32,i32))=pair(&mut count); var (c,(d,e))=(a,(b,7)); c+=10; d+=20; e+=30; let ()=(); let (_,)= (9,); return count*1000+a*100+b*10+c+d+e; }",
        Some(1190),
    );
    assert!(llvm.contains("extractvalue"));
    assert!(!llvm.contains("malloc"));
}
#[test]
fn tuple_binding_shadow_closure_and_copy_fields() {
    native(
        "struct P { x:i32; } fn result()->i32 { let x=5; if true { let (x,y)=(x+1,2); let f=()=>{ let (a,b)=(x,y); return a+b; }; var (p,a,flag,u)=(P{x:f()},[3,4],true,()); p.x+=1; a[1]=7; if flag { return p.x*100+a[0]*10+a[1]; } }; return 0; }",
        Some(937),
    );
    native(
        "fn result()->i32 { var (a,b)=(1,2); var f=()=>{ a+=b; return a; }; let first=f(); return first*10+f(); }",
        Some(35),
    );
    native(
        "fn result()->i64 { let (a,b):(i64,i64)=(3000000000,2); return a+b; }",
        Some(3000000002),
    );
}
#[test]
fn tuple_binding_shape_and_permission_neighbors() {
    for (text, code) in [
        (
            "fn result()->i32 { let (a,)=(1,2); return a; }",
            "E-MATCH-ARITY",
        ),
        (
            "fn result()->i32 { let (a,b,c)=(1,2); return a; }",
            "E-MATCH-ARITY",
        ),
        (
            "fn result()->i32 { let (a,(b,))=(1,(2,3)); return a; }",
            "E-MATCH-ARITY",
        ),
        (
            "fn result()->i32 { let (a,b)=1; return a; }",
            "E-MATCH-TYPE",
        ),
        (
            "fn result()->i32 { let ()=(1,2); return 0; }",
            "E-MATCH-ARITY",
        ),
        (
            "fn result()->i32 { let (a,a)=(1,2); return a; }",
            "E-MATCH-BINDING",
        ),
        (
            "fn result()->i32 { let (a,b):(i32,bool)=(1,2); return a; }",
            "E-TYPE-MISMATCH",
        ),
        (
            "fn result()->i32 { let (a,b)=(1,2); a=3; return a; }",
            "E-IMMUTABLE-ASSIGNMENT",
        ),
        (
            "fn result()->i32 { let (self,b)=(1,2); return b; }",
            "E-RECEIVER-RESERVED-NAME",
        ),
        (
            "fn result()->i32 { var (a,b)=(1,2); let f=()=>{a+=b;return a;}; return f(); }",
            "E-CLOSURE-MUTABILITY",
        ),
    ] {
        reject(text, code);
    }
    for text in [
        "fn result()->i32 { let (ref a,b)=(1,2); return b; }",
        "fn result()->i32 { let (ref mut a,b)=(1,2); return b; }",
        "fn result()->i32 { let (1,b)=(1,2); return b; }",
        "fn result()->i32 { var a=1; let (_,_) = (&mut a,2); return a; }",
    ] {
        let report = compile_source(&source(text), &CompileOptions::default());
        assert_eq!(
            report.status,
            ResultCategory::Unsupported,
            "{}",
            report.to_json()
        );
        assert!(!report.artifacts.contains_key("executable"));
    }
}
#[test]
fn tuple_binding_existing_ir_wire_and_independent_projection_check() {
    let src = source("fn result()->i32 { let (a,(b,_))=(1,(2,3)); return a+b; }");
    let ast = zil_syntax::parse(&src).into_result().unwrap();
    let core = zil_expand::expand(&src, &ast).unwrap();
    let target = TargetProfile::default();
    let limits = Limits::default();
    let typed = zil_types::check(&core, &target, &limits).unwrap();
    let plan = zil_analyze::analyze(&typed, &limits).unwrap();
    let bytes = zil_ir::wire::encode_module_candidate(&typed, &plan, &limits).unwrap();
    assert_eq!(
        zil_verify::verify_module_bytes(&bytes, &target, &limits)
            .unwrap()
            .module(),
        &typed
    );
    let mut damaged = typed.clone();
    let field = damaged.functions[0]
        .blocks
        .iter_mut()
        .flat_map(|block| &mut block.instructions)
        .find(|i| matches!(i.kind, InstructionKind::ExtractField { .. }))
        .unwrap();
    if let InstructionKind::ExtractField { index, .. } = &mut field.kind {
        *index = 99;
    }
    let mut claims = plan;
    claims.module_fingerprint = damaged.fingerprint();
    let error = zil_verify::verify_module(damaged, claims, &target, &limits).unwrap_err();
    assert_eq!(error[0].phase, Phase::Verify);
    assert!(error[0].code.starts_with("E-VERIFY-"));
}
#[test]
fn tuple_binding_pattern_work_has_exact_fresh_limit() {
    let src = source("fn result()->i32 { let (a,(b,_))=(1,(2,3)); return a+b; }");
    let ast = zil_syntax::parse(&src).into_result().unwrap();
    let core = zil_expand::expand(&src, &ast).unwrap();
    let target = TargetProfile::default();
    let succeeds = |steps| {
        zil_types::check(
            &core,
            &target,
            &Limits {
                max_analysis_steps: steps,
                ..Limits::default()
            },
        )
    };
    let (mut lo, mut hi) = (1, Limits::default().max_analysis_steps);
    assert!(succeeds(hi).is_ok());
    while lo < hi {
        let middle = lo + (hi - lo) / 2;
        if succeeds(middle).is_ok() {
            hi = middle;
        } else {
            lo = middle + 1;
        }
    }
    let failed = succeeds(lo - 1).unwrap_err();
    assert_eq!(failed[0].category, ResultCategory::ResourceLimit);
    assert!(succeeds(lo).is_ok());
    assert!(succeeds(lo).is_ok());
    eprintln!("tuple binding Types exact analysis limit: {lo}");
}
#[test]
fn record_defaults_context_and_inferred_updates() {
    native(
        "struct Pair { x: i64 = 7; y: i64 = 9; } fn result() -> i64 { let p: Pair = .{ y: 20 }; let q = .{ ...p, x: 4 }; return p.x * 100 + q.x * 10 + q.y; }",
        Some(760),
    );
}
#[test]
fn static_and_value_receiver_methods() {
    native(
        "struct Item { price: i64; quantity: i64; } impl Item { fn make(price: i64, quantity: i64) -> Self => Item { price, quantity }; fn total(self: Self) -> i64 => .price * .quantity; } fn result() -> i64 { let item = Item.make(7, 3); return item.total() + Item.total(Item.make(12, 4)); }",
        Some(69),
    );
}
#[test]
fn malformed_record_fields_and_nominal_mismatch() {
    for (text, code) in [
        (
            "struct P { x: i32; } fn result() -> i32 { let p=P { x:1,x:2 }; return p.x; }",
            "E-FIELD-DUPLICATE",
        ),
        (
            "struct P { x: i32; y:i32; } fn result() -> i32 { let p=P { x:1 }; return p.x; }",
            "E-FIELD-MISSING",
        ),
        (
            "struct P { x: i32; } fn result() -> i32 { let p=P { x:1 }; return p.y; }",
            "E-UNKNOWN-FIELD",
        ),
        (
            "struct P { x: i32; } struct Q { x:i32; } fn f(p:P)->i32{return p.x;} fn result()->i32{return f(Q{x:1});}",
            "E-TYPE-MISMATCH",
        ),
        (
            "struct P { x:P; } fn result()->i32{return 0;}",
            "E-INFINITE-LAYOUT",
        ),
        (
            "struct P { x:i32; } fn result()->i32{let p=P{x:1};p.x=2;return p.x;}",
            "E-IMMUTABLE-ASSIGNMENT",
        ),
        (
            "fn result()->i32{let p:[i32;2]=[1];return p[0];}",
            "E-ARRAY-LENGTH",
        ),
        (
            "fn result()->i32{let p=[1,2];return p[true];}",
            "E-INDEX-TYPE",
        ),
        (
            "fn result()->i32{let p=(1,2);return p.2;}",
            "E-UNKNOWN-FIELD",
        ),
    ] {
        reject(text, code);
    }
}
#[test]
fn signed_and_upper_bounds_reads_and_writes_trap() {
    for source in [
        "fn get(i:i64)->i32 {let values=[3,4];return values[i];} fn result()->i32{return get(-1);}",
        "fn get(i:i32)->i32 {let values=[3,4];return values[i];} fn result()->i32{return get(2);}",
        "fn result()->i32{var values=[3,4];values[2]=9;return values[0];}",
        "fn result()->i32{let values:[i32;0]=[];return values[0];}",
    ] {
        native(source, None);
    }
}
#[test]
fn instance_method_value_is_not_silently_bound() {
    reject(
        "struct P{x:i32;} impl P {fn get(self:Self)->i32=>.x;} fn result()->i32{let p=P{x:1};let f=p.get;return 0;}",
        "E-RECEIVER-METHOD-VALUE",
    );
}
#[test]
fn wire_roundtrip_and_independent_field_forgery_rejection() {
    let src = source(
        "struct P { x:i64; y:i64; } fn result()->i64 { var p=P{x:1,y:2};p.x=3;return p.y; }",
    );
    let ast = zil_syntax::parse(&src).into_result().unwrap();
    let core = zil_expand::expand(&src, &ast).unwrap();
    let target = TargetProfile::default();
    let limits = Limits::default();
    let typed = zil_types::check(&core, &target, &limits).unwrap();
    let plan = zil_analyze::analyze(&typed, &limits).unwrap();
    let bytes = zil_ir::wire::encode_module_candidate(&typed, &plan, &limits).unwrap();
    let checked = zil_verify::verify_module_bytes(&bytes, &target, &limits).unwrap();
    assert_eq!(checked.module(), &typed);
    for modification in 0..3 {
        let mut damaged = typed.clone();
        let f = &mut damaged.functions[0];
        if modification == 2 {
            let i = f
                .blocks
                .iter_mut()
                .flat_map(|b| &mut b.instructions)
                .find(|i| matches!(i.kind, InstructionKind::MakeAggregate { .. }))
                .unwrap();
            if let InstructionKind::MakeAggregate { fields } = &mut i.kind {
                fields.pop();
            }
        } else {
            let i = f
                .blocks
                .iter_mut()
                .flat_map(|b| &mut b.instructions)
                .find(|i| matches!(i.kind, InstructionKind::ExtractField { .. }))
                .unwrap();
            if modification == 0 {
                if let InstructionKind::ExtractField { index, .. } = &mut i.kind {
                    *index = 99;
                }
            } else {
                f.values[i.result.unwrap().0 as usize] = Type::Bool;
            }
        }
        let mut claims = plan.clone();
        claims.module_fingerprint = damaged.fingerprint();
        let errors = zil_verify::verify_module(damaged, claims, &target, &limits).unwrap_err();
        assert_eq!(errors[0].phase, Phase::Verify);
        assert!(errors[0].code.starts_with("E-VERIFY-"));
    }
}

#[test]
fn raw_aggregate_shapes_obey_work_and_depth_limits_before_identity_checks() {
    let src = source("fn result()->i32{return 3;}");
    let ast = zil_syntax::parse(&src).into_result().unwrap();
    let core = zil_expand::expand(&src, &ast).unwrap();
    let target = TargetProfile::default();
    let defaults = Limits::default();
    let typed = zil_types::check(&core, &target, &defaults).unwrap();
    let plan = zil_analyze::analyze(&typed, &defaults).unwrap();
    let mut nested = Type::I32;
    for _ in 0..32 {
        nested = Type::Tuple(vec![nested]);
    }
    for (shape, limits) in [
        (
            nested,
            Limits {
                max_call_depth: 16,
                ..defaults.clone()
            },
        ),
        (
            Type::Array {
                element: Box::new(Type::I32),
                length: u32::MAX,
            },
            Limits {
                max_analysis_steps: 1024,
                ..defaults.clone()
            },
        ),
        (
            Type::Record {
                name: "x".repeat(2048),
                fields: vec![],
            },
            Limits {
                max_analysis_steps: 1024,
                ..defaults.clone()
            },
        ),
    ] {
        let mut damaged = typed.clone();
        damaged.functions[0].values[0] = shape;
        let analyzed = zil_analyze::analyze(&damaged, &limits).unwrap_err();
        assert_eq!(analyzed[0].category, ResultCategory::ResourceLimit);
        // Keep the old identity claim: shape limits must be reached before
        // computing the raw candidate fingerprint or trusting any certificate.
        let checked =
            zil_verify::verify_module(damaged, plan.clone(), &target, &limits).unwrap_err();
        assert_eq!(checked[0].category, ResultCategory::ResourceLimit);
        assert_eq!(checked[0].phase, Phase::Verify);
    }
}

#[test]
fn large_copy_abi_copy_in_and_rhs_order() {
    let first = "[0]".repeat(13);
    let text = large_copy_program(&format!(
        "var original=a12;let pointer:fn(Big)->Big=keep;let returned=pointer(original);let observed=pair(original,touch(&mut original));return i32(returned{first})*100+i32(original{first})*10+observed;"
    ));
    let text = format!(
        "fn keep(value:Big)->Big{{var changed=value;changed{first}=9u8;return value}}fn pair(value:Big,marker:i32)->i32{{var changed=value;changed{first}=8u8;return i32(value{first})+marker}}{text}"
    );
    let llvm = native(&text, Some(344));
    assert!(llvm.contains(".lc1(ptr %lc1.result, ptr %arg.0)"));
    assert!(!llvm.contains("define internal void @zil.fn."));
}

#[test]
fn large_copy_abi_special_closure_is_explicitly_unsupported() {
    // Match the ordinary CLI's 8 MiB main-thread stack. The recursive closure
    // metadata check exceeds Rust's smaller default test-worker stack.
    std::thread::Builder::new()
        .stack_size(
            std::env::var("ZIL_333_CAPTURE_STACK_BYTES")
                .ok()
                .map(|n| n.parse().unwrap())
                .unwrap_or(8 * 1024 * 1024),
        )
        .spawn(|| {
            let first = "[0]".repeat(13);
            for (prefix, body) in [
                ("", "let f=(value:Big)->Big=>value;return 0;".to_owned()),
                ("", format!("let f=move()=>i32(a12{first});return f();")),
                (
                    "",
                    "let f=(pointer:fn(Big,i32)->i32)->i32=>0;return 0;".to_owned(),
                ),
                (
                    "",
                    "let pointer=inspect;let f=()=>{let held=pointer;return 0};return f();"
                        .to_owned(),
                ),
                (
                    "resource Holder{call:fn(Big,i32)->i32}",
                    "let holder=Holder{call:inspect};return 0;".to_owned(),
                ),
            ] {
                let text = format!("{prefix}{}", large_copy_program(&body));
                eprintln!(
                    "333 capture probe: Verify API begins; move_capture={}",
                    body.contains("move")
                );
                let verified = compile_source(&source(&text), &CompileOptions::default());
                assert_eq!(
                    verified.status,
                    ResultCategory::Success,
                    "{}",
                    verified.to_json()
                );
                eprintln!("333 capture probe: Verify API completed; Build API begins");
                let report = compile_source(
                    &source(&text),
                    &CompileOptions {
                        entry: Some("result".into()),
                        stop_after: Stage::Build,
                        ..CompileOptions::default()
                    },
                );
                assert_eq!(
                    report.status,
                    ResultCategory::Unsupported,
                    "{}",
                    report.to_json()
                );
                assert!(report.diagnostics.iter().any(|d| d.phase == Phase::Codegen));
                assert!(!report.artifacts.contains_key("llvm"));
                assert!(!report.artifacts.contains_key("executable"));
            }
        })
        .unwrap()
        .join()
        .unwrap();
}

#[test]
fn large_copy_abi_codegen_budget_exact_and_fresh() {
    let src = source(include_str!(
        "../../../tests/language-readiness/codegen-pending/large-array-a18-function-pointer.zil"
    ));
    let ast = zil_syntax::parse(&src).into_result().unwrap();
    let core = zil_expand::expand(&src, &ast).unwrap();
    let target = TargetProfile::default();
    let defaults = Limits::default();
    let typed = zil_types::check(&core, &target, &defaults).unwrap();
    let candidate = zil_analyze::analyze(&typed, &defaults).unwrap();
    let emit = |steps| {
        let limits = Limits {
            max_analysis_steps: steps,
            ..defaults.clone()
        };
        let verified =
            zil_verify::verify_module(typed.clone(), candidate.clone(), &target, &limits)?;
        let lir = zil_lower::lower(&verified)?;
        zil_backend_llvm::emit(&verified, &lir)
    };
    let (mut low, mut high) = (1, defaults.max_analysis_steps);
    assert!(emit(high).is_ok());
    while low < high {
        let middle = low + (high - low) / 2;
        if emit(middle).is_ok() {
            high = middle
        } else {
            low = middle + 1
        }
    }
    let errors = emit(low - 1).unwrap_err();
    assert_eq!(errors[0].category, ResultCategory::ResourceLimit);
    assert_eq!(errors[0].phase, Phase::Codegen);
    assert!(emit(low).unwrap().contains(".lc1"));
    assert!(emit(low).unwrap().contains(".lc1"));
    if cfg!(feature = "llvm-api") {
        let out = Temp::new();
        let options = CompileOptions {
            backend: zil_driver::NativeBackend::LlvmApi,
            stop_after: Stage::Build,
            object_cache: Some(out.0.join("objects")),
            output: Some(out.0.join("program")),
            ..CompileOptions::default()
        };
        let cold = compile_source(&src, &options);
        assert_eq!(cold.status, ResultCategory::Success, "{}", cold.to_json());
        assert!(cold.native_codegen.as_ref().unwrap().object_cache.misses > 0);
        let failed = compile_source(
            &src,
            &CompileOptions {
                limits: Limits {
                    max_analysis_steps: low - 1,
                    ..defaults.clone()
                },
                ..options.clone()
            },
        );
        assert_eq!(
            failed.status,
            ResultCategory::ResourceLimit,
            "{}",
            failed.to_json()
        );
        assert_eq!(failed.phase, Phase::Codegen);
        assert!(!failed.artifacts.contains_key("executable"));
        let warm = compile_source(&src, &options);
        assert_eq!(warm.status, ResultCategory::Success, "{}", warm.to_json());
        assert_eq!(warm.native_codegen.as_ref().unwrap().object_cache.misses, 0);
        assert!(warm.native_codegen.as_ref().unwrap().object_cache.hits > 0);
    }
    eprintln!("333 a18 function-pointer Codegen exact work limit: {low}");
}

#[test]
fn large_copy_abi_cgu_cache_and_canonical_function_reference() {
    use zil_driver::{
        CodegenOptions, NativeBackend, Optimization, ProjectSources, compile_project,
    };
    let mut ty = "[u8;1]".to_owned();
    let mut initialization = "let a0=[3u8];".to_owned();
    for n in 1..=12 {
        ty = format!("[{ty};2]");
        initialization.push_str(&format!("let a{n}=[a{},a{}];", n - 1, n - 1));
    }
    let first = "[0]".repeat(13);
    for backend in [NativeBackend::Clang, NativeBackend::LlvmApi] {
        if backend == NativeBackend::LlvmApi && !cfg!(feature = "llvm-api") {
            continue;
        }
        for optimization in [Optimization::O0, Optimization::O2] {
            let out = Temp::new();
            for (round, changed) in [(0, false), (1, false), (2, true)] {
                let body = if changed {
                    format!("var changed=value;changed{first}=9u8;return changed")
                } else {
                    "return value".into()
                };
                let modules=[
              ("main",format!("import model;import factory;fn main()->i32{{{initialization}let p=factory.pointer();let value=p(a12);return i32(value{first})}}")),
              ("model",format!("pub type Big={ty};pub fn identity(value:Big)->Big{{{body}}}")),
              ("factory","import model;pub fn pointer()->fn(model.Big)->model.Big{return model.identity}".into()),
            ];
                let project = ProjectSources {
                    root: ModuleId(0),
                    modules: modules
                        .iter()
                        .enumerate()
                        .map(|(n, (name, text))| {
                            SourceUnit::new(
                                FileId(n as u32),
                                ModuleId(n as u32),
                                format!("{name}.zil"),
                                text.clone(),
                            )
                        })
                        .collect(),
                    paths: [
                        (vec!["model".into()], ModuleId(1)),
                        (vec!["factory".into()], ModuleId(2)),
                    ]
                    .into_iter()
                    .collect(),
                };
                let report = compile_project(
                    &project,
                    &CompileOptions {
                        backend,
                        optimization,
                        incremental_o2: optimization == Optimization::O2,
                        stop_after: Stage::Build,
                        object_cache: (backend == NativeBackend::LlvmApi)
                            .then(|| out.0.join("objects")),
                        codegen: CodegenOptions { units: 3, jobs: 1 },
                        output: Some(out.0.join(format!("program{round}"))),
                        ..CompileOptions::default()
                    },
                );
                assert_eq!(
                    report.status,
                    ResultCategory::Success,
                    "{}",
                    report.to_json()
                );
                let actual = Command::new("timeout")
                    .args(["-s", "KILL", "5s"])
                    .arg(&report.artifacts["executable"])
                    .output()
                    .unwrap();
                assert_eq!(
                    actual.status.code(),
                    Some(if changed { 9 } else { 3 }),
                    "{actual:?}"
                );
                let codegen = report.native_codegen.as_ref().unwrap();
                assert!(codegen.actual_units >= 2, "{}", report.to_json());
                assert!(codegen.fallback.is_none(), "{}", report.to_json());
                if backend == NativeBackend::LlvmApi {
                    assert!(codegen.object_cache.enabled, "{}", report.to_json());
                    if round == 1 {
                        assert_eq!(codegen.object_cache.hits, codegen.actual_units);
                        assert_eq!(codegen.object_cache.misses, 0);
                    } else {
                        assert!(codegen.object_cache.misses > 0);
                    }
                }
                let llvm = fs::read_to_string(&report.artifacts["llvm"]).unwrap();
                assert!(llvm.contains(".lc1"));
            }
        }
    }
}

#[test]
fn large_copy_abi_record_tuple_unit_padding_roundtrip_all_backends() {
    use zil_driver::{NativeBackend, Optimization};
    let first = "[0]".repeat(13);
    let text = large_copy_program(&format!(
        "let pair=(a12,());var record=Envelope{{unit:(),flag:true,body:a12,tail:9}};let pointer:fn((Big,()),Envelope)->(Envelope,Big)=relay;let kept=pointer(pair,record);record.body{first}=7u8;return i32(kept.0.body{first})*1000+i32(kept.1{first})*100+i32(record.body{first})*10+i32(kept.0.tail);"
    ));
    let text = format!(
        "struct Envelope{{unit:();flag:bool;body:Big;tail:i64}}fn relay(pair:(Big,()),record:Envelope)->(Envelope,Big){{var changed=record;changed.body{first}=9u8;changed.tail=11;return (changed,pair.0)}}{text}"
    );
    for backend in [NativeBackend::Clang, NativeBackend::LlvmApi] {
        if backend == NativeBackend::LlvmApi && !cfg!(feature = "llvm-api") {
            continue;
        }
        for optimization in [Optimization::O0, Optimization::O2] {
            let out = Temp::new();
            let report = compile_source(
                &source(&text),
                &CompileOptions {
                    backend,
                    optimization,
                    entry: Some("result".into()),
                    stop_after: Stage::Build,
                    output: Some(out.0.join("program")),
                    ..CompileOptions::default()
                },
            );
            assert_eq!(
                report.status,
                ResultCategory::Success,
                "{}",
                report.to_json()
            );
            let observed = Command::new("timeout")
                .args(["-s", "KILL", "5s"])
                .arg(&report.artifacts["executable"])
                .output()
                .unwrap();
            assert!(observed.status.success(), "{observed:?}");
            assert_eq!(observed.stdout, b"9381\n");
            let llvm = fs::read_to_string(&report.artifacts["llvm"]).unwrap();
            assert!(llvm.contains(".lc1(ptr %lc1.result, ptr %arg.0, ptr %arg.1)"));
        }
    }
}

#[test]
fn large_copy_abi_unit_erasure_small_signature_and_void_return() {
    let first = "[0]".repeat(13);
    let text = large_copy_program(
        "var observed=0;consume((),a12,2,&mut observed);let scalar=small([4u8]);return observed+i32(scalar[0]);",
    );
    let text = format!(
        "fn consume(unit:(),value:Big,marker:i32,output:&mut i32)->(){{*output=i32(value{first})+marker;return ()}}fn small(value:[u8;1])->[u8;1]{{return value}}{text}"
    );
    let llvm = native(&text, Some(9));
    assert!(llvm.contains(".lc1(ptr %arg.1, i32 %arg.2, ptr %arg.3)"));
    let small = llvm
        .lines()
        .find(|line| line.starts_with("define [1 x i8] @zil.fn."))
        .unwrap();
    assert!(!small.contains(".lc1"));
}

#[test]
fn large_copy_abi_borrow_capture_keeps_pointer_environment() {
    // The captured source variable is Big, but its mutable borrowed environment
    // field is one pointer. LargeCopy guards must inspect the physical value.
    std::thread::Builder::new().stack_size(8*1024*1024).spawn(|| {
        use zil_driver::{NativeBackend,Optimization};
        let first="[0]".repeat(13);
        let text=large_copy_program(&format!("var original=a12;var f=()=>{{original{first}+=1u8;return i32(original{first})}};return f()*10+f();"));
        for backend in [NativeBackend::Clang,NativeBackend::LlvmApi] {
            if backend==NativeBackend::LlvmApi && !cfg!(feature="llvm-api") {continue;}
            for optimization in [Optimization::O0,Optimization::O2] {
                let out=Temp::new();
                let report=compile_source(&source(&text),&CompileOptions {backend,optimization,entry:Some("result".into()),stop_after:Stage::Build,output:Some(out.0.join("program")),..CompileOptions::default()});
                assert_eq!(report.status,ResultCategory::Success,"{}",report.to_json());
                let actual=Command::new("timeout").args(["-s","KILL","5s"]).arg(&report.artifacts["executable"]).output().unwrap();
                assert!(actual.status.success(),"{actual:?}");
                assert_eq!(actual.stdout,b"45\n");
            }
        }
    }).unwrap().join().unwrap();
}

#[test]
fn large_copy_abi_recursive_carrier_symbols_and_old_link_rejection() {
    use zil_driver::{NativeBackend, Optimization};
    let input = include_str!(
        "../../../tests/language-readiness/codegen-pending/large-copy-version-carriers.zil"
    );
    for backend in [NativeBackend::Clang, NativeBackend::LlvmApi] {
        if backend == NativeBackend::LlvmApi && !cfg!(feature = "llvm-api") {
            continue;
        }
        for optimization in [Optimization::O0, Optimization::O2] {
            let out = Temp::new();
            let report = compile_source(
                &source(input),
                &CompileOptions {
                    backend,
                    optimization,
                    stop_after: Stage::Build,
                    output: Some(out.0.join("program")),
                    ..CompileOptions::default()
                },
            );
            assert_eq!(
                report.status,
                ResultCategory::Success,
                "{}",
                report.to_json()
            );
            let llvm = fs::read_to_string(&report.artifacts["llvm"]).unwrap();
            for id in 0..=12 {
                let base = format!("@zil.fn.{id}");
                let expected = if id <= 8 {
                    format!("{base}.lc1(")
                } else {
                    format!("{base}(")
                };
                assert!(
                    llvm.lines()
                        .any(|line| line.starts_with("define ") && line.contains(&expected)),
                    "missing {expected}"
                );
            }
            assert!(llvm.contains("ptr @zil.fn.1.lc1()"));
            assert!(llvm.contains("i32 @zil.fn.2.lc1(ptr %arg.0)"));
            let actual = Command::new("timeout")
                .args(["-s", "KILL", "5s"])
                .arg(&report.artifacts["executable"])
                .output()
                .unwrap();
            assert_eq!(actual.status.code(), Some(0), "{actual:?}");
            let symbols = Command::new("nm")
                .arg("-g")
                .arg(&report.artifacts["executable"])
                .output()
                .unwrap();
            assert!(symbols.status.success());
            let symbols = String::from_utf8(symbols.stdout).unwrap();
            for id in 0..=8 {
                assert!(
                    symbols
                        .lines()
                        .any(|line| line.ends_with(&format!(" T zil.fn.{id}.lc1")))
                );
                assert!(
                    !symbols
                        .lines()
                        .any(|line| line.ends_with(&format!(" T zil.fn.{id}")))
                );
            }
            if backend == NativeBackend::Clang && optimization == Optimization::O0 {
                let object = out.0.join("definitions.o");
                let built = Command::new("clang")
                    .args(["-x", "ir", "-O0", "-c"])
                    .arg(&report.artifacts["llvm"])
                    .arg("-o")
                    .arg(&object)
                    .output()
                    .unwrap();
                assert!(built.status.success(), "{built:?}");
                for (suffix, success) in [("", false), (".lc1", true)] {
                    let probe = out.0.join("consumer.c");
                    fs::write(&probe,format!("extern void *factory(void) __asm__(\"zil.fn.1{suffix}\");void *consume_factory(void){{return factory();}}\n")).unwrap();
                    let linked = Command::new("clang")
                        .arg(&probe)
                        .arg(&object)
                        .arg("-o")
                        .arg(out.0.join("consumer"))
                        .output()
                        .unwrap();
                    assert_eq!(linked.status.success(), success, "{linked:?}");
                    if !success {
                        let error = String::from_utf8_lossy(&linked.stderr);
                        assert!(
                            error.contains("undefined reference") && error.contains("zil.fn.1"),
                            "{error}"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn review329_astra_symbolic_copy_does_not_grant_primitive_operations() {
    for (label, text) in [
        (
            "named_i32",
            "enum E<T:Copy>{V{x:T}}fn bad<i32:Copy>(e:E<i32>)->u8{match e{E.V{x:1}=>{return 1},E.V{x:_}=>{return 0}}}fn result()->i32{return 0}",
        ),
        (
            "tuple_i32",
            "enum E<T:Copy>{V(T)}fn bad<i32:Copy>(e:E<i32>)->u8{match e{E.V(1)=>{return 1},E.V(_)=>{return 0}}}fn result()->i32{return 0}",
        ),
        (
            "named_i64",
            "enum E<T:Copy>{V{x:T}}fn bad<i64:Copy>(e:E<i64>)->i32{match e{E.V{x:1}=>{return 1},E.V{x:_}=>{return 0}}}fn result()->i32{return 0}",
        ),
        (
            "tuple_i64",
            "enum E<T:Copy>{V(T)}fn bad<i64:Copy>(e:E<i64>)->i32{match e{E.V(1)=>{return 1},E.V(_)=>{return 0}}}fn result()->i32{return 0}",
        ),
        (
            "named_u8",
            "enum E<T:Copy>{V{x:T}}fn bad<u8:Copy>(e:E<u8>)->i32{match e{E.V{x:1}=>{return 1},E.V{x:_}=>{return 0}}}fn result()->i32{return 0}",
        ),
        (
            "tuple_u8",
            "enum E<T:Copy>{V(T)}fn bad<u8:Copy>(e:E<u8>)->i32{match e{E.V(1)=>{return 1},E.V(_)=>{return 0}}}fn result()->i32{return 0}",
        ),
        (
            "named_u16",
            "enum E<T:Copy>{V{x:T}}fn bad<u16:Copy>(e:E<u16>)->i32{match e{E.V{x:1}=>{return 1},E.V{x:_}=>{return 0}}}fn result()->i32{return 0}",
        ),
        (
            "tuple_u16",
            "enum E<T:Copy>{V(T)}fn bad<u16:Copy>(e:E<u16>)->i32{match e{E.V(1)=>{return 1},E.V(_)=>{return 0}}}fn result()->i32{return 0}",
        ),
        (
            "named_bool",
            "enum E<T:Copy>{V{x:T}}fn bad<bool:Copy>(e:E<bool>)->i32{match e{E.V{x:true}=>{return 1},E.V{x:_}=>{return 0}}}fn result()->i32{return 0}",
        ),
        (
            "tuple_bool",
            "enum E<T:Copy>{V(T)}fn bad<bool:Copy>(e:E<bool>)->i32{match e{E.V(true)=>{return 1},E.V(_)=>{return 0}}}fn result()->i32{return 0}",
        ),
        (
            "named_T",
            "enum E<T:Copy>{V{x:T}}fn bad<T:Copy>(e:E<T>)->i32{match e{E.V{x:1}=>{return 1},E.V{x:_}=>{return 0}}}fn result()->i32{return 0}",
        ),
        (
            "tuple_T",
            "enum E<T:Copy>{V(T)}fn bad<T:Copy>(e:E<T>)->i32{match e{E.V(1)=>{return 1},E.V(_)=>{return 0}}}fn result()->i32{return 0}",
        ),
        (
            "alias_i64",
            "enum E<T:Copy>{V{x:T}}type Alias<T>=E<T>;fn bad<i64:Copy>(e:Alias<i64>)->i32{match e{E.V{x:1}=>{return 1},E.V{x:_}=>{return 0}}}fn result()->i32{return 0}",
        ),
        (
            "nested_i64",
            "enum E<T:Copy>{V{x:(T,bool)}}fn bad<i64:Copy>(e:E<i64>)->i32{match e{E.V{x:(1,_)}=>{return 1},E.V{x:(_,_) }=>{return 0}}}fn result()->i32{return 0}",
        ),
        (
            "scalar_bool",
            "fn bad<bool:Copy>(b:bool)->i32{match b{true=>{return 1},false=>{return 0}}}fn result()->i32{return 0}",
        ),
        (
            "bool_construct",
            "enum E<T:Copy>{V{x:T}}fn bad<bool:Copy>()->E<bool>{return E::<bool>.V{x:true}}fn result()->i32{return 0}",
        ),
        (
            "bool_return",
            "fn bad<bool:Copy>()->bool{return true}fn result()->i32{return 0}",
        ),
        (
            "bool_condition",
            "fn bad<bool:Copy>(b:bool)->i32{if b{return 1}return 0}fn result()->i32{return 0}",
        ),
        (
            "i64_conversion",
            "fn bad<i64:Copy>(x:i64)->i32{return i32(x)}fn result()->i32{return 0}",
        ),
        (
            "i64_index",
            "fn bad<i64:Copy>(a:[i32;2],i:i64)->i32{return a[i]}fn result()->i32{return 0}",
        ),
        (
            "enum_shadow_named",
            "enum E{V{x:i32}}fn bad<E:Copy>(e:E)->i32{match e{E.V{x:x}=>{return x}}}fn result()->i32{return 0}",
        ),
        (
            "enum_shadow_tuple",
            "enum E{V(i32)}fn bad<E:Copy>(e:E)->i32{match e{E.V(x)=>{return x}}}fn result()->i32{return 0}",
        ),
        (
            "enum_shadow_construct",
            "enum E{V{x:i32}}fn bad<E:Copy>()->E{return E.V{x:1}}fn result()->i32{return 0}",
        ),
        (
            "struct_shadow_field",
            "struct P{x:i32}fn bad<P:Copy>(p:P)->i32{return p.x}fn result()->i32{return 0}",
        ),
        (
            "negative_slice",
            "fn bad<i64:Copy>(a:[i32;2],i:i64){let s=&a[i..i]}fn result()->i32{return 0}",
        ),
        (
            "negative_while",
            "fn bad<bool:Copy>(b:bool){while b{break}}fn result()->i32{return 0}",
        ),
        (
            "negative_conversion_output",
            "fn bad<i64:Copy>()->i64{return i64(1)}fn result()->i32{return 0}",
        ),
        (
            "negative_ordinary_call",
            "fn consume(x:i64){}fn bad<i64:Copy>(x:i64){consume(x)}fn result()->i32{return 0}",
        ),
        (
            "negative_unrelated_alias",
            "type Alias=i32;fn bad<bool:Copy>()->bool{return true}fn result()->i32{return 0}",
        ),
    ] {
        let report = compile_source(&source(text), &CompileOptions::default());
        assert!(
            matches!(
                report.status,
                ResultCategory::SafeReject | ResultCategory::Unsupported
            ),
            "{label}: {}",
            report.to_json()
        );
        assert_eq!(report.phase, Phase::Expand, "{label}: {}", report.to_json());
        assert_eq!(report.verification_runs, 0, "{label}");
    }
}

#[test]
fn review329_astra_lexical_builtin_controls() {
    for (label, text) in [
        (
            "legal_life_bool_type_i64",
            "fn keep<i64:Copy,life bool>(x:&'bool i64)->&'bool i64{return x}fn result()->i32{let x=1;return *keep(&x)}",
        ),
        (
            "legal_life_alias_type_i64",
            "type A=i32;fn keep<i64:Copy,life A>(x:&'A i64)->&'A i64{return x}fn result()->A{let x=1;return *keep(&x)}",
        ),
        (
            "legal_life_i64_only",
            "fn trigger<bool:Copy>(x:bool)->bool{return x}fn keep<life i64>(x:&'i64 i64)->&'i64 i64{return x}fn result()->i64{let x:i64=1;return *keep(&x)}",
        ),
        (
            "legal_concrete_named",
            "enum E<T:Copy>{V{x:T}}fn good<T:Copy>(unused:T,e:E<i64>)->i32{match e{E.V{x:1}=>{return 1},E.V{x:_}=>{return 0}}}fn result()->i32{return 0}",
        ),
        (
            "legal_dependent_named",
            "enum E<T:Copy>{V{x:T}}fn good<i64:Copy>(e:E<i64>)->i64{match e{E.V{x:x}=>{return x}}}fn result()->i32{return 0}",
        ),
        (
            "legal_bool_concrete",
            "enum E<T:Copy>{V{x:T}}fn good<T:Copy>(unused:T,e:E<bool>)->i32{match e{E.V{x:true}=>{return 1},E.V{x:false}=>{return 0}}}fn result()->i32{return 0}",
        ),
        (
            "legal_callee_i64",
            "fn scalar()->i64{return 1}fn keep<i64:Copy>(x:i64)->i32{let n=scalar();match n{1=>{return 1},_=>{return 0}}}fn result()->i32{return keep(true)}",
        ),
        (
            "legal_declared_i64_field",
            "enum E<T:Copy>{V{x:T,y:i64}}fn good<i64:Copy>(e:E<i64>)->i32{match e{E.V{x:_,y:1}=>{return 1},E.V{x:_,y:_}=>{return 0}}}fn result()->i32{return good(E::<bool>.V{x:true,y:i64(1)})}",
        ),
        (
            "legal_callee_bool",
            "fn scalar()->bool{return true}fn keep<bool:Copy>(x:bool)->i32{if scalar(){return 1}return 0}fn result()->i32{return keep(7)}",
        ),
        (
            "legal_renamed_identity",
            "fn keep<i64:Copy>(x:i64)->i64{return x}fn result()->i32{if keep::<bool>(true){return 1}return 0}",
        ),
        (
            "legal_alias_identity",
            "type A<i64>=i64;fn keep<bool:Copy>(x:A<bool>)->A<bool>{return x}fn result()->i32{return keep(1)}",
        ),
        (
            "legal_renamed_enum_binder",
            "enum E<bool:Copy>{V{x:bool,y:i64}}fn wrap<bool:Copy>(x:bool)->E<bool>{return E::<bool>.V{x:x,y:i64(7)}}fn result()->i64{match wrap(1){E.V{x:_,y:y}=>{return y}}}",
        ),
        (
            "legal_declared_i64_field_unsuffixed",
            "enum E<T:Copy>{V{x:T,y:i64}}fn good<i64:Copy>(e:E<i64>)->i32{match e{E.V{x:_,y:1}=>{return 1},E.V{x:_,y:_}=>{return 0}}}fn result()->i32{return good(E::<bool>.V{x:true,y:1})}",
        ),
        (
            "legal_renamed_enum_binder_unsuffixed",
            "enum E<bool:Copy>{V{x:bool,y:i64}}fn wrap<bool:Copy>(x:bool)->E<bool>{return E::<bool>.V{x:x,y:7}}fn result()->i64{match wrap(1){E.V{x:_,y:y}=>{return y}}}",
        ),
    ] {
        let report = compile_source(&source(text), &CompileOptions::default());
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{label}: {}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1, "{label}");
    }
}

#[test]
fn large_copy_enum_error_original_scale_all_backends() {
    // Isolate LLVM failures and cap each compiler process. This remains one
    // existing Cargo integration target and preserves the original input bytes.
    if let Ok(profile) = std::env::var("ZIL_338_SUM_CHILD") {
        use zil_driver::{NativeBackend, Optimization};
        let (backend, optimization) = match profile.as_str() {
            "clang-O0" => (NativeBackend::Clang, Optimization::O0),
            "clang-O2" => (NativeBackend::Clang, Optimization::O2),
            "llvm-api-O0" => (NativeBackend::LlvmApi, Optimization::O0),
            "llvm-api-O2" => (NativeBackend::LlvmApi, Optimization::O2),
            _ => panic!("unknown child profile"),
        };
        let out = Temp::new();
        let case = std::env::var("ZIL_338_CASE").unwrap();
        // Explicit manual KnownIncomplete reproduction only. The default test
        // cases below never invoke this API small-stack crash or enlarge it.
        if case == "capture-check" {
            let input = include_str!(
                "../../../tests/language-readiness/codegen-pending/large-copy-enum-error-borrow-capture.zil"
            );
            eprintln!(
                "338 borrowed Sum capture: Verify-only API begins (ordinary test worker stack)"
            );
            let report = compile_source(&source(input), &CompileOptions::default());
            eprintln!(
                "338 borrowed Sum capture: Verify-only API completed: {}",
                report.to_json()
            );
            return;
        }
        let input = match case.as_str() {
            "enum" => include_str!(
                "../../../tests/language-readiness/codegen-pending/large-copy-enum-error-enum-a18.zil"
            ),
            "error" => include_str!(
                "../../../tests/language-readiness/codegen-pending/large-copy-enum-error-error-a18.zil"
            ),
            "empty" => include_str!(
                "../../../tests/language-readiness/codegen-pending/large-copy-enum-error-enum-empty-a18.zil"
            ),
            "enum-indirect" => include_str!(
                "../../../tests/language-readiness/codegen-pending/large-copy-enum-error-enum-fnptr-a18.zil"
            ),
            "error-indirect" => include_str!(
                "../../../tests/language-readiness/codegen-pending/large-copy-enum-error-error-fnptr-a18.zil"
            ),
            _ => panic!("unknown ABI case"),
        };
        let report = compile_source(
            &source(input),
            &CompileOptions {
                backend,
                optimization,
                stop_after: Stage::Build,
                cancellation: zil_driver::CancellationToken::with_timeout(
                    std::time::Duration::from_secs(45),
                ),
                output: Some(out.0.join("program")),
                ..CompileOptions::default()
            },
        );
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        let llvm = fs::read_to_string(&report.artifacts["llvm"]).unwrap();
        assert!(llvm.contains(".lc1("), "versioned definition/call absent");
        assert!(llvm.contains("@zil.copy.bytes"));
        let codegen = report.native_codegen.as_ref().unwrap();
        assert_eq!(codegen.backend, backend);
        assert!(codegen.fallback.is_none());
        if backend == NativeBackend::LlvmApi {
            let pipeline = format!("\"pipeline\":\"default<{optimization:?}>\"");
            assert!(!codegen.object_reports.is_empty());
            assert!(codegen.object_reports.iter().all(|r| r.contains(&pipeline)));
        } else {
            let flag = if optimization == Optimization::O0 {
                "-O0"
            } else {
                "-O2"
            };
            assert!(
                report
                    .commands
                    .iter()
                    .any(|command| command.iter().any(|arg| arg == flag))
            );
        }
        let actual = Command::new("timeout")
            .args(["-s", "KILL", "5s"])
            .arg(&report.artifacts["executable"])
            .output()
            .unwrap();
        assert_eq!(actual.status.code(), Some(0), "{actual:?}");
        assert!(actual.stdout.is_empty());
        assert!(actual.stderr.is_empty());
        return;
    }
    for backend in ["clang", "llvm-api"] {
        if backend == "llvm-api" && !cfg!(feature = "llvm-api") {
            continue;
        }
        for optimization in ["O0", "O2"] {
            for case in ["enum", "error", "empty", "enum-indirect", "error-indirect"] {
                let actual = Command::new("timeout")
                    .args([
                        "-s",
                        "KILL",
                        "55s",
                        "bash",
                        "-c",
                        "ulimit -c 0; ulimit -t 45; ulimit -v 2097152; exec \"$@\"",
                        "--",
                    ])
                    .arg(std::env::current_exe().unwrap())
                    .args([
                        "--exact",
                        "large_copy_enum_error_original_scale_all_backends",
                        "--nocapture",
                        "--test-threads=1",
                    ])
                    .env("ZIL_338_CASE", case)
                    .env("ZIL_338_SUM_CHILD", format!("{backend}-{optimization}"))
                    .output()
                    .unwrap();
                assert!(
                    actual.status.success(),
                    "{backend}-{optimization}: {} {}",
                    String::from_utf8_lossy(&actual.stdout),
                    String::from_utf8_lossy(&actual.stderr)
                );
                eprintln!(
                    "338 a18 Sum ABI {case} {backend}-{optimization}: backend matched, requested pipeline, fallback=null, native_exit=0"
                );
            }
        }
    }
}

#[test]
fn large_copy_enum_error_value_order_alignment_and_canonical_small_payloads() {
    let first = "[0]".repeat(13);
    let body = format!(
        "var original=a12;var message:Message=Message.Data{{unit:(),flag:true,n:9,bytes:original}};let saved:Message=message;let score=read(message,empty(&mut message));let pointer:fn(Message)->Message=keep;let returned:Message=pointer(saved);original{first}=4u8;let small:Message=Message.Small{{pair:(true,7),unit:()}};let small_score=read(small,0);match returned{{Message.Data{{unit:_,flag:ok,n:n,bytes:bytes}}=>{{if !ok{{return -1}}return i32(bytes{first})*1000+i32(original{first})*100+i32(n)*10+score+small_score}},Message.Empty()=>{{return -2}},Message.Small{{pair:_,unit:_}}=>{{return -3}}}}"
    );
    let declarations = format!(
        "enum Message{{Data{{unit:(),flag:bool,n:i64,bytes:Big}},Empty,Small{{pair:(bool,i64),unit:()}}}}fn keep(value:Message)->Message{{var changed=value;changed=Message.Empty;return value}}fn empty(value:&mut Message)->i32{{*value=Message.Empty;return 1}}fn read(value:Message,marker:i32)->i32{{match value{{Message.Data{{unit:_,flag:_,n:_,bytes:bytes}}=>{{return i32(bytes{first})+marker}},Message.Empty()=>{{return 0}},Message.Small{{pair:(flag,n),unit:_}}=>{{if flag{{return i32(n)}}return -4}}}}}}"
    );
    let text = format!("{declarations}{}", large_copy_program(&body));
    review329_native(&[("main", &text)], 3501);
}

#[test]
fn large_copy_enum_error_both_arms_propagation_and_failure_cleanup() {
    use zil_driver::{NativeBackend, Optimization};
    let first = "[0]".repeat(13);
    let prefix = large_copy_program("");
    let prefix = &prefix[..prefix.find("fn result").unwrap()];
    let init = (0..=12)
        .map(|n| {
            if n == 0 {
                "let a0=[3u8];".to_owned()
            } else {
                format!("let a{n}=[a{},a{}];", n - 1, n - 1)
            }
        })
        .collect::<String>();
    for ok in [true, false] {
        let text = format!(
            r#"{prefix}extern "C" fn putchar(code:i32)->i32;resource Token{{code:i32}}impl Drop for Token{{fn drop(self:Self){{unsafe{{putchar(.code)}}}}}}struct Fault{{bytes:Big,flag:bool}}fn produce(ok:bool,bytes:Big)->Fault!Big{{if ok{{return bytes}}return Fault{{bytes,flag:true}}}}fn relay(value:Fault!Big)->Fault!Big{{return value}}fn work()->Fault!i32{{{init}let token=Token{{code:65}};var original=a12;let pointer:fn(Fault!Big)->Fault!Big=relay;let saved=pointer(produce({ok},original));original{first}=9u8;let bytes=saved?;return i32(bytes{first})}}fn main()->Fault!i32{{return work()?}}"#
        );
        for backend in [NativeBackend::Clang, NativeBackend::LlvmApi] {
            if backend == NativeBackend::LlvmApi && !cfg!(feature = "llvm-api") {
                continue;
            }
            for optimization in [Optimization::O0, Optimization::O2] {
                let out = Temp::new();
                let report = compile_source(
                    &source(&text),
                    &CompileOptions {
                        backend,
                        optimization,
                        stop_after: Stage::Build,
                        output: Some(out.0.join("program")),
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
                let observed = Command::new("timeout")
                    .args(["-s", "KILL", "5s"])
                    .arg(&report.artifacts["executable"])
                    .output()
                    .unwrap();
                assert_eq!(
                    observed.status.code(),
                    Some(if ok { 3 } else { 1 }),
                    "{observed:?}"
                );
                assert_eq!(observed.stdout, b"A");
                assert!(observed.stderr.is_empty());
                let llvm = fs::read_to_string(&report.artifacts["llvm"]).unwrap();
                assert!(llvm.contains(".lc1(ptr %lc1.result, ptr %arg.0)"));
                assert!(
                    !llvm.contains("extractvalue { i1, {"),
                    "large error must use memory projections"
                );
            }
        }
    }
}

#[test]
fn large_copy_enum_error_codegen_budget_exact_and_fresh() {
    let src = source(include_str!(
        "../../../tests/language-readiness/codegen-pending/large-copy-enum-error-enum-a18.zil"
    ));
    let ast = zil_syntax::parse(&src).into_result().unwrap();
    let core = zil_expand::expand(&src, &ast).unwrap();
    let target = TargetProfile::default();
    let defaults = Limits::default();
    let typed = zil_types::check(&core, &target, &defaults).unwrap();
    let candidate = zil_analyze::analyze(&typed, &defaults).unwrap();
    let emit = |steps| {
        let limits = Limits {
            max_analysis_steps: steps,
            ..defaults.clone()
        };
        let verified =
            zil_verify::verify_module(typed.clone(), candidate.clone(), &target, &limits)?;
        let lir = zil_lower::lower(&verified)?;
        zil_backend_llvm::emit(&verified, &lir)
    };
    let (mut low, mut high) = (1, defaults.max_analysis_steps);
    assert!(emit(high).is_ok());
    while low < high {
        let middle = low + (high - low) / 2;
        if emit(middle).is_ok() {
            high = middle
        } else {
            low = middle + 1
        }
    }
    let errors = emit(low - 1).unwrap_err();
    assert_eq!(errors[0].category, ResultCategory::ResourceLimit);
    assert_eq!(errors[0].phase, Phase::Codegen);
    assert!(emit(low).unwrap().contains(".lc1"));
    assert!(emit(low).unwrap().contains(".lc1"));
    if cfg!(feature = "llvm-api") {
        let out = Temp::new();
        let options = CompileOptions {
            backend: zil_driver::NativeBackend::LlvmApi,
            stop_after: Stage::Build,
            object_cache: Some(out.0.join("objects")),
            output: Some(out.0.join("program")),
            ..CompileOptions::default()
        };
        let cold = compile_source(&src, &options);
        assert_eq!(cold.status, ResultCategory::Success, "{}", cold.to_json());
        assert!(cold.native_codegen.as_ref().unwrap().object_cache.misses > 0);
        let failed = compile_source(
            &src,
            &CompileOptions {
                limits: Limits {
                    max_analysis_steps: low - 1,
                    ..defaults.clone()
                },
                ..options.clone()
            },
        );
        assert_eq!(
            failed.status,
            ResultCategory::ResourceLimit,
            "{}",
            failed.to_json()
        );
        assert_eq!(failed.phase, Phase::Codegen);
        assert!(!failed.artifacts.contains_key("executable"));
        let warm = compile_source(&src, &options);
        assert_eq!(warm.status, ResultCategory::Success, "{}", warm.to_json());
        assert_eq!(warm.native_codegen.as_ref().unwrap().object_cache.misses, 0);
        assert!(warm.native_codegen.as_ref().unwrap().object_cache.hits > 0);
    }
    eprintln!("338 a18 enum Codegen exact work limit: {low}");
}

#[test]
fn large_copy_enum_error_function_pointer_carriers_are_versioned() {
    let first = "[0]".repeat(13);
    let text = large_copy_program(&format!(
        "let handle:Handle=wrap();let pointer:Worker=unwrap(handle);let message:Message=pointer(Message.Data(a12));let raw=read_address(&message);match message{{Message.Data(bytes)=>{{return raw+i32(bytes{first})}},Message.Empty()=>{{return -1}}}}"
    ));
    let text = format!(
        "enum Message{{Data(Big),Empty}}type Worker=fn(Message)->Message;enum Handle{{Call(Worker),None}}fn relay(value:Message)->Message{{return value}}fn factory()->Worker{{return relay}}fn wrap()->Handle{{return Handle.Call(factory())}}fn unwrap(handle:Handle)->Worker{{match handle{{Handle.Call(pointer)=>{{return pointer}},Handle.None()=>{{return factory()}}}}}}fn read_address(value:&Message)->i32{{match *value{{Message.Data(bytes)=>{{return i32(bytes{first})}},Message.Empty()=>{{return 0}}}}}}{text}"
    );
    let llvm = native(&text, Some(6));
    for id in 0..=3 {
        assert!(
            llvm.lines()
                .any(|l| l.starts_with("define ") && l.contains(&format!("@zil.fn.{id}.lc1("))),
            "missing versioned {id}"
        );
    }
    assert!(llvm.contains("define i32 @zil.fn.4(ptr %arg.0)"));
    assert!(!llvm.contains("@zil.fn.4.lc1"));
}

#[test]
fn large_copy_enum_error_cgu_cache_and_canonical_function_reference() {
    use zil_driver::{
        CodegenOptions, NativeBackend, Optimization, ProjectSources, compile_project,
    };
    let mut ty = "[u8;1]".to_owned();
    let mut initialization = "let a0=[3u8];".to_owned();
    for n in 1..=12 {
        ty = format!("[{ty};2]");
        initialization.push_str(&format!("let a{n}=[a{},a{}];", n - 1, n - 1));
    }
    let first = "[0]".repeat(13);
    for backend in [NativeBackend::Clang, NativeBackend::LlvmApi] {
        if backend == NativeBackend::LlvmApi && !cfg!(feature = "llvm-api") {
            continue;
        }
        for optimization in [Optimization::O0, Optimization::O2] {
            let out = Temp::new();
            for (round, changed) in [(0, false), (1, false), (2, true)] {
                let body = if changed {
                    format!(
                        "match value{{Message.Data(bytes)=>{{var changed=bytes;changed{first}=9u8;return Message.Data(changed)}},Message.Empty()=>{{return Message.Empty}}}}"
                    )
                } else {
                    "return value".into()
                };
                let modules=[
              ("main",format!("import model;import factory;fn main()->i32{{{initialization}let p=factory.pointer();let value:model.Message=p(model.Message.Data(a12));match value{{model.Message.Data(bytes)=>{{return i32(bytes{first})}},model.Message.Empty()=>{{return 0}}}}}}")),
              ("model",format!("pub type Big={ty};pub enum Message{{Data(Big),Empty}}pub fn identity(value:Message)->Message{{{body}}}")),
              ("factory","import model;pub fn pointer()->fn(model.Message)->model.Message{return model.identity}".into()),
            ];
                let project = ProjectSources {
                    root: ModuleId(0),
                    modules: modules
                        .iter()
                        .enumerate()
                        .map(|(n, (name, text))| {
                            SourceUnit::new(
                                FileId(n as u32),
                                ModuleId(n as u32),
                                format!("{name}.zil"),
                                text.clone(),
                            )
                        })
                        .collect(),
                    paths: [
                        (vec!["model".into()], ModuleId(1)),
                        (vec!["factory".into()], ModuleId(2)),
                    ]
                    .into_iter()
                    .collect(),
                };
                let report = compile_project(
                    &project,
                    &CompileOptions {
                        backend,
                        optimization,
                        incremental_o2: optimization == Optimization::O2,
                        stop_after: Stage::Build,
                        object_cache: (backend == NativeBackend::LlvmApi)
                            .then(|| out.0.join("objects")),
                        codegen: CodegenOptions { units: 3, jobs: 1 },
                        output: Some(out.0.join(format!("program{round}"))),
                        ..CompileOptions::default()
                    },
                );
                assert_eq!(
                    report.status,
                    ResultCategory::Success,
                    "{}",
                    report.to_json()
                );
                let actual = Command::new("timeout")
                    .args(["-s", "KILL", "5s"])
                    .arg(&report.artifacts["executable"])
                    .output()
                    .unwrap();
                assert_eq!(
                    actual.status.code(),
                    Some(if changed { 9 } else { 3 }),
                    "{actual:?}"
                );
                let codegen = report.native_codegen.as_ref().unwrap();
                assert!(codegen.actual_units >= 2, "{}", report.to_json());
                assert!(codegen.fallback.is_none(), "{}", report.to_json());
                if backend == NativeBackend::LlvmApi {
                    assert!(codegen.object_cache.enabled, "{}", report.to_json());
                    if round == 1 {
                        assert_eq!(codegen.object_cache.hits, codegen.actual_units);
                        assert_eq!(codegen.object_cache.misses, 0);
                    } else {
                        assert!(codegen.object_cache.misses > 0);
                    }
                }
                let llvm = fs::read_to_string(&report.artifacts["llvm"]).unwrap();
                assert!(llvm.contains(".lc1"));
            }
        }
    }
}

#[test]
fn large_copy_enum_error_long_source_names_are_prepaid_before_emission() {
    let input = include_str!(
        "../../../tests/language-readiness/codegen-pending/large-copy-enum-error-enum-a12.zil"
    );
    let short = input
        .replace(
            "enum E{Data(Big),Empty}",
            "struct Pad{n:i64,flag:bool}enum E{Data(Big,Pad),Empty}",
        )
        .replace("E.Data(a12)", "E.Data(a12,Pad{n:5,flag:true})")
        .replace("E.Data(a)=>", "E.Data(a,p)=>");
    let suffix = "x".repeat(256);
    let long = short
        .replace("Pad", &format!("Pad{suffix}"))
        .replace("Data", &format!("Data{suffix}"))
        .replace("Empty", &format!("Empty{suffix}"))
        .replace("flag", &format!("flag{suffix}"));
    let mut costs = Vec::new();
    for text in [&short, &long] {
        let src = source(text);
        let ast = zil_syntax::parse(&src).into_result().unwrap();
        let core = zil_expand::expand(&src, &ast).unwrap();
        let limits = Limits::default();
        let target = TargetProfile::default();
        let typed = zil_types::check(&core, &target, &limits).unwrap();
        let candidate = zil_analyze::analyze(&typed, &limits).unwrap();
        let emit = |steps| {
            let limits = Limits {
                max_analysis_steps: steps,
                ..limits.clone()
            };
            let verified =
                zil_verify::verify_module(typed.clone(), candidate.clone(), &target, &limits)?;
            let lir = zil_lower::lower(&verified)?;
            zil_backend_llvm::emit(&verified, &lir)
        };
        let (mut low, mut high) = (1, limits.max_analysis_steps);
        assert!(emit(high).is_ok());
        while low < high {
            let mid = low + (high - low) / 2;
            if emit(mid).is_ok() {
                high = mid
            } else {
                low = mid + 1
            }
        }
        let errors = emit(low - 1).unwrap_err();
        assert_eq!(errors[0].phase, Phase::Codegen);
        assert_eq!(errors[0].category, ResultCategory::ResourceLimit);
        assert!(emit(low).is_ok());
        assert!(emit(low).is_ok());
        costs.push(low);
    }
    assert!(
        costs[1] > costs[0] + 4 * 256,
        "source metadata bytes must be in repeated emission reservations: {costs:?}"
    );
    eprintln!("338 enum source metadata Codegen exact limits short/long: {costs:?}");
}
