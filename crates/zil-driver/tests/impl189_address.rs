//! Actual callback parameters retain their one-call address qualifications.
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_base::{FileId, ModuleId, Phase, ResultCategory, SourceUnit};
use zil_driver::{
    CompileOptions, CompileReport, Optimization, ProjectSources, Stage, compile_project,
};

static NEXT: AtomicUsize = AtomicUsize::new(0);
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
fn directory() -> PathBuf {
    let base = std::env::var_os("ZIL_TEST_REPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let dir = base.join(format!(
        "address189-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&dir).unwrap();
    dir
}
fn compile(files: &[(&str, &str)], options: CompileOptions, dir: &Path) -> CompileReport {
    for (name, text) in files {
        fs::write(dir.join(format!("{name}.zil")), text).unwrap();
    }
    let report = compile_project(
        &project(files),
        &CompileOptions {
            emit_dir: Some(dir.join("phases")),
            ..options
        },
    );
    fs::write(dir.join("compile.json"), report.to_json()).unwrap();
    report
}
fn check(source: &str) -> CompileReport {
    compile(&[("main", source)], CompileOptions::default(), &directory())
}
fn native(files: &[(&str, &str)], expected_stdout: &[u8], exit: i32) {
    for optimization in [Optimization::O0, Optimization::O2] {
        let dir = directory();
        let executable = dir.join("program");
        let report = compile(
            files,
            CompileOptions {
                stop_after: Stage::Build,
                optimization,
                output: Some(executable.clone()),
                ..CompileOptions::default()
            },
            &dir,
        );
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1);
        let output = Command::new(executable).output().unwrap();
        fs::write(dir.join("stdout"), &output.stdout).unwrap();
        fs::write(dir.join("stderr"), &output.stderr).unwrap();
        fs::write(dir.join("exit.txt"), format!("{:?}", output.status.code())).unwrap();
        assert_eq!(output.status.code(), Some(exit), "{output:?}");
        assert_eq!(output.stdout, expected_stdout, "{output:?}");
        assert!(output.stderr.is_empty(), "{output:?}");
    }
}
fn conflict(source: &str) {
    let report = check(source);
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
    assert_eq!(report.phase, Phase::Verify, "{}", report.to_json());
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.code == "E-BORROW-CONFLICT"),
        "{}",
        report.to_json()
    );
}

#[test]
fn direct_shared_and_exclusive_arguments_reborrow_the_same_input_sequentially() {
    native(
        &[(
            "main",
            "fn main()->i32{var n=20;let p=&mut n;let read=(q:&i32)->i32=>*q;let bump=(q:&mut i32)->i32=>{*q+=1;return *q};let old=read(p);let a=bump(p);let b=bump(p);return old+a+b+n-85}",
        )],
        b"",
        0,
    );
}

#[test]
fn one_signature_is_inferred_from_body_and_actual_address_calls() {
    native(
        &[(
            "main",
            "fn main()->i32{var n=41;let read=(p)=>*p;let bump=(p)=>{*p+=1;return *p};let old=read(&n);let fresh=bump(&mut n);return old+fresh+n-125}",
        )],
        b"",
        0,
    );
    let mismatch = check("fn main(){let a:i32=1;let b:i64=2;let f=(p)=>*p;f(&a);f(&b);}");
    assert_eq!(
        mismatch.status,
        ResultCategory::SafeReject,
        "{}",
        mismatch.to_json()
    );
    assert_eq!(mismatch.phase, Phase::Types, "{}", mismatch.to_json());
}

#[test]
fn multiple_shared_arguments_may_alias_and_exclusive_arguments_use_distinct_storage() {
    native(
        &[(
            "main",
            "fn main()->i32{var a=20;let b=2;let sum=(p:&i32,q:&i32)=>*p+*q;let add=(p:&mut i32,q:&i32)=>{*p+=*q};let before=sum(&a,&a);add(&mut a,&b);return before+a-62}",
        )],
        b"",
        0,
    );
    conflict(
        "fn main()->i32{var a=20;let f=(p:&mut i32,q:&i32)=>{*p+=1;return *q};return f(&mut a,&a)}",
    );
    conflict(
        "fn main()->i32{var a=20;let f=(p:&mut i32,q:&mut i32)=>{*p+=1;return *q};return f(&mut a,&mut a)}",
    );
}

#[test]
fn copy_aggregate_addresses_use_actual_inline_layout() {
    native(
        &[(
            "main",
            "struct Pair{x:i32;y:i64}fn main()->i32{var pair=Pair{x:20,y:22};let read=(p:&Pair)=>i64(p.x)+p.y;let edit=(p:&mut Pair)=>{p.x+=1};let old=read(&pair);edit(&mut pair);var values:[i32;2]=[1,2];let set=(p:&mut [i32;2])=>{p[0]+=p[1]};set(&mut values);return i32(old)+pair.x+values[0]-66}",
        )],
        b"",
        0,
    );
}

