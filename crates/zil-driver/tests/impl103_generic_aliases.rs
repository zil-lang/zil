//! Transparent aliases are checked through the ordinary production pipeline.
//! All evidence stays in ZIL_TEST_REPORT_DIR when the author/reviewer supplies it.
use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
};
use zil_base::{FileId, Limits, ModuleId, Phase, ResultCategory, SourceUnit, json_string};
use zil_driver::{
    CompileOptions, CompileReport, Optimization, ProjectSources, Stage, compile_path,
    compile_project, compile_source,
};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Case(PathBuf, bool);
impl Case {
    fn new(label: &str) -> Self {
        let evidence = std::env::var_os("ZIL_TEST_REPORT_DIR");
        let root = evidence
            .clone()
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        fs::create_dir_all(&root).unwrap();
        let path = root.join(format!(
            "alias103-{label}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path, evidence.is_some())
    }
    fn record(&self, report: &CompileReport) {
        fs::write(self.0.join("compile.json"), report.to_json()).unwrap();
    }
}
impl Drop for Case {
    fn drop(&mut self) {
        if !self.1 {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
}
fn source(text: &str) -> SourceUnit {
    SourceUnit::new(FileId(0), ModuleId(0), "alias103.zil", text)
}
fn check(text: &str) -> CompileReport {
    let case = Case::new("source");
    fs::write(case.0.join("source.zil"), text).unwrap();
    let report = compile_source(
        &source(text),
        &CompileOptions {
            stop_after: Stage::Lower,
            emit_dir: Some(case.0.join("phases")),
            ..CompileOptions::default()
        },
    );
    case.record(&report);
    report
}
fn success(report: &CompileReport) {
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1, "{}", report.to_json());
}
fn rejection(text: &str, status: ResultCategory, code: &str) {
    let report = check(text);
    assert_eq!(report.status, status, "{text}\n{}", report.to_json());
    assert_eq!(report.phase, Phase::Expand, "{text}\n{}", report.to_json());
    assert_eq!(
        report.diagnostics[0].code,
        code,
        "{text}\n{}",
        report.to_json()
    );
    assert!(!report.artifacts.contains_key("executable"));
}
fn same_rejection(aliased: &str, direct: &str) {
    let direct = check(direct);
    let aliased = check(aliased);
    assert!(
        matches!(
            direct.status,
            ResultCategory::SafeReject | ResultCategory::Unsupported
        ),
        "{}",
        direct.to_json()
    );
    assert_eq!(
        aliased.status,
        direct.status,
        "{}\n{}",
        aliased.to_json(),
        direct.to_json()
    );
    assert_eq!(
        aliased.phase,
        direct.phase,
        "{}\n{}",
        aliased.to_json(),
        direct.to_json()
    );
    assert_eq!(
        aliased.diagnostics[0].code,
        direct.diagnostics[0].code,
        "{}\n{}",
        aliased.to_json(),
        direct.to_json()
    );
    assert!(!aliased.artifacts.contains_key("executable"));
}
fn execute(case: &Case, binary: &Path, input: &[u8], output: &[u8], code: i32) {
    let prefix = case
        .0
        .join(format!("run-{}", NEXT.fetch_add(1, Ordering::Relaxed)));
    fs::write(
        prefix.with_extension("command.json"),
        format!(
            "{{\"command\":[{}]}}",
            json_string(&binary.to_string_lossy())
        ),
    )
    .unwrap();
    fs::write(prefix.with_extension("stdin"), input).unwrap();
    let mut child = Command::new(binary)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(input).unwrap();
    let result = child.wait_with_output().unwrap();
    fs::write(prefix.with_extension("stdout"), &result.stdout).unwrap();
    fs::write(prefix.with_extension("stderr"), &result.stderr).unwrap();
    fs::write(
        prefix.with_extension("status"),
        format!("{:?}\n", result.status),
    )
    .unwrap();
    assert_eq!(result.status.code(), Some(code), "{result:?}");
    assert_eq!(result.stdout, output);
    assert!(result.stderr.is_empty(), "{result:?}");
}
fn native(text: &str, expected: i64) {
    for optimization in [Optimization::O0, Optimization::O2] {
        let case = Case::new("native");
        fs::write(case.0.join("source.zil"), text).unwrap();
        let report = compile_source(
            &source(text),
            &CompileOptions {
                stop_after: Stage::Build,
                entry: Some("value".into()),
                output: Some(case.0.join("program")),
                emit_dir: Some(case.0.join("phases")),
                optimization,
                ..CompileOptions::default()
            },
        );
        case.record(&report);
        success(&report);
        execute(
            &case,
            &report.artifacts["executable"],
            &[],
            format!("{expected}\n").as_bytes(),
            0,
        );
    }
}

fn native_main(text: &str, expected_exit: i32) {
    for optimization in [Optimization::O0, Optimization::O2] {
        let case = Case::new("main");
        fs::write(case.0.join("source.zil"), text).unwrap();
        let report = compile_source(
            &source(text),
            &CompileOptions {
                stop_after: Stage::Build,
                output: Some(case.0.join("program")),
                emit_dir: Some(case.0.join("phases")),
                optimization,
                ..CompileOptions::default()
            },
        );
        case.record(&report);
        success(&report);
        execute(
            &case,
            &report.artifacts["executable"],
            &[],
            &[],
            expected_exit,
        );
    }
}

#[test]
fn type_arguments_compose_simultaneously_without_new_nominal_identity() {
    native(
        "type Id<T>=T;type Swap<A,B>=(B,A);type Twice<T>=(T,T);type Nested<T>=Twice<Id<T>>;type View<T>=&T;type Call<A,R>=fn(A)->R;fn inc(n:i64)->i64{return n+1}fn read(x:View<i64>)->i64{return *x}fn value()->i64{let p:Swap<bool,i64>=(20,true);let q:Nested<i64>=(p.0,21);let f:Call<i64,i64>=inc;return f(q.0)+read(&q.1)}",
        42,
    );
    native(
        "type A<T,U>=(T,U);type B<T>=A<i64,T>;type C<T>=B<T>;fn carry<T>(x:T)->C<T>{return (20,x)}fn value()->i64{let p=carry::<i64>(22);return p.0+p.1}",
        42,
    );
    native(
        "struct Cell<T>{value:T;}enum Option<T>{None,Some(T)}type Entry<T>=Cell<T>;type Maybe<T>=Option<Entry<T>>;type Fixed=Maybe<i64>;fn keep<T>(x:Entry<T>)->Entry<T>{return x}fn value()->i64{let raw=Entry::<i64>{value:42};let x:Fixed=Maybe::<i64>.Some(keep(raw));match x{Fixed.Some(v)=>{return v.value},Fixed.None()=>{return 0}}}",
        42,
    );
}

#[test]
fn aliases_preserve_lexical_type_and_value_shadowing() {
    native(
        "type T=i64;type Wrap<T>=T;type Pair<T>=(T,T);fn id<T>(x:Wrap<T>)->Wrap<T>{return x}fn value()->i64{let Pair=40;let f=(Wrap:i64)=>Wrap+2;if id::<bool>(true){return f(Pair)}return 0}",
        42,
    );
    success(&check(
        "type SelfAlias<T>=T;struct Thing{x:i64;}impl Thing(self:&Self){fn get()->SelfAlias<i64>{return .x}}fn value()->i64{let a=Thing{x:42};return a.get()}",
    ));
    native(
        "type Id<T>=T;type View<T>=&T;struct Thing{x:i64;}impl Thing {fn copied(self:View<Self>)->Id<Self>{return *self}}fn value()->i64{let a=Thing{x:42};return a.copied().x}",
        42,
    );
}

#[test]
fn definition_nominals_survive_alias_and_function_binder_names() {
    // The independent reviewer found this through compile_source. A project
    // normalization pass alone cannot protect the direct public API.
    native(
        "struct T{x:i64;}type Fixed<U>=T;type Wrap<T>=Fixed<T>;fn value()->i64{let x:Wrap<i64>=T{x:42};return x.x}",
        42,
    );
    native(
        "struct T{x:i64;}type Fixed=T;fn keep<T>(x:Fixed)->Fixed{return x}fn value()->i64{let x=keep::<i64>(T{x:42});return x.x}",
        42,
    );
    native(
        "enum T{N(i64),Empty}type Fixed<U>=T;type Wrap<T>=Fixed<T>;fn value()->i64{let x:Wrap<i64>=Wrap::<i64>.N(42);match x{T.N(n)=>{return n},T.Empty()=>{return 0}}}",
        42,
    );
}

#[test]
fn nominal_capture_cannot_accept_the_callers_primitive_instead() {
    same_rejection(
        "struct T{x:i64;}type Fixed<U>=T;type Wrap<T>=Fixed<T>;fn value()->i64{let x:Wrap<i64>=42;return x}",
        "struct Nominal{x:i64;}fn value()->i64{let x:Nominal=42;return x}",
    );
    same_rejection(
        "struct T{x:i64;}type Fixed=T;fn coerce<T>(x:Fixed)->T{return x}fn value()->i64{return coerce::<i64>(42)}",
        "struct Nominal{x:i64;}fn coerce<T>(x:Nominal)->T{return x}fn value()->i64{return coerce::<i64>(42)}",
    );
}

#[test]
fn builtin_type_names_and_value_names_are_not_captured_by_binders() {
    for text in [
        "type Fixed<T>=i64;type Wrap<i64>=Fixed<i64>;fn value()->i64{let x:Wrap<bool>=42;return x}",
        "type Fixed=i64;fn keep<i64>(x:Fixed)->Fixed{return x}fn value()->i64{return keep::<bool>(42)}",
        "type Mix<i64,bool>=(i64,bool);fn value()->i64{let x:Mix<bool,i64>=(true,42);return x.1}",
        "type Id<U>=U;fn keep<T>(T:Id<T>)->Id<T>{return T}fn value()->i64{return keep::<i64>(42)}",
    ] {
        native(text, 42);
        success(&check_project(&project(&[("main", text)])));
    }
    // A type binder must not rename a distinct value/function namespace.
    native(
        "type Id<T>=T;fn answer()->i64{return 42}fn call<answer>()->i64{return answer()}fn value()->i64{return call::<bool>()}",
        42,
    );
    for (aliased, direct) in [
        (
            "type Fixed<T>=i64;type Wrap<i64>=Fixed<i64>;fn value()->bool{let x:Wrap<bool>=true;return x}",
            "fn value()->bool{let x:i64=true;return x}",
        ),
        (
            "type Fixed=i64;fn coerce<i64>(x:Fixed)->i64{return x}fn value()->bool{return coerce::<bool>(true)}",
            "fn coerce<T>(x:i64)->T{return x}fn value()->bool{return coerce::<bool>(true)}",
        ),
    ] {
        same_rejection(aliased, direct);
        let ordinary = check(direct);
        let project = check_project(&project(&[("main", aliased)]));
        assert_eq!(project.status, ordinary.status, "{}", project.to_json());
        assert_eq!(project.phase, ordinary.phase, "{}", project.to_json());
        assert_eq!(project.diagnostics[0].code, ordinary.diagnostics[0].code);
    }
    rejection(
        "type Id<T>=T;fn bad<T,T>(x:T)->T{return x}fn value(){}",
        ResultCategory::SafeReject,
        "E-GENERIC-ARGUMENT",
    );
    rejection(
        "type Id<T>=T;fn keep<i64>(i64:Id<i64>)->Id<i64>{return i64}fn value()->i64{return keep::<i64>(42)}",
        ResultCategory::SafeReject,
        "E-INTEGER-CONVERSION-NAME",
    );
}

#[test]
fn declarations_and_phantom_arguments_are_checked_even_when_unused() {
    for text in [
        "type Bad<T>=Missing;fn value(){}",
        "type Bad<T>=T<i32>;fn value(){}",
        "type Bad<T,T>=T;fn value(){}",
        "type Id<T>=T;type Bad=Id;fn value(){}",
        "type Id<T>=T;type Bad=Id<i32,i64>;fn value(){}",
        "type Ignore<T>=i64;type Bad=Ignore<Missing>;fn value(){}",
        "type Ignore<T>=i64;type Bad=Ignore<i32<i64>>;fn value(){}",
        "type Id<T>=T;fn value(x:Id){}",
        "struct Cell<T>{x:T;}type Entry<T>=Cell<T>;fn value(){let x=Entry{x:1}}",
        "enum Option<T>{None,Some(T)}type Maybe<T>=Option<T>;fn value(x:Maybe<i64>){match x{Maybe.None()=>{},Maybe.Some(_)=>{}}}",
    ] {
        rejection(text, ResultCategory::SafeReject, "E-GENERIC-ARGUMENT");
    }
    for text in [
        "type Id<T:Copy>=T;fn value(){}",
        "type Id<life a,T>=T;fn value(){}",
        "type Fixed<T,#N:i32>=[T;N];fn value(){}",
        "type Id<T>=T;type Bad=Id<4>;fn value(){}",
        "type Ignore<T>=i64;type Bad=Ignore<true>;fn value(){}",
        "type Id<T>=T;fn value(){let x:Id<4>=0}",
        "#[doc=\"alias\"] type Id<T>=T;fn value(){}",
        "type Optional<T>=T?;fn value(){}",
    ] {
        rejection(text, ResultCategory::Unsupported, "E-UNSUPPORTED");
    }
}

#[test]
fn alias_cycles_include_argument_growth_and_erased_arguments() {
    for text in [
        "type A<T>=A<T>;fn value(){}",
        "type A<T>=B<T>;type B<T>=A<T>;fn value(){}",
        "type A<T>=A<(T,T)>;fn value(){}",
        "type Ignore<T>=i64;type A<T>=Ignore<A<T>>;fn value(){}",
        "type A<T>=B<A<T>>;type B<T>=T;fn value(){}",
    ] {
        rejection(text, ResultCategory::SafeReject, "E-GENERIC-ARGUMENT");
    }
    success(&check(
        "type Tail<T>=Pair<T>;type Pair<U>=(U,U);type Empty<T>=i64;fn value()->Tail<i64>{let x:Empty<bool>=21;return (x,x)}",
    ));
}

#[test]
fn alias_substitution_preserves_slice_callback_and_generic_constraints() {
    for text in [
        "type Slice<T>=[T];type Bad=Slice<&i32>;fn value(){}",
        "type Slice<T>=[T];type Bad=Slice<[i32]>;fn value(){}",
        "type Ignore<T>=i64;type Bad=Ignore<[&i32]>;fn value(){}",
    ] {
        rejection(text, ResultCategory::Unsupported, "E-UNSUPPORTED");
    }
    for text in [
        "type Callback<T>=extern \"C\" fn(T)->i32;type Bad=Callback<bool>;fn value(){}",
        "type Ignore<T>=i64;type Bad=Ignore<extern \"C\" fn(bool)->i32>;fn value(){}",
        "struct Record{x:i32;}type Callback<T>=extern \"C\" fn(T)->i32;type Bad=Callback<Record>;fn value(){}",
    ] {
        rejection(text, ResultCategory::SafeReject, "E-FFI-CALLBACK-TYPE");
    }
    same_rejection(
        "type Id<T>=T;fn wrong<T>(x:Id<T>)->Id<T>{return x+x}fn value(){}",
        "fn wrong<T>(x:T)->T{return x+x}fn value(){}",
    );
    same_rejection(
        "type Id<T>=T;fn constrained<T:Copy>(x:Id<T>)->Id<T>{return x}fn value(){}",
        "fn constrained<T:Copy>(x:T)->T{return x}fn value(){}",
    );
    native(
        "type Span<T>=&[T];fn first<T>(s:Span<T>)->Span<T>{return &s[0..1]}fn value()->i64{let a=[40,2];let s=first(&a[..]);return i64(s[0])+2}",
        42,
    );
}

#[test]
fn transparency_cannot_grant_copy_ownership_or_extend_a_borrow() {
    same_rejection(
        "type View<T>=&T;fn bad()->View<i64>{let x:i64=42;return &x}fn value(){}",
        "fn bad()->&i64{let x:i64=42;return &x}fn value(){}",
    );
    same_rejection(
        "type Id<T>=T;fn take(give x:Id<ByteBuffer>){}fn value(){let b=bytes_new();take(give b);bytes_len(b)}",
        "fn take(give x:ByteBuffer){}fn value(){let b=bytes_new();take(give b);bytes_len(b)}",
    );
    same_rejection(
        "type Id<T>=T;type View<T>=&[T];fn view(b:Id<ByteBuffer>)->View<u8>{return bytes_view(b)}fn value()->i64{let b=bytes_new();let s=view(b);bytes_push(b,1u8);return s.len()}",
        "fn view(b:ByteBuffer)->&[u8]{return bytes_view(b)}fn value()->i64{let b=bytes_new();let s=view(b);bytes_push(b,1u8);return s.len()}",
    );
    same_rejection(
        "type Outcome<E,T>=E!T;fn value()->Outcome<bool,ByteBuffer>{return bytes_new()}",
        "fn value()->bool!ByteBuffer{return bytes_new()}",
    );
    native(
        "type Id<T>=T;fn make()->Id<ByteBuffer> contract{result(create)}{return bytes_new()}fn value()->i64{let b=make();bytes_reserve(b,2);bytes_push(b,42u8);let s=bytes_view(b);return i64(s[0])}",
        42,
    );
}

fn project(files: &[(&str, &str)]) -> ProjectSources {
    ProjectSources {
        root: ModuleId(0),
        modules: files
            .iter()
            .enumerate()
            .map(|(i, (name, text))| {
                SourceUnit::new(
                    FileId(i as u32),
                    ModuleId(i as u32),
                    format!("{name}.zil"),
                    *text,
                )
            })
            .collect(),
        paths: files
            .iter()
            .enumerate()
            .skip(1)
            .map(|(i, (name, _))| (vec![name.to_string()], ModuleId(i as u32)))
            .collect::<BTreeMap<_, _>>(),
    }
}
fn check_project(input: &ProjectSources) -> CompileReport {
    let case = Case::new("project");
    for module in &input.modules {
        fs::write(
            case.0.join(format!("module{}.zil", module.module_id.0)),
            &module.text,
        )
        .unwrap();
    }
    let report = compile_project(
        input,
        &CompileOptions {
            stop_after: Stage::Lower,
            emit_dir: Some(case.0.join("phases")),
            ..CompileOptions::default()
        },
    );
    case.record(&report);
    report
}
#[test]
fn module_aliases_keep_definition_identity_and_private_boundaries() {
    let mut input = project(&[
        (
            "main",
            "import facade as f\nimport model as m\nimport other as same\nstruct Item<T>{value:T;}fn value()->i64{let x:f.Entry<i64>=m.Item::<i64>{value:42};let y:same.Item<i64>=x;return y.value}",
        ),
        ("model", "pub struct Item<T>{pub value:T;}"),
        ("facade", "import model\npub type Entry<T>=model.Item<T>"),
    ]);
    input.paths.insert(vec!["other".into()], ModuleId(1));
    success(&check_project(&input));
    for (files, code) in [
        (
            vec![
                ("main", "import model\nfn value(x:model.Entry<i64>){}"),
                ("model", "type Entry<T>=T"),
            ],
            "E-MODULE-PRIVATE",
        ),
        (
            vec![
                (
                    "main",
                    "import left\nimport right\nfn value(){left.consume(right.Item::<i64>{value:42})}",
                ),
                (
                    "left",
                    "pub struct Item<T>{pub value:T;}pub type Entry<T>=Item<T>;pub fn consume(x:Entry<i64>){}",
                ),
                ("right", "pub struct Item<T>{pub value:T;}"),
            ],
            "E-GENERIC-ARGUMENT",
        ),
        (
            vec![
                (
                    "main",
                    "import model\nfn value(){let x=model.Entry::<i64>{value:42}}",
                ),
                (
                    "model",
                    "pub struct Hidden<T>{value:T;}pub type Entry<T>=Hidden<T>",
                ),
            ],
            "E-FIELD-PRIVATE",
        ),
    ] {
        let report = check_project(&project(&files));
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{}",
            report.to_json()
        );
        assert_eq!(report.diagnostics[0].code, code, "{}", report.to_json());
    }
}

#[test]
fn aliases_feed_ct_and_callable_bound_signatures_without_runtime_authority() {
    native(
        "type Pair<T>=(T,T);fn pair(n:i64)->Pair<i64>{return(n,n+2)}fn value()->i64{let p:Pair<i64>=#(pair(20));return p.0+p.1}",
        42,
    );
    native(
        "type Id<T>=T;fn apply<F:Fn(Id<i64>)->Id<i64>>(f:&F,x:Id<i64>)->Id<i64>{return f(x)}fn value()->i64{let f=(n:i64)=>n+2;return apply(&f,40)}",
        42,
    );
    native_main(
        "type Result<E,T>=E!T;fn number()->Result<bool,i32>{return 42}fn main()->Result<bool,i32>{return number()?}",
        42,
    );
    native_main(
        "type Result<E,T>=E!T;fn number()->Result<bool,i32>{return true}fn main()->Result<bool,i32>{return number()?}",
        1,
    );
}

#[test]
fn expansion_preserves_exact_work_node_and_depth_boundaries() {
    let text = "type Double<T>=(T,T);type Four<T>=Double<Double<T>>;type Eight<T>=Double<Four<T>>;type End=Eight<(i64,bool)>;fn value(){}";
    let input = source(text);
    let parsed = zil_syntax::parse(&input).into_result().unwrap();
    let full = zil_expand::expand_with_map_limits(&input, &parsed, &Limits::default()).unwrap();
    let case = Case::new("budgets");
    fs::write(case.0.join("source.zil"), text).unwrap();
    let mut records = Vec::new();
    for field in ["work", "nodes", "depth"] {
        let probe = |n| {
            let mut limits = Limits::default();
            match field {
                "work" => limits.max_analysis_steps = n,
                "nodes" => limits.max_nodes = n,
                _ => limits.max_call_depth = n,
            }
            zil_expand::expand_with_map_limits(&input, &parsed, &limits)
        };
        let mut high = match field {
            "work" => Limits::default().max_analysis_steps,
            "nodes" => Limits::default().max_nodes,
            _ => Limits::default().max_call_depth,
        };
        let mut low = 0;
        assert!(probe(high).is_ok());
        while low + 1 < high {
            let middle = low + (high - low) / 2;
            match probe(middle) {
                Ok(_) => high = middle,
                Err(errors) => {
                    assert_eq!(
                        errors[0].category,
                        ResultCategory::ResourceLimit,
                        "{field} {middle} {errors:?}"
                    );
                    low = middle;
                }
            }
        }
        assert_eq!(probe(high).unwrap().core, full.core);
        let errors = probe(high - 1).unwrap_err();
        assert_eq!(errors[0].category, ResultCategory::ResourceLimit);
        assert_eq!(errors[0].phase, Phase::Expand);
        records.push(format!("{{\"dimension\":{},\"N\":{high},\"at_N\":\"Success\",\"at_N_minus_1\":\"ResourceLimit\"}}",json_string(field)));
    }
    fs::write(
        case.0.join("boundaries.json"),
        format!("[{}]", records.join(",")),
    )
    .unwrap();
    let growing = (0..12).fold("type A0<T>=T;".to_owned(), |mut s, i| {
        s.push_str(&format!("type A{}<T>=(A{}<T>,A{}<T>);", i + 1, i, i));
        s
    }) + "fn value(){}";
    let input = source(&growing);
    let parsed = zil_syntax::parse(&input).into_result().unwrap();
    let errors = zil_expand::expand_with_map_limits(
        &input,
        &parsed,
        &Limits {
            max_nodes: 4000,
            ..Limits::default()
        },
    )
    .unwrap_err();
    assert_eq!(errors[0].category, ResultCategory::ResourceLimit);
    fs::write(case.0.join("growth.zil"), growing).unwrap();
    fs::write(case.0.join("growth-rejection.txt"), format!("{errors:#?}")).unwrap();
}

#[test]
fn four_module_alias_consumer_executes_dynamic_inputs_at_o0_and_o2() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/generic_alias");
    for optimization in [Optimization::O0, Optimization::O2] {
        let case = Case::new("consumer");
        for name in ["main", "model", "facade", "transform"] {
            fs::copy(
                root.join(format!("{name}.zil")),
                case.0.join(format!("{name}.zil")),
            )
            .unwrap();
        }
        let report = compile_path(
            &case.0.join("main.zil"),
            &CompileOptions {
                stop_after: Stage::Build,
                output: Some(case.0.join("program")),
                emit_dir: Some(case.0.join("phases")),
                optimization,
                ..CompileOptions::default()
            },
        );
        case.record(&report);
        success(&report);
        for byte in [0u8, 1, 65, 127, 128, 255] {
            execute(
                &case,
                &report.artifacts["executable"],
                &[byte],
                &[((2 * u16::from(byte) + 42) % 256) as u8],
                0,
            );
        }
        execute(&case, &report.artifacts["executable"], &[], &[], 1);
    }
}
