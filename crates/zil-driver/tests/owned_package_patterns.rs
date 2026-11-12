use std::{
    fs,
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};
use zil_base::{FileId, ModuleId, ResultCategory, SourceUnit};
use zil_driver::{CompileOptions, Stage, compile_source};

static NEXT: AtomicU64 = AtomicU64::new(0);

const MIXED: &str = "resource R{n:i32}package P{manages r:R;n:i32}fn result()->i32{var p=P{r:R{n:5},n:7};let P{r:moved,n:ref mut n}=p;*n=9;return moved.n+*n}fn main()->i32{return result()-14}";

#[test]
fn owned_package_named_pattern_moves_and_borrows_distinct_fields() {
    let source = SourceUnit::new(FileId(0), ModuleId(0), "owned_package_pattern.zil", MIXED);
    let report = compile_source(&source, &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
}

fn native(source: &str, expected: i32) {
    native_stdout(source, &format!("{expected}\n"));
}

fn native_stdout(source: &str, expected: &str) {
    let dir = std::env::temp_dir().join(format!(
        "zil-owned-package-patterns-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&dir).unwrap();
    let unit = SourceUnit::new(FileId(0), ModuleId(0), "owned_package_patterns.zil", source);
    let mut backends = vec![zil_driver::NativeBackend::Clang];
    if cfg!(feature = "llvm-api") {
        backends.push(zil_driver::NativeBackend::LlvmApi);
    }
    for backend in backends {
        for optimization in [zil_driver::Optimization::O0, zil_driver::Optimization::O2] {
            let report = compile_source(
                &unit,
                &CompileOptions {
                    stop_after: Stage::Build,
                    entry: Some("result".into()),
                    backend,
                    optimization,
                    output: Some(dir.join("program")),
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
            let output = Command::new(&report.artifacts["executable"])
                .output()
                .unwrap();
            assert!(output.status.success(), "{output:?}");
            assert_eq!(output.stdout, expected.as_bytes());
            assert!(output.stderr.is_empty());
        }
    }
    fs::remove_dir_all(&dir).unwrap();
}

fn reject(source: &str, code: &str) {
    let unit = SourceUnit::new(
        FileId(0),
        ModuleId(0),
        "owned_package_patterns_negative.zil",
        source,
    );
    let report = compile_source(&unit, &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
    assert_eq!(report.diagnostics[0].code, code, "{}", report.to_json());
}

#[test]
fn named_owned_package_patterns_run_in_bindings_and_conditions() {
    native(
        "resource R{n:i32}package P{manages r:R;n:i32}fn result()->i32{var p=P{r:R{n:5},n:7};let P{n:ref mut n,r:moved}=p;*n=9;return moved.n+*n}",
        14,
    );
    native(
        "resource R{n:i32}package P{manages r:R;n:i32}fn result()->i32{let p=P{r:R{n:5},n:7};match p{P{n:k,r:moved}=>{return moved.n+k}}}",
        12,
    );
    native(
        "resource R{n:i32}package P{manages r:R;n:i32}fn result()->i32{let p=P{r:R{n:5},n:7};if let P{n:k,r:moved}=p{return moved.n+k}}",
        12,
    );
    native(
        "resource R{n:i32}package P{manages r:R;n:i32}fn make(n:i32)->P{return P{r:R{n:n},n:n+1}}fn result()->i32{var i=0;var total=0;while let P{r:moved,n:k}=make(i){total+=moved.n+k;i+=1;if i==3{break}}return total}",
        9,
    );
}

#[test]
fn error_union_owned_package_payload_borrows_its_package_child() {
    native(
        "resource R{n:i32}package Child{manages r:R;n:i32}package Failure{child:Child;code:i32}fn fail()->Failure!void{return Failure{child:Child{r:R{n:5},n:7},code:1}}fn inspect(child:&Child)->i32{return child.n}fn result()->i32{match fail(){.err(failed)=>{return inspect(&failed.child)},.ok(_)=>{return 0}}}",
        7,
    );
}

#[test]
fn named_pattern_mutably_borrows_owned_package_child_and_moves_sibling() {
    native(
        "resource R{n:i32}package Child{manages r:R;n:i32}package Parent{child:Child;manages side:R}fn result()->i32{let p=Parent{child:Child{r:R{n:5},n:7},side:R{n:2}};let Parent{child:Child{r:moved,n:k},side:side}=p;return moved.n+side.n+k}",
        14,
    );
    native(
        "resource R{n:i32}package Child{manages r:R;n:i32}package Parent{child:Child;manages side:R}fn result()->i32{let p=Parent{child:Child{r:R{n:5},n:7},side:R{n:2}};match p{Parent{child:Child{r:moved,n:k},side:side}=>{return moved.n+side.n+k}}}",
        14,
    );
    native(
        "resource R{n:i32}package Child{manages r:R;n:i32}package Parent{child:Child;manages side:R}fn result()->i32{let p=Parent{child:Child{r:R{n:5},n:7},side:R{n:2}};if let Parent{child:Child{r:moved,n:k},side:side}=p{return moved.n+side.n+k}}",
        14,
    );
    native(
        "resource R{n:i32}package Child{manages r:R;n:i32}package Parent{child:Child;manages side:R}fn result()->i32{var p=Parent{child:Child{r:R{n:5},n:7},side:R{n:2}};let Parent{child:Child{r:moved,n:ref mut k},side:side}=p;*k=9;return moved.n+side.n+*k}",
        16,
    );
    native(
        "resource R{n:i32}package Child{manages r:R;n:i32}package Parent{child:Child;manages side:R}fn make(n:i32)->Parent{return Parent{child:Child{r:R{n:n},n:n+1},side:R{n:2}}}fn result()->i32{var i=0;var total=0;while let Parent{child:Child{r:moved,n:k},side:side}=make(i){total+=moved.n+k+side.n;i+=1;if i==3{break}}return total}",
        15,
    );
    native(
        "resource R{n:i32}package Child{manages r:R;n:i32}package Parent{child:Child;manages side:R}fn result()->i32{let p=Parent{child:Child{r:R{n:5},n:7},side:R{n:2}};match p{Parent{child:Child{r:_,n:0},side:_}=>{return 0},Parent{child:Child{r:moved,n:k},side:side}=>{return moved.n+side.n+k}}}",
        14,
    );
    native(
        "resource R{n:i32}package Child{manages r:R;n:i32}package Parent{child:Child;manages side:R}fn bump(child:&mut Child)->i32{child.n+=1;return child.n}fn result()->i32{var p=Parent{child:Child{r:R{n:5},n:7},side:R{n:2}};let Parent{side:moved,child:ref mut child}=p;return bump(child)+moved.n}",
        10,
    );
    native(
        "resource R{n:i32}package Child{manages r:R;n:i32}package Parent{child:Child;manages side:R}fn result()->i32{let p=Parent{child:Child{r:R{n:5},n:7},side:R{n:2}};let Parent{child:child,side:side}=p;let Child{r:moved,n:k}=child;return moved.n+side.n+k}",
        14,
    );
}

#[test]
fn named_owned_package_pattern_drops_each_field_once() {
    native_stdout(
        "extern \"C\" fn putchar(n:i32)->i32;resource R{n:i32}impl Drop for R{fn drop(self:Self){unsafe{putchar(.n)}}}package P{manages a:R;manages b:R}fn result()->i32{let p=P{a:R{n:65},b:R{n:66}};let P{a:moved,b:_}=p;return moved.n}",
        "AB65\n",
    );
    native_stdout(
        "extern \"C\" fn putchar(n:i32)->i32;resource R{n:i32}impl Drop for R{fn drop(self:Self){unsafe{putchar(.n)}}}package Child{manages r:R;n:i32}package Parent{child:Child;manages side:R}fn result()->i32{let p=Parent{child:Child{r:R{n:65},n:7},side:R{n:66}};let Parent{child:Child{r:moved,n:_},side:_}=p;return moved.n}",
        "AB65\n",
    );
}

#[test]
fn named_owned_package_pattern_returns_field_and_cleans_error_exit() {
    native_stdout(
        "extern \"C\" fn putchar(n:i32)->i32;resource R{n:i32}impl Drop for R{fn drop(self:Self){unsafe{putchar(.n)}}}package P{manages a:R;manages b:R}fn peel(p:P)->P{let P{a:moved,b:_}=p;return P{a:give moved,b:R{n:67}}}fn result()->i32{let returned=peel(P{a:R{n:65},b:R{n:66}});return returned.a.n}",
        "BAC65\n",
    );
    native_stdout(
        "extern \"C\" fn putchar(n:i32)->i32;resource R{n:i32}impl Drop for R{fn drop(self:Self){unsafe{putchar(.n)}}}package P{manages a:R;manages b:R}fn fail()->void!i32{return ()}fn run()->void!i32{let p=P{a:R{n:65},b:R{n:66}};let P{a:moved,b:_}=p;let n=fail()?;return n}fn result()->i32{match run(){.err(_)=>{return 1},.ok(n)=>{return n}}}",
        "AB1\n",
    );
    native_stdout(
        "extern \"C\" fn putchar(n:i32)->i32;resource R{n:i32}impl Drop for R{fn drop(self:Self){unsafe{putchar(.n)}}}package Child{manages r:R;n:i32}package Parent{child:Child;manages side:R}fn fail()->void!i32{return ()}fn run()->void!i32{let p=Parent{child:Child{r:R{n:65},n:7},side:R{n:66}};let Parent{child:Child{r:moved,n:_},side:_}=p;let n=fail()?;return n}fn result()->i32{match run(){.err(_)=>{return 1},.ok(n)=>{return n}}}",
        "AB1\n",
    );
}

#[test]
fn named_owned_package_patterns_reject_invalid_owner_and_partial_parent_use() {
    const HEAD: &str = "resource R{n:i32}package P{manages r:R;n:i32}package Q{manages r:R;n:i32}";
    reject(
        &format!(
            "{HEAD}fn result()->i32{{let p=P{{r:R{{n:5}},n:7}};let Q{{r:m,n:k}}=p;return m.n+k}}"
        ),
        "E-MATCH-TYPE",
    );
    reject(
        &format!(
            "{HEAD}fn result()->i32{{let p=P{{r:R{{n:5}},n:7}};let P{{r:m,r:again}}=p;return m.n}}"
        ),
        "E-MATCH-FIELD",
    );
    reject(
        &format!(
            "{HEAD}fn result()->i32{{let p=P{{r:R{{n:5}},n:7}};let P{{r:m,n:_}}=p;let whole=p;return whole.n}}"
        ),
        "E-ANALYZE-IR",
    );
    reject(
        "resource R{n:i32}package Child{manages r:R;n:i32}package Parent{child:Child;manages side:R}fn result()->i32{let p=Parent{child:Child{r:R{n:5},n:7},side:R{n:2}};let Parent{child:Child{r:m,n:_},side:_}=p;let whole=p;return whole.child.n}",
        "E-ANALYZE-IR",
    );
    reject(
        "resource R{n:i32}package Child{manages r:R;n:i32}package Parent{child:Child;manages side:R}fn result()->i32{let p=Parent{child:Child{r:R{n:5},n:7},side:R{n:2}};let Parent{child:Child{r:m,r:again},side:_}=p;return m.n}",
        "E-MATCH-FIELD",
    );
}

#[test]
fn borrowed_package_child_blocks_stale_move_and_whole_parent_take() {
    const HEAD: &str = "resource R{n:i32}package Child{manages r:R;n:i32}package Parent{child:Child;code:i32}fn inspect(child:&Child)->i32{return child.n}";
    reject(
        &format!(
            "{HEAD}fn result()->i32{{var p=Parent{{child:Child{{r:R{{n:5}},n:7}},code:1}};let view=&p.child;let moved=p.child;return inspect(view)}}"
        ),
        "E-BORROW-CONFLICT",
    );
    reject(
        &format!(
            "{HEAD}fn result()->i32{{var p=Parent{{child:Child{{r:R{{n:5}},n:7}},code:1}};let view=&p.child;let whole=p;return inspect(view)+whole.code}}"
        ),
        "E-BORROW-CONFLICT",
    );
}

#[test]
fn nested_consuming_member_pattern_preserves_the_real_parent() {
    native_stdout(
        "extern \"C\" fn putchar(n:i32)->i32;resource R{n:i32}impl Drop for R{fn drop(self:Self){unsafe{putchar(.n)}}}package Child{manages a:R;manages b:R;n:i32}package Middle{child:Child;manages side:R}package Parent{middle:Middle;manages tail:R}fn result()->i32{var p=Parent{middle:Middle{child:Child{a:R{n:65},b:R{n:66},n:7},side:R{n:67}},tail:R{n:68}};let Child{a:moved,b:_,n:n}=p.middle.child;return moved.n+n}",
        "ABCD72\n",
    );
}

#[test]
fn recursive_mixed_patterns_keep_borrows_in_the_original_parent() {
    native_stdout(
        "extern \"C\" fn putchar(n:i32)->i32;resource R{n:i32}impl Drop for R{fn drop(self:Self){unsafe{putchar(.n)}}}package Child{manages a:R;manages b:R;n:i32}package Parent{child:Child;manages side:R}fn result()->i32{var p=Parent{child:Child{a:R{n:65},b:R{n:66},n:7},side:R{n:67}};let Parent{child:Child{a:moved,b:ref b,n:ref mut n},side:_}=p;*n=9;return moved.n+b.n+*n}",
        "ABC140\n",
    );
}

#[test]
fn nested_patterns_preserve_remaining_cleanup_on_error_exit() {
    native_stdout(
        "extern \"C\" fn putchar(n:i32)->i32;resource R{n:i32}impl Drop for R{fn drop(self:Self){unsafe{putchar(.n)}}}package Child{manages a:R;manages b:R;n:i32}package Parent{child:Child;manages side:R}fn fail()->void!i32{return ()}fn work()->void!i32{var p=Parent{child:Child{a:R{n:65},b:R{n:66},n:7},side:R{n:67}};let Parent{child:Child{a:moved,b:ref b,n:ref mut n},side:_}=p;*n=b.n;let ignored=fail()?;return ignored}fn result()->i32{match work(){.err(_)=>{return 1},.ok(n)=>{return n}}}",
        "ABC1\n",
    );
}

#[test]
fn refilling_a_partially_moved_child_cleans_only_its_remaining_leaves() {
    native_stdout(
        "extern \"C\" fn putchar(n:i32)->i32;resource R{n:i32}impl Drop for R{fn drop(self:Self){unsafe{putchar(.n)}}}package Child{manages a:R;manages b:R;n:i32}package Parent{child:Child;manages side:R}fn result()->i32{var p=Parent{child:Child{a:R{n:65},b:R{n:66},n:7},side:R{n:67}};let Parent{child:Child{a:moved,b:ref b,n:ref mut n},side:_}=p;*n=b.n;p.child=Child{a:R{n:68},b:R{n:69},n:9};return moved.n+p.child.n}",
        "BDEAC74\n",
    );
}

#[test]
fn direct_child_borrow_and_selective_sibling_move_use_the_same_actual_intervals() {
    native(
        "resource R{n:i32}package Child{manages r:R;n:i32}package Parent{child:Child;manages side:R}fn result()->i32{var p=Parent{child:Child{r:R{n:5},n:7},side:R{n:2}};let view=&p.child;let Parent{child:ref retained,side:moved}=p;return view.n+retained.n+moved.n}",
        16,
    );
}

#[test]
fn observing_nested_copy_fields_keeps_the_complete_parent_for_retry() {
    native_stdout(
        "extern \"C\" fn putchar(n:i32)->i32;resource R{n:i32}impl Drop for R{fn drop(self:Self){unsafe{putchar(.n)}}}package Child{manages r:R;n:i32}package Parent{left:Child;right:Child}fn identity(p:Parent)->Parent{return p}fn result()->i32{let p=Parent{left:Child{r:R{n:65},n:20},right:Child{r:R{n:66},n:22}};let observed=p.left.n+p.right.n;let whole=identity(p);return observed+whole.left.n+whole.right.n}",
        "AB84\n",
    );
}

#[test]
fn observing_nested_managed_fields_keeps_the_complete_parent_for_retry() {
    let source = r#"
package Value{manages bytes:ByteBuffer;id:i32}
package Entry{value:Value;key:i32}
fn relay(entry:Entry)->Entry{return entry}
fn result()->i32{
 let bytes=bytes_new()
 if !bytes_reserve(bytes,1)||!bytes_push(bytes,66u8){return 1}
 var original=Entry{value:Value{bytes:give bytes,id:7},key:9}
 let description=original.value.bytes
 let old=i32(bytes_view(description)[0])
 let retried=relay(original)
 return old+retried.value.id+retried.key-16
}
fn main()->i32{return result()-66}
"#;
    native(source, 66);
}