#[test]
fn cross_module_opaque_factory_and_borrowed_helper_preserve_parameter_types() {
    native(
        &[
            (
                "main",
                "import factory;import helpers;fn main()->i32{var n=20;let f=factory.make(1);let total=helpers.twice(&f,&mut n);return total+n-65}",
            ),
            (
                "factory",
                "pub fn make(step:i32)->impl Fn(&mut i32)->i32{return move(p:&mut i32)=>{*p+=step;return *p}}",
            ),
            (
                "helpers",
                "pub fn twice<F:Fn(&mut i32)->i32>(f:&F,p:&mut i32)->i32{return f(p)+f(p)}",
            ),
        ],
        b"",
        0,
    );
}

#[test]
fn mutable_and_consuming_environments_keep_their_own_independent_modes() {
    native(
        &[(
            "main",
            "fn apply<F:FnMut(&mut i32)->i32>(f:&mut F,p:&mut i32)->i32{return f(p)+f(p)}fn main()->i32{var step=0;var f=move(p:&mut i32)=>{step+=1;*p+=step;return *p};var n=20;let total=apply(&mut f,&mut n);return total+n-67}",
        )],
        b"",
        0,
    );
    native(
        &[(
            "main",
            "extern \"C\" fn putchar(n:i32)->i32;resource R{n:i32}impl Drop for R{fn drop(self:Self){unsafe{putchar(65)}}}fn eat(give r:R){}fn main()->i32{let r=R{n:2};let f=move(p:&mut i32)=>{*p+=r.n;eat(give r);return *p};var n=40;return f(&mut n)-42}",
        )],
        b"A",
        0,
    );
}

#[test]
fn current_call_includes_desc_capture_and_explicit_shared_argument() {
    for (helper, call) in [
        ("", "f(q)"),
        (
            "fn apply<F:Fn(&i32)->i32>(f:&F,p:&i32)->i32{return f(p)}",
            "apply(&f,q)",
        ),
    ] {
        let same = format!(
            "resource R{{n:i32}}{helper}fn main()->i32{{let r=R{{n:42}};let d=r;let h=&r;let q=&h.n;let f=(p:&i32)->i32=>{{d.n=43;return *p}};return {call}-42}}"
        );
        conflict(&same);
        let other = same.replace("let h=&r;let q=&h.n", "let other=42;let q=&other");
        native(&[("main", &other)], b"", 0);
        let shared = same
            .replace("d.n=43;return *p", "return d.n+*p")
            .replace("-42}", "-84}");
        native(&[("main", &shared)], b"", 0);
    }
}

#[test]
fn current_call_includes_captured_address_qualification() {
    for call in ["f(&mut n)", "apply(&f,&mut n)"] {
        let source = format!(
            "fn apply<F:Fn(&mut i32)->i32>(f:&F,p:&mut i32)->i32{{return f(p)}}fn main()->i32{{var n=3;let saved=&n;let f=move(p:&mut i32)->i32=>{{*p+=1;return *saved}};return {call}}}"
        );
        conflict(&source);
        let other = source
            .replace("let saved=&n", "var other=4;let saved=&n")
            .replace("f(&mut n)", "f(&mut other)")
            .replace("apply(&f,&mut n)", "apply(&f,&mut other)")
            .replace("return f(&mut other)", "return f(&mut other)-3")
            .replace(
                "return apply(&f,&mut other)",
                "return apply(&f,&mut other)-3",
            );
        native(&[("main", &other)], b"", 0);
    }
}

#[test]
fn byte_description_writes_are_checked_against_explicit_element_windows() {
    for (helper, call) in [
        ("", "f(p)"),
        (
            "fn apply<F:Fn(&u8)->i32>(f:&F,p:&u8)->i32{return f(p)}",
            "apply(&f,p)",
        ),
    ] {
        let same = format!(
            "{helper}fn main()->i32{{let b=bytes_new();bytes_reserve(b,1);bytes_push(b,u8(7));let view=bytes_view(b);let p=&view[0];let f=(q:&u8)->i32=>{{bytes_reserve(b,8);return i32(*q)}};return {call}-7}}"
        );
        conflict(&same);
        let other = same.replace(
            "let view=bytes_view(b);let p=&view[0]",
            "let other=u8(7);let p=&other",
        );
        native(&[("main", &other)], b"", 0);
        let readonly = same.replace("bytes_reserve(b,8)", "bytes_len(b)");
        native(&[("main", &readonly)], b"", 0);
    }
    let original187 = check("fn main(){let b=bytes_new();let f=(n:&i32)=>i32(bytes_len(b))+*n}");
    assert_eq!(
        original187.status,
        ResultCategory::Success,
        "{}",
        original187.to_json()
    );
    assert_eq!(original187.verification_runs, 1);
}

