//! Local nominal trait declarations -> ordinary calls -> independent Verify/native.
use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_base::{FileId, ModuleId, ResultCategory, SourceUnit};
use zil_driver::{CompileOptions, CompileReport, Optimization, Stage, compile_source};
static NEXT: AtomicUsize = AtomicUsize::new(0);
fn compile(source: &str) -> CompileReport {
    compile_source(
        &SourceUnit::new(FileId(0), ModuleId(0), "main.zil", source),
        &CompileOptions::default(),
    )
}
fn native(source: &str) {
    for optimization in [Optimization::O0, Optimization::O2] {
        let root = std::env::var_os("ZIL_TEST_REPORT_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        let dir = root.join(format!(
            "traits147-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("main.zil"), source).unwrap();
        let executable = dir.join("program");
        let r = compile_source(
            &SourceUnit::new(FileId(0), ModuleId(0), "main.zil", source),
            &CompileOptions {
                stop_after: Stage::Build,
                optimization,
                output: Some(executable.clone()),
                emit_dir: Some(dir.join("phases")),
                ..CompileOptions::default()
            },
        );
        fs::write(dir.join("compile.json"), r.to_json()).unwrap();
        assert_eq!(r.status, ResultCategory::Success, "{}", r.to_json());
        assert_eq!(r.verification_runs, 1);
        let out = Command::new(executable).output().unwrap();
        fs::write(dir.join("stdout"), &out.stdout).unwrap();
        fs::write(dir.join("stderr"), &out.stderr).unwrap();
        assert_eq!(out.status.code(), Some(0), "{out:?}");
    }
}
#[test]
fn nominal_impls_inherit_read_receiver_and_dispatch_by_concrete_type() {
    native(
        r#"
trait Measure{fn measure(self:&Self)->i32;}
struct Point{x:i32}
struct Area{w:i32,h:i32}
impl Measure for Point{fn measure()->i32{return .x}}
impl Measure for Area{fn measure(self:&Self)->i32{return .w * .h}}
fn main()->i32{let p=Point{x:7};let r=Area{w:5,h:7};if p.measure()+r.measure()!=42{return 1};return Point.measure(&p)-7}
"#,
    );
}
#[test]
fn mutable_and_static_trait_methods_use_existing_borrow_and_return_paths() {
    native(
        r#"
trait Change{fn set(self:&mut Self,value:i32);fn get(self:&Self)->i32;fn make(value:i32)->Self;}
struct Cell{value:i32}
impl Change for Cell{
 fn set(value:i32){.value=value}
 fn get()->i32{return .value}
 fn make(value:i32)->Self{return Cell{value:value}}
}
fn main()->i32{var c=Cell.make(2);c.set(42);return c.get()-42}
"#,
    );
}
#[test]
fn value_receiver_and_ordinary_closure_composition_are_native() {
    native(
        r#"
trait Step{fn apply(self:Self,x:i32)->i32;}
struct Increment{delta:i32}
impl Step for Increment{fn apply(x:i32)->i32{return .delta+x}}
fn main()->i32{let op=Increment{delta:2};let f=(x:i32)->i32=>Increment.apply(op,x);return f(40)-42}
"#,
    );
}
#[test]
fn trait_signature_cannot_silently_change_modes_types_or_results() {
    for body in [
        "fn read(self:&mut Self)->i32{return .x}",
        "fn read(self:Self)->i32{return .x}",
        "fn read()->i64{return 1}",
        "fn read(x:i32)->i32{return x}",
    ] {
        let src = format!(
            "trait Read{{fn read(self:&Self)->i32;}} struct C{{x:i32}} impl Read for C{{{body}}} fn main(){{}}"
        );
        let r = compile(&src);
        assert_eq!(r.status, ResultCategory::SafeReject, "{}", r.to_json());
        assert!(
            r.diagnostics.iter().any(|d| d.code == "E-TRAIT-SIGNATURE"),
            "{}",
            r.to_json()
        );
    }
}
#[test]
fn missing_extra_duplicate_methods_and_overlapping_impls_reject() {
    for (body, code) in [
        ("impl Read for C{}", "E-TRAIT-METHOD-SET"),
        (
            "impl Read for C{fn read()->i32=>.x;fn extra(){}}",
            "E-TRAIT-METHOD-SET",
        ),
        (
            "impl Read for C{fn read()->i32=>.x;fn read()->i32=>.x}",
            "E-TRAIT-METHOD-SET",
        ),
        (
            "impl Read for C{fn read()->i32=>.x}impl Read for C{fn read()->i32=>.x}",
            "E-TRAIT-DUPLICATE",
        ),
    ] {
        let r = compile(&format!(
            "trait Read{{fn read(self:&Self)->i32;}}struct C{{x:i32}}{body}fn main(){{}}"
        ));
        assert_eq!(r.status, ResultCategory::SafeReject, "{}", r.to_json());
        assert!(
            r.diagnostics.iter().any(|d| d.code == code),
            "{}",
            r.to_json()
        );
    }
}
#[test]
fn unsupported_solver_features_are_not_invented_by_lowering_to_functions() {
    for src in [
        "trait Read<T>{fn read(self:&Self)->T;}fn main(){}",
        "trait Read{fn read(self:&Self)->i32{return 1}}fn main(){}",
        "trait A{fn f(self:&Self)->i32;}trait B{fn f(self:&Self)->i32;}struct C{x:i32}impl A for C{fn f()->i32=>1}impl B for C{fn f()->i32=>2}fn main(){}",
    ] {
        let r = compile(src);
        assert_eq!(r.status, ResultCategory::Unsupported, "{}", r.to_json());
    }
    // 171 implements this exact original resource shared-head declaration.
    let r = compile(
        "trait Read{fn read(self:&Self)->i32;}resource C{x:i32}impl Read for C{fn read()->i32=>.x}fn main(){}",
    );
    assert_eq!(r.status, ResultCategory::Success, "{}", r.to_json());
    assert_eq!(r.verification_runs, 1);
}
#[test]
fn ordinary_ownership_checks_still_reject_trait_method_escape() {
    let r = compile(
        "trait Leak{fn leak(self:&Self)->&i32;}struct C{x:i32}impl Leak for C{fn leak()->&i32{let x=7;return &x}}fn main(){}",
    );
    assert_eq!(r.status, ResultCategory::SafeReject, "{}", r.to_json());
    assert!(
        r.diagnostics
            .iter()
            .any(|d| d.code.starts_with("E-BORROW") || d.code.starts_with("E-ADDRESS")),
        "{}",
        r.to_json()
    );
}
#[test]
fn empty_impl_cannot_manufacture_builtin_capabilities() {
    for name in ["Copy", "Share", "Transfer"] {
        let r = compile(&format!(
            "trait {name}{{}}struct C{{x:i32}}impl {name} for C{{}}fn main(){{}}"
        ));
        assert_eq!(r.status, ResultCategory::SafeReject, "{}", r.to_json());
        assert_eq!(r.verification_runs, 0);
        let expected_code = if name == "Copy" {
            // 194 reserves structural Copy before nominal trait processing.
            assert_eq!(r.phase, zil_base::Phase::Expand, "{}", r.to_json());
            assert!(
                r.diagnostics
                    .iter()
                    .any(|d| d.message == "Copy is a reserved structural capability name")
            );
            "E-GENERIC-ARGUMENT"
        } else {
            "E-TRAIT-RESERVED"
        };
        assert!(
            r.diagnostics.iter().any(|d| d.code == expected_code),
            "{}",
            r.to_json()
        );
    }
}

#[test]
fn headless_layout_traits_write_512_rectangles_through_owned_buffer() {
    native(include_str!(
        "../../../examples/mainline/static_traits/main.zil"
    ));
}

#[test]
fn attributes_and_ambiguous_method_names_are_not_silently_erased() {
    for (i,src) in [
        "trait X{fn f(self:&Self)->i32;}struct C{f:i32}impl X for C{fn f()->i32=>1}fn main(){}",
        "trait X{fn f(self:&Self)->i32;}struct C{x:i32}impl C{fn f(self:&Self)->i32=>1}impl X for C{fn f()->i32=>2}fn main(){}",
        "trait X{#[unknown] fn f(self:&Self)->i32;}struct C{x:i32}impl X for C{fn f()->i32=>2}fn main(){}",
    ].into_iter().enumerate() {
        let r = compile(src);
        assert_eq!(r.status, if i!=1 {ResultCategory::SafeReject}else{ResultCategory::Unsupported}, "{}", r.to_json());
    }
}

#[test]
fn finite_nominal_generic_bounds_are_now_checked_before_instantiation() {
    let r = compile(
        "trait Read{fn read(self:&Self)->i32;}struct C{x:i32}impl Read for C{fn read()->i32=>.x}fn read<T:Read>(x:&T)->i32=>x.read();fn main(){}",
    );
    assert_eq!(r.status, ResultCategory::Success, "{}", r.to_json());
}
