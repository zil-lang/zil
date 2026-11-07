//! Independent source-level neighbors for the exact 090 candidate.
use zil_base::{FileId, ModuleId, Phase, ResultCategory, SourceUnit};
use zil_driver::{CompileOptions, compile_source};

fn check(name: &str, text: &str) -> zil_driver::CompileReport {
    let base = std::env::var_os("ZIL_TEST_REPORT_DIR").map(std::path::PathBuf::from);
    let directory = base.map(|p| p.join(name));
    if let Some(path) = &directory {
        std::fs::create_dir_all(path).unwrap();
        std::fs::write(path.join("source.zil"), text).unwrap();
    }
    let report = compile_source(
        &SourceUnit::new(FileId(0), ModuleId(0), name, text),
        &CompileOptions {
            emit_dir: directory.clone(),
            ..CompileOptions::default()
        },
    );
    if let Some(path) = directory {
        std::fs::write(path.join("report.json"), report.to_json()).unwrap();
    }
    report
}

#[test]
fn review_generic_forwarding_keeps_slice_element_identity() {
    let text = "fn relay<T>(p:&[T])->&[T]{return p;}fn put(p:&mut[i32],v:i32){p[0]=v;}type Bytes=[u8];fn count(p:&Bytes)->i64{return p.len();}fn result()->i64{var a=[1,2,3];put(&mut a[..],7);let b=[2u8,4,6];let x=relay::<i32>(&a[1..]);let y=relay::<u8>(&b[1..]);return i64(x[0])+i64(y[0])+count(y);}";
    let report = check("generic-forwarding", text);
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.phase, Phase::Verify);
}

#[test]
fn review_parametric_writes_and_range_bodies_are_checked_symbolically() {
    for (name, text) in [
        (
            "generic-write-slice",
            "fn put<T>(p:&mut[T],v:T){p[0]=v;}fn result()->i32{var a=[1,2];put(&mut a[..],3);return a[0];}",
        ),
        (
            "generic-write-no-slice",
            "fn put<T>(p:&mut T,v:T){*p=v;}fn result()->i32{var a=1;put(&mut a,3);return a;}",
        ),
        (
            "generic-range-body",
            "fn tail<T>(p:&[T])->&[T]{return &p[1..];}fn result()->i32{let a=[1,2];return tail(&a[..])[0];}",
        ),
    ] {
        let report = check(name, text);
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert_eq!(report.phase, Phase::Verify);
        assert!(!report.artifacts.contains_key("llvm"));
    }
    // Closest generic neighbours remain rejected even without instantiation.
    // A successful i32 instance is never used as evidence that arbitrary T is
    // Copy, numeric, or writable through a shared parameter.
    for (name, text, code) in [
        (
            "generic-shared-slice-write",
            "fn bad<T>(p:&[T],v:T){p[0]=v;}fn result(){}",
            "E-GENERIC-ARGUMENT",
        ),
        (
            "generic-shared-value-write",
            "fn bad<T>(p:&T,v:T){*p=v;}fn result(){}",
            "E-GENERIC-ARGUMENT",
        ),
        (
            "generic-range-bool",
            "fn bad<T>(p:&[T])->&[T]{return &p[true..];}fn result(){}",
            "E-GENERIC-ARGUMENT",
        ),
        (
            "generic-copy-not-inferred",
            "fn bad<T>(p:&mut[T],v:T){p[0]=v;p[1]=v;}fn result(){}",
            "E-GENERIC-ARGUMENT",
        ),
    ] {
        let report = check(name, text);
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{}",
            report.to_json()
        );
        assert_eq!(report.phase, Phase::Expand);
        assert_eq!(report.diagnostics[0].code, code);
        assert!(!report.artifacts.contains_key("llvm"));
    }
}

#[test]
fn review_nested_aliases_cannot_hide_a_reference_or_unsized_slice_element() {
    for (name, text) in [
        (
            "alias-reference",
            "type Ref=&i32;type Pair=(i32,Ref);type Rows=[Pair];fn result(){}",
        ),
        (
            "alias-unsized",
            "type Row=[i32];type Pair=(i32,Row);type Rows=[Pair];fn result(){}",
        ),
    ] {
        let report = check(name, text);
        assert_eq!(
            report.status,
            ResultCategory::Unsupported,
            "{}",
            report.to_json()
        );
        assert_eq!(report.phase, Phase::Expand);
        assert_eq!(report.diagnostics[0].code, "E-UNSUPPORTED");
    }
}
