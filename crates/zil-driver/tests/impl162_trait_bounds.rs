//! Ordinary nominal bounds are checked symbolically and at each instantiation.
use std::{
    collections::BTreeMap,
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_base::{FileId, Limits, ModuleId, Phase, ResultCategory, SourceUnit};
use zil_driver::{
    CompileOptions, CompileReport, Optimization, ProjectSources, Stage, compile_project,
    compile_source,
};

static NEXT: AtomicUsize = AtomicUsize::new(0);
fn source(text: &str) -> SourceUnit {
    SourceUnit::new(FileId(0), ModuleId(0), "main.zil", text)
}
fn check(text: &str) -> CompileReport {
    compile_source(&source(text), &CompileOptions::default())
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
fn native(files: &[(&str, &str)]) {
    for optimization in [Optimization::O0, Optimization::O2] {
        let root = std::env::var_os("ZIL_TEST_REPORT_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        let dir = root.join(format!(
            "bounds162-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&dir).unwrap();
        for (name, text) in files {
            fs::write(dir.join(format!("{name}.zil")), text).unwrap();
        }
        let executable = dir.join("program");
        let options = CompileOptions {
            stop_after: Stage::Build,
            optimization,
            output: Some(executable.clone()),
            emit_dir: Some(dir.join("phases")),
            ..CompileOptions::default()
        };
        let report = if files.len() == 1 {
            compile_source(&source(files[0].1), &options)
        } else {
            compile_project(&project(files), &options)
        };
        fs::write(dir.join("compile.json"), report.to_json()).unwrap();
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1);
        let output = Command::new(&executable).output().unwrap();
        fs::write(dir.join("stdout"), &output.stdout).unwrap();
        fs::write(dir.join("stderr"), &output.stderr).unwrap();
        assert_eq!(output.status.code(), Some(0), "{output:?}");
    }
}
fn reject(text: &str, fragment: &str) {
    let report = check(text);
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
            .any(|d| d.message.contains(fragment)),
        "{}",
        report.to_json()
    );
}

#[test]
fn shared_bounds_dispatch_for_multiple_nominal_types_and_forward_symbolically() {
    native(&[(
        "main",
        r#"
trait Measure { fn measure(self:&Self)->i32; }
struct Point { value:i32 }
struct Area { width:i32,height:i32 }
impl Measure for Point { fn measure()->i32=>.value }
impl Measure for Area { fn measure()->i32=>.width*.height }
fn measured<T:Measure>(value:&T)->i32=>value.measure()
fn relay<U:Measure>(value:&U)->i32=>measured(value)
fn main()->i32 { let p=Point{value:7};let a=Area{width:5,height:7};return relay(&p)+measured::<Area>(&a)-42 }
"#,
    )]);
}

#[test]
fn mutable_multiple_bounds_and_value_receiver_keep_passing_modes() {
    native(&[(
        "main",
        r#"
trait Read { fn get(self:&Self)->i32; }
trait Change { fn set(self:&mut Self,n:i32); }
trait Consume { fn finish(self:Self)->i32; }
struct Counter { n:i32 }
impl Read for Counter { fn get()->i32=>.n }
impl Change for Counter { fn set(n:i32){.n=n} }
impl Consume for Counter { fn finish()->i32=>.n }
fn update<T:Read+Change>(value:&mut T,n:i32)->i32{value.set(n);return value.get()}
fn local<T:Read+Change>(value:T,n:i32)->i32{var x=value;x.set(n);return x.get()}
fn finish<T:Consume>(value:T)->i32=>value.finish()
fn main()->i32 {var c=Counter{n:1};if update(&mut c,20)!=20{return 1};if local(c,40)!=40{return 2};return finish(Counter{n:42})-42}
"#,
    )]);
}

#[test]
fn bound_method_named_len_and_self_parameters_are_symbolic() {
    native(&[(
        "main",
        r#"
trait Measure {fn len(self:&Self)->i32;fn plus(self:&Self,other:&Self)->i32;}
struct Number {n:i32}
impl Measure for Number {fn len()->i32=>.n;fn plus(other:&Self)->i32=>.n+other.n}
fn combine<T:Measure>(a:&T,b:&T)->i32=>a.len()+a.plus(b)
fn main()->i32{let a=Number{n:7};let b=Number{n:28};return combine(&a,&b)-42}
"#,
    )]);
}

#[test]
fn imported_public_bounds_keep_definition_and_nominal_module_identity() {
    native(&[
        (
            "main",
            "import left;import right;import helpers;fn main()->i32{let a=left.make(7);let b=right.make(35);return helpers.apply(&a)+right.apply(&b)-42}",
        ),
        (
            "left",
            "pub trait Measure{fn measure(self:&Self)->i32;}pub struct Value{n:i32}impl Measure for Value{fn measure()->i32=>.n}pub fn make(n:i32)->Value=>Value{n:n}",
        ),
        (
            "right",
            "pub trait Measure{fn measure(self:&Self)->i32;}pub struct Value{n:i32}impl Measure for Value{fn measure()->i32=>.n}pub fn make(n:i32)->Value=>Value{n:n};pub fn apply<T:Measure>(x:&T)->i32=>x.measure()",
        ),
        (
            "helpers",
            "import left;pub fn apply<T:left.Measure>(x:&T)->i32=>x.measure()",
        ),
    ]);
}

#[test]
fn same_named_inherent_method_does_not_satisfy_nominal_bound() {
    reject(
        "trait Read{fn get(self:&Self)->i32;}struct C{n:i32}impl C{fn get(self:&Self)->i32=>.n}fn read<T:Read>(x:&T)->i32=>x.get();fn main()->i32{let c=C{n:7};return read(&c)}",
        "does not satisfy nominal trait bound",
    );
}

#[test]
fn unused_definitions_cannot_gain_members_types_or_copy_from_instances() {
    for (body, reason) in [
        (
            "fn bad<T:Read>(x:&T)->i32=>x.missing()",
            "not provided by the declared trait bounds",
        ),
        (
            "fn bad<T:Read>(x:&T)->bool=>x.get()",
            "generic body types must agree",
        ),
        (
            "fn bad<T:Read>(x:&T)->i32=>x.get(true)",
            "argument count differs",
        ),
        ("fn bad<T:Read>(x:T)->(T,T)=>(x,x)", "used after move"),
        ("fn bad<T:Read>(x:T)->T=>x+1", "requires a declared bound"),
    ] {
        let text = format!("trait Read{{fn get(self:&Self)->i32;}}{body};fn main(){{}}");
        let report = check(&text);
        assert_ne!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert_eq!(report.phase, Phase::Expand, "{}", report.to_json());
        assert!(
            report
                .diagnostics
                .iter()
                .any(|d| d.message.contains(reason)),
            "{}",
            report.to_json()
        );
    }
}

#[test]
fn forwarding_requires_a_prior_bound_even_when_unused() {
    reject(
        "trait Read{fn get(self:&Self)->i32;}fn read<T:Read>(x:&T)->i32=>x.get();fn bad<U>(x:&U)->i32=>read(x);fn main(){}",
        "does not satisfy nominal trait bound",
    );
    reject(
        "trait Read{fn get(self:&Self)->i32;}struct T{n:i32}impl Read for T{fn get()->i32=>.n}fn read<U:Read>(x:&U)->i32=>x.get();fn bad<T>(x:&T)->i32=>read(x);fn main(){}",
        "does not satisfy nominal trait bound",
    );
}

#[test]
fn missing_trait_wrong_arguments_and_ambiguous_bounds_reject_before_specialization() {
    reject(
        "fn bad<T:Missing>(x:T)->T=>x;fn main(){}",
        "not a declared nominal trait",
    );
    reject(
        "trait Read{fn get(self:&Self,n:i32)->i32;}fn bad<T:Read>(x:&T)->i32=>x.get(true);fn main(){}",
        "generic body types must agree",
    );
    reject(
        "trait A{fn get(self:&Self)->i32;}trait B{fn get(self:&Self)->i32;}fn bad<T:A+B>(x:&T)->i32=>x.get();fn main(){}",
        "ambiguous between declared trait bounds",
    );
}

#[test]
fn borrowed_receivers_do_not_gain_mutation_or_move_permissions() {
    reject(
        "trait Change{fn set(self:&mut Self,n:i32);}fn bad<T:Change>(x:&T){x.set(1)}fn main(){}",
        "shared reference cannot grant exclusive access",
    );
    reject(
        "trait Change{fn set(self:&mut Self,n:i32);}fn bad<T:Change>(x:T){x.set(1)}fn main(){}",
        "requires a writable place",
    );
    reject(
        "trait Consume{fn take(self:Self)->i32;}fn bad<T:Consume>(x:&T)->i32=>x.take();fn main(){}",
        "cannot move an unknown non-Copy trait receiver",
    );
    reject(
        "trait Consume{fn take(self:Self)->i32;}fn bad<T:Consume>(x:T)->i32{x.take();return x.take()}fn main(){}",
        "used after move",
    );
}

#[test]
fn actual_impl_signatures_and_independent_loan_checks_still_run() {
    let report = check(
        "trait Read{fn get(self:&Self)->i32;}struct C{n:i32}impl Read for C{fn get()->bool=>true}fn read<T:Read>(x:&T)->i32=>x.get();fn main()->i32{let c=C{n:7};return read(&c)}",
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
            .any(|d| d.code == "E-TRAIT-SIGNATURE"),
        "{}",
        report.to_json()
    );
    let report = check(
        "trait Change{fn set(self:&mut Self,n:i32);}struct C{n:i32}impl Change for C{fn set(n:i32){.n=n}}fn update<T:Change>(x:&mut T){x.set(1)}fn main()->i32{var c=C{n:7};let old=&c;update(&mut c);return old.n}",
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
            .any(|d| d.code.starts_with("E-BORROW") || d.code.starts_with("E-ADDRESS")),
        "{}",
        report.to_json()
    );
}

#[test]
fn private_and_same_spelling_foreign_traits_cannot_satisfy_bound() {
    let files = [
        (
            "main",
            "import model;fn read<T:model.Secret>(x:&T)->i32=>x.get();fn main(){}",
        ),
        ("model", "trait Secret{fn get(self:&Self)->i32;}"),
    ];
    let r = compile_project(&project(&files), &CompileOptions::default());
    assert_eq!(r.status, ResultCategory::SafeReject, "{}", r.to_json());
    assert!(
        r.diagnostics.iter().any(|d| d.code.contains("PRIVATE")),
        "{}",
        r.to_json()
    );
    let files = [
        (
            "main",
            "import left;import right;fn main()->i32{let c=right.make();return left.read(&c)}",
        ),
        (
            "left",
            "pub trait Read{fn get(self:&Self)->i32;}pub fn read<T:Read>(x:&T)->i32=>x.get()",
        ),
        (
            "right",
            "pub trait Read{fn get(self:&Self)->i32;}pub struct C{n:i32}impl Read for C{fn get()->i32=>.n}pub fn make()->C=>C{n:7}",
        ),
    ];
    let r = compile_project(&project(&files), &CompileOptions::default());
    assert_eq!(r.status, ResultCategory::SafeReject, "{}", r.to_json());
    assert!(
        r.diagnostics
            .iter()
            .any(|d| d.message.contains("does not satisfy nominal trait bound")),
        "{}",
        r.to_json()
    );
}

#[test]
fn trait_bound_scans_obey_actual_work_budget() {
    let source = source(
        "trait Read{fn get(self:&Self)->i32;}struct C{n:i32}impl Read for C{fn get()->i32=>.n}fn read<T:Read>(x:&T)->i32=>x.get();fn main()->i32{let c=C{n:7};return read(&c)}",
    );
    let ast = zil_syntax::parse(&source).into_result().unwrap();
    assert!(zil_expand::expand_with_map_limits(&source, &ast, &Limits::default()).is_ok());
    let mut low = 0usize;
    let mut high = Limits::default().max_analysis_steps;
    while low + 1 < high {
        let middle = low + (high - low) / 2;
        let limits = Limits {
            max_analysis_steps: middle,
            ..Limits::default()
        };
        match zil_expand::expand_with_map_limits(&source, &ast, &limits) {
            Ok(_) => high = middle,
            Err(errors) => {
                assert_eq!(
                    errors[0].category,
                    ResultCategory::ResourceLimit,
                    "{errors:?}"
                );
                low = middle;
            }
        }
    }
    assert!(
        zil_expand::expand_with_map_limits(
            &source,
            &ast,
            &Limits {
                max_analysis_steps: high,
                ..Limits::default()
            }
        )
        .is_ok()
    );
    let errors = zil_expand::expand_with_map_limits(
        &source,
        &ast,
        &Limits {
            max_analysis_steps: high - 1,
            ..Limits::default()
        },
    )
    .unwrap_err();
    assert_eq!(errors[0].category, ResultCategory::ResourceLimit);
}