#[test]
fn helper_extra_address_arguments_and_zero_call_paths_keep_actual_effects() {
    let source = "resource R{n:i32}fn after<F:Fn()->i32>(f:&F,p:&i32)->i32{return f()+*p}fn main()->i32{let r=R{n:21};let d=r;let h=&r;let p=&h.n;let f=()=>{d.n=22;return 0};return after(&f,p)-21}";
    conflict(source);
    let other = source.replace("let h=&r;let p=&h.n", "let other=21;let p=&other");
    native(&[("main", &other)], b"", 0);
    let idle = source.replace("return f()+*p", "return *p");
    native(&[("main", &idle)], b"", 0);
    native(
        &[(
            "main",
            "fn id(p:&i32)->&i32{return p}fn main()->i32{let n=42;let f=(p:&i32)=>*id(p);return f(&n)-42}",
        )],
        b"",
        0,
    );
}

#[test]
fn every_environment_call_form_checks_the_current_argument_window() {
    for (setup, invocation) in [
        ("let f=(p:&i32)=>{d.n=43;return *p}", "f(q)"),
        (
            "let token=R{n:1};let f=move(p:&i32)=>{token.n;d.n=43;return *p}",
            "f(q)",
        ),
        (
            "var step=0;var f=move(p:&i32)=>{step+=1;d.n=43;return *p}",
            "f(q)",
        ),
        (
            "let token=R{n:1};let f=move(p:&i32)=>{eat(give token);d.n=43;return *p}",
            "f(q)",
        ),
        (
            "let f=(p:&i32)=>{d.n=43;return *p};let borrowed=&f",
            "borrowed(q)",
        ),
    ] {
        let same = format!(
            "resource R{{n:i32}}fn eat(give r:R){{}}fn main()->i32{{let r=R{{n:42}};let d=r;let h=&r;let q=&h.n;{setup};return {invocation}-42}}"
        );
        conflict(&same);
        let other = same.replace("let h=&r;let q=&h.n", "let other=42;let q=&other");
        native(&[("main", &other)], b"", 0);
    }
}

#[test]
fn unused_generic_helpers_are_checked_and_shared_inputs_cannot_grant_mutability() {
    let valid = check("fn apply<F:Fn(&i32)->i32>(f:&F,p:&i32)->i32{return f(p)}fn main(){}");
    assert_eq!(valid.status, ResultCategory::Success, "{}", valid.to_json());
    for source in [
        "fn apply<F:Fn(&mut i32)->i32>(f:&F,p:&i32)->i32{return f(p)}fn main(){}",
        "fn main(){let n=3;let f=(p:&mut i32)=>{*p+=1};f(&n);}",
        "fn main(){let n=3;let f=(p:&i32)=>{*p+=1};f(&n);}",
    ] {
        let report = check(source);
        assert_ne!(
            report.status,
            ResultCategory::Success,
            "{source}: {}",
            report.to_json()
        );
    }
}

#[test]
fn address_parameter_results_and_unsupported_shapes_remain_closed() {
    for source in [
        "fn main(){let n=3;let f=(p:&i32)->&i32=>p;let q=f(&n);}",
        "fn make()->impl Fn(&i32)->&i32{let n=3;return move(p:&i32)=>&n}fn main(){}",
        "fn main(){let f=(p:&[i32])=>p.len();}",
        "resource R{n:i32}fn main(){let f=(p:&R)=>p.n;}",
        "resource R{n:i32}package P{manages r:R;n:i32}fn main(){let f=(p:&P)=>p.n;}",
        "fn main(){let f=(p:&(&i32))=>**p;}",
        "fn main(){let f:fn(&i32)->i32=(p:&i32)=>*p;}",
    ] {
        let report = check(source);
        assert_ne!(
            report.status,
            ResultCategory::Success,
            "{source}: {}",
            report.to_json()
        );
        assert_eq!(report.phase, Phase::Types, "{source}: {}", report.to_json());
    }
    // Existing184 environment lending remains a separate, fully executable lane.
    native(
        &[(
            "main",
            "fn make(n:i32)->impl Fn()->&i32{return move()=>&n}fn main()->i32{let f=make(42);return *f()-42}",
        )],
        b"",
        0,
    );
}
