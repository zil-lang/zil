//! Finite resource receiver syntax through the production verifier and machine code.
use std::{
    collections::BTreeMap,
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_base::{FileId, Limits, ModuleId, ResultCategory, SourceUnit, TargetProfile};
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
        "resource-methods163-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn check(files: &[(&str, &str)]) -> CompileReport {
    let dir = directory();
    for (name, text) in files {
        fs::write(dir.join(format!("{name}.zil")), text).unwrap();
    }
    let report = compile_project(
        &project(files),
        &CompileOptions {
            emit_dir: Some(dir.join("phases")),
            ..CompileOptions::default()
        },
    );
    fs::write(dir.join("compile.json"), report.to_json()).unwrap();
    report
}

fn native(files: &[(&str, &str)], stdout: &[u8], exit: i32) {
    for optimization in [Optimization::O0, Optimization::O2] {
        let dir = directory();
        for (name, text) in files {
            fs::write(dir.join(format!("{name}.zil")), text).unwrap();
        }
        let executable = dir.join("program");
        let report = compile_project(
            &project(files),
            &CompileOptions {
                stop_after: Stage::Build,
                optimization,
                output: Some(executable.clone()),
                emit_dir: Some(dir.join("phases")),
                ..CompileOptions::default()
            },
        );
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
        assert_eq!(output.status.code(), Some(exit), "{output:?}");
        assert_eq!(output.stdout, stdout);
    }
}

const RESOURCE: &str = r#"
extern "C" fn putchar(x:i32)->i32;
resource R{code:i32}
impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
impl R(self:Self){
    fn read()->i32=>.code
    fn set(n:i32){.code=n}
    fn finish(give self:Self)->i32=>.code
}
"#;

#[test]
fn descriptor_and_explicit_owned_receivers_keep_unique_cleanup() {
    let source = format!(
        "{RESOURCE} fn main()->i32{{let r=R{{code:65}};r.set(66);let d=r;if d.read()!=66{{return 1}};return (give r).finish()-66}}"
    );
    native(&[("main", &source)], b"B", 0);
}

#[test]
fn owned_group_default_can_be_overridden_with_a_descriptor() {
    native(
        &[(
            "main",
            r#"
extern "C" fn putchar(x:i32)->i32;
resource R{code:i32}
impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
impl R(give self:Self){fn finish()->i32=>.code;fn read(self:Self)->i32=>.code}
fn main()->i32{let r=R{code:65};if r.read()!=65{return 1};return R.finish(give r)-65}
"#,
        )],
        b"A",
        0,
    );
}

#[test]
fn byte_buffer_methods_use_existing_descriptor_and_transfer_operations() {
    native(
        &[(
            "main",
            r#"
impl ByteBuffer(self:Self){
    fn reserve(n:i64)->bool=>bytes_reserve(self,n)
    fn append(value:u8)->bool=>bytes_push(self,value)
    fn size()->i64=>bytes_len(self)
    fn handoff(give self:Self)->ByteBuffer contract{result(give)}{return give self}
}
fn main()->i32{let b=bytes_new();if !b.reserve(8)||!b.append(42u8){return 1};let c=(give b).handoff();return i32(c.size()+i64(bytes_view(c)[0]))-43}
"#,
        )],
        b"",
        0,
    );
}

#[test]
fn package_by_value_receiver_consumes_the_whole_package() {
    native(
        &[(
            "main",
            r#"
extern "C" fn putchar(x:i32)->i32;
resource R{code:i32} impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
package P{manages a:R;manages b:R;n:i32}
impl P{fn finish(self:Self)->i32=>.n}
fn main()->i32{let p=P{a:R{code:65},b:R{code:66},n:42};return p.finish()-42}
"#,
        )],
        b"AB",
        0,
    );
}

#[test]
fn stored_closures_call_read_and_write_resource_methods() {
    native(
        &[(
            "main",
            r#"
resource R{n:i32}
impl R(self:Self){fn get()->i32=>.n;fn add(v:i32){.n+=v}}
fn reader()->impl Fn()->i32{let r=R{n:21};return move()=>r.get()}
fn writer()->impl FnMut(i32)->i32{let r=R{n:0};return move(v:i32)=>{r.add(v);return r.get()}}
fn main()->i32{let read=reader();var write=writer();if read()!=21||read()!=21||write(20)!=20||write(22)!=42{return 1};return 0}
"#,
        )],
        b"",
        0,
    );
}

#[test]
fn captured_owner_is_consumed_once_by_an_explicit_give_method() {
    let source = format!(
        "{RESOURCE} fn make()->impl FnOnce()->i32{{let r=R{{code:65}};return move()=> (give r).finish()}} fn main()->i32{{let f=make();return f()-65}}"
    );
    native(&[("main", &source)], b"A", 0);
}

#[test]
fn descriptor_method_calls_inside_effect_wrappers_keep_fnmut() {
    native(
        &[(
            "main",
            r#"
resource R{n:i32}
impl R(self:Self){fn get()->i32=>.n;fn add(v:i32){.n+=v};fn through(v:i32){self.add(v)}}
fn update(r:R,v:i32){r.through(v)}
fn make()->impl FnMut(i32)->i32{let r=R{n:0};return move(v:i32)=>{update(r,v);return r.get()}}
fn main()->i32{var f=make();if f(20)!=20||f(22)!=42{return 1};return 0}
"#,
        )],
        b"",
        0,
    );
}

#[test]
fn public_resource_methods_cross_module_boundaries_and_retained_callbacks() {
    native(
        &[
            (
                "main",
                r#"import model
fn make()->impl FnMut(i32)->i32{let r=model.create(0);return move(n:i32)=>{r.add(n);return r.read()}}
fn main()->i32{var f=make();if f(20)!=20||f(22)!=42{return 1};let r=model.create(42);return (give r).finish()-42}"#,
            ),
            (
                "model",
                r#"pub resource R{n:i32}
impl R(self:Self){pub fn read()->i32=>.n;pub fn add(v:i32){.n+=v};pub fn finish(give self:Self)->i32=>.n;fn secret()->i32=>0}
pub fn create(n:i32)->R contract{result(create)}{return R{n:n}}"#,
            ),
        ],
        b"",
        0,
    );
}

#[test]
fn receiver_is_staged_before_later_argument_failure_and_dropped_once() {
    native(
        &[(
            "main",
            r#"
extern "C" fn putchar(x:i32)->i32;
enum E{Fail}
resource R{code:i32} impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
impl R{fn finish(give self:Self,v:i32)->i32=>.code+v}
fn fail()->E!i32{return E.Fail}
fn run()->E!i32{let r=R{code:65};return (give r).finish(fail()?)}
fn main()->E!i32{return run()?}
"#,
        )],
        b"A",
        1,
    );
}

#[test]
fn ordinary_call_cannot_silently_give_and_old_descriptors_cannot_survive_consumption() {
    for body in [
        "fn main()->i32{let r=R{code:65};return r.finish()}",
        "fn main()->i32{let r=R{code:65};let d=r;return (give d).finish()}",
        "fn main()->i32{let r=R{code:65};let d=r;(give r).finish();return d.read()}",
        "fn main()->i32{let r=R{code:65};(give r).finish();return (give r).finish()}",
    ] {
        let source = format!("{RESOURCE}{body}");
        let report = check(&[("main", &source)]);
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{}",
            report.to_json()
        );
    }
}

#[test]
fn byte_storage_windows_remain_subject_to_mutation_and_transfer_checks() {
    let methods = "impl ByteBuffer(self:Self){fn grow()->bool=>bytes_reserve(self,32);fn consume(give self:Self){}}";
    for body in [
        "fn main()->i32{let b=bytes_new();bytes_reserve(b,8);bytes_push(b,1u8);let view=bytes_view(b);b.grow();return i32(view[0])}",
        "fn main()->i32{let b=bytes_new();bytes_reserve(b,8);bytes_push(b,1u8);let view=bytes_view(b);(give b).consume();return i32(view[0])}",
    ] {
        let source = format!("{methods}{body}");
        let report = check(&[("main", &source)]);
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{}",
            report.to_json()
        );
        assert_eq!(
            report.phase,
            zil_base::Phase::Verify,
            "{}",
            report.to_json()
        );
        assert!(
            report
                .diagnostics
                .iter()
                .any(|d| d.code == "E-BORROW-CONFLICT"),
            "{}",
            report.to_json()
        );
    }
}

#[test]
fn original_resource_address_receiver_probe_remains_explicitly_unsupported() {
    let report = check(&[(
        "main",
        "trait Read{fn read(self:&Self)->i32;}resource C{x:i32}impl Read for C{fn read()->i32=>.x}fn main(){}",
    )]);
    assert_eq!(
        report.status,
        ResultCategory::Unsupported,
        "{}",
        report.to_json()
    );
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.message.contains("addresses to resource or package heads")),
        "{}",
        report.to_json()
    );
}

#[test]
fn byte_buffer_method_writes_are_persistent_in_retained_fnmut() {
    native(
        &[(
            "main",
            r#"
impl ByteBuffer(self:Self){
    fn append(value:u8)->bool=>bytes_push(self,value)
    fn size()->i64=>bytes_len(self)
}
fn make()->impl FnMut(u8)->i64{let b=bytes_new();bytes_reserve(b,8);return move(v:u8)=>{b.append(v);return b.size()}}
fn main()->i32{var f=make();if f(20u8)!=1||f(22u8)!=2{return 1};return 0}
"#,
        )],
        b"",
        0,
    );
}

#[test]
fn transitively_readonly_resource_methods_keep_fn_and_borrowed_writes_remain_local() {
    native(
        &[(
            "main",
            r#"
resource R{n:i32}
impl R(self:Self){fn get()->i32=>.n;fn through()->i32=>self.get();fn add(){.n+=1}}
fn read(r:R)->i32=>r.through()
fn make()->impl Fn()->i32{let r=R{n:42};return move()=>read(r)}
fn main()->i32{let f=make();if f()!=42||f()!=42{return 1};let r=R{n:0};let g=()=>r.add();g();g();return r.get()-2}
"#,
        )],
        b"",
        0,
    );
}

#[test]
fn owned_package_method_inside_move_closure_runs_once_and_cleans_all_fields() {
    native(
        &[(
            "main",
            r#"
extern "C" fn putchar(x:i32)->i32;
resource R{code:i32} impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
package P{manages a:R;manages b:R;n:i32}
impl P{fn finish(self:Self)->i32=>.n}
fn make()->impl FnOnce()->i32{let p=P{a:R{code:65},b:R{code:66},n:42};return move()=>p.finish()}
fn main()->i32{let f=make();return f()-42}
"#,
        )],
        b"AB",
        0,
    );
}

#[test]
fn fresh_receiver_precedes_arguments_and_failed_preparation_cleans_in_reverse_order() {
    native(
        &[(
            "main",
            r#"
extern "C" fn putchar(x:i32)->i32;
resource R{code:i32} impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
impl R{fn finish(give self:Self,v:i32)->i32=>.code+v}
fn make()->R contract{result(create)}{unsafe{putchar(65)};return R{code:66}}
fn arg()->i32{unsafe{putchar(67)};return 0}
fn main()->i32{return make().finish(arg())-66}
"#,
        )],
        b"ACB",
        0,
    );
    native(
        &[(
            "main",
            r#"
extern "C" fn putchar(x:i32)->i32;
enum E{Fail}
resource R{code:i32} impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
impl R{fn finish(give self:Self,give other:R,v:i32)->i32=>.code+other.code+v}
fn fail()->E!i32{return E.Fail}
fn run()->E!i32{let a=R{code:65};let b=R{code:66};return (give a).finish(give b,fail()?)}
fn main()->E!i32{return run()?}
"#,
        )],
        b"BA",
        1,
    );
}

#[test]
fn imported_package_receiver_and_resource_nominal_identity_remain_exact() {
    native(
        &[
            (
                "main",
                "import model;import other;fn main()->i32{let p=model.make_package();let r=other.create();return p.finish()+(give r).finish()-42}",
            ),
            (
                "model",
                r#"pub resource R{n:i32}
pub package P{manages r:R}
impl P{pub fn finish(self:Self)->i32=>.r.n}
pub fn make_package()->P=>P{r:R{n:20}}"#,
            ),
            (
                "other",
                r#"pub resource R{n:i32}
impl R{pub fn finish(give self:Self)->i32=>.n}
pub fn create()->R contract{result(create)}{return R{n:22}}"#,
            ),
        ],
        b"",
        0,
    );
}

#[test]
fn private_owner_private_methods_and_drop_entry_are_not_exported() {
    for (model, expression) in [
        (
            "resource R{n:i32}impl R{pub fn read(self:Self)->i32=>.n}pub fn make()->R contract{result(create)}{return R{n:42}}",
            "r.read()",
        ),
        (
            "pub resource R{n:i32}impl R{fn read(self:Self)->i32=>.n}pub fn make()->R contract{result(create)}{return R{n:42}}",
            "r.read()",
        ),
        (
            "pub resource R{n:i32}impl Drop for R{fn drop(self:Self){}}pub fn make()->R contract{result(create)}{return R{n:42}}",
            "r.drop()",
        ),
        (
            "resource R{n:i32}package P{manages r:R}impl P{pub fn read(self:Self)->i32=>.r.n}pub fn make()->P=>P{r:R{n:42}}",
            "r.read()",
        ),
    ] {
        let main = format!("import model;fn main(){{let r=model.make();{expression}}}");
        let report = check(&[("main", &main), ("model", model)]);
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{}",
            report.to_json()
        );
        assert_ne!(report.phase, zil_base::Phase::Parse, "{}", report.to_json());
    }
}

#[test]
fn callback_contracts_and_consumed_values_cannot_be_reused() {
    for source in [
        "resource R{n:i32}impl R{fn take(give self:Self)->i32=>.n}fn main()->i32{let r=R{n:42};let f=move()=>r.take();return f()}",
        "resource R{n:i32}impl R{fn add(self:Self){.n+=1}}fn make()->impl Fn()->i32{let r=R{n:0};return move()=>{r.add();return r.n}}fn main(){}",
        "resource R{n:i32}impl R{fn take(give self:Self)->i32=>.n}fn main()->i32{let r=R{n:42};let f=move()=>(give r).take();let n=f();return f()+n}",
        "resource R{n:i32}package P{manages r:R}impl P{fn take(self:Self)->i32=>.r.n}fn main()->i32{let p=P{r:R{n:42}};let n=p.take();return p.take()+n}",
        "impl ByteBuffer{fn size(self:Self)->i64=>bytes_len(self)}fn main(){let b=bytes_new();let f=b.size}",
    ] {
        let report = check(&[("main", source)]);
        assert_ne!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert_ne!(report.phase, zil_base::Phase::Parse, "{}", report.to_json());
    }
}

#[test]
fn raw_and_wire_candidates_cannot_relabel_a_mutating_owned_method_callback_as_fn() {
    use zil_ir::{CallableKind, InstructionKind};
    let source = SourceUnit::new(
        FileId(0),
        ModuleId(0),
        "raw163.zil",
        "resource R{n:i32}impl R{fn add(self:Self){.n+=1}}fn main(){let r=R{n:0};var f=move()=>r.add();f()}",
    );
    let syntax = zil_syntax::parse(&source).into_result().unwrap();
    let core = zil_expand::expand(&source, &syntax).unwrap();
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    let original = zil_types::check(&core, &target, &limits).unwrap();
    let plan = zil_analyze::analyze(&original, &limits).unwrap();
    zil_verify::verify_module(original.clone(), plan.clone(), &target, &limits).unwrap();
    let mut forged = original;
    assert_eq!(forged.closures.len(), 1);
    assert_eq!(forged.closures[0].kind, CallableKind::FnMut);
    forged.closures[0].kind = CallableKind::Fn;
    let mut changed = 0;
    for instruction in forged
        .functions
        .iter_mut()
        .flat_map(|f| &mut f.blocks)
        .flat_map(|b| &mut b.instructions)
    {
        if let InstructionKind::CallClosureMut { callee, arguments } = &instruction.kind {
            instruction.kind = InstructionKind::CallClosureShared {
                callee: *callee,
                arguments: arguments.clone(),
            };
            changed += 1;
        }
    }
    assert_eq!(changed, 1);
    let mut plan = plan;
    plan.module_fingerprint = forged.fingerprint();
    let bytes = zil_ir::wire::encode_module_candidate(&forged, &plan, &limits).unwrap();
    let dir = directory();
    fs::write(dir.join("raw163.zil"), &source.text).unwrap();
    fs::write(dir.join("forged-candidate.bin"), &bytes).unwrap();
    for (label, errors) in [
        (
            "raw",
            zil_verify::verify_module(forged, plan, &target, &limits).unwrap_err(),
        ),
        (
            "wire",
            zil_verify::verify_module_bytes(&bytes, &target, &limits).unwrap_err(),
        ),
    ] {
        fs::write(
            dir.join(format!("{label}-errors.txt")),
            format!("{errors:#?}\n"),
        )
        .unwrap();
        assert!(
            errors.iter().all(|d| d.phase == zil_base::Phase::Verify),
            "{errors:?}"
        );
        assert!(
            errors.iter().any(|d| d.code == "E-VERIFY-RESOURCE"),
            "{errors:?}"
        );
    }
}

#[test]
fn consuming_and_refilling_a_package_method_receiver_remains_fnmut() {
    native(
        &[(
            "main",
            r#"
resource R{n:i32} package P{manages r:R}
impl P{fn take(self:Self)->i32=>.r.n}
fn make()->impl FnMut()->i32{var p=P{r:R{n:1}};return move()=>{let n=p.take();p=P{r:R{n:n+1}};return n}}
fn main()->i32{var f=make();if f()!=1||f()!=2||f()!=3{return 1};return 0}
"#,
        )],
        b"",
        0,
    );
}

#[test]
fn package_receiver_error_before_refill_is_once_and_keeps_unique_cleanup() {
    let source = r#"
extern "C" fn putchar(x:i32)->i32;
enum E{Fail}
resource R{code:i32} impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
package P{manages r:R}
impl P{fn take(self:Self)->i32=>.r.code}
fn fail()->E!i32{return E.Fail}
fn make()->impl FnOnce()->E!i32{var p=P{r:R{code:65}};return move()=>{let n=p.take();let v=fail()?;p=P{r:R{code:66}};return n+v}}
fn main()->E!i32{let f=make();return f()?}
"#;
    native(&[("main", source)], b"A", 1);
    let repeatable = source.replace("FnOnce", "FnMut");
    let report = check(&[("main", &repeatable)]);
    assert_eq!(
        report.status,
        ResultCategory::Unsupported,
        "{}",
        report.to_json()
    );
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.message.contains("declared and actual Fn/FnMut/FnOnce")),
        "{}",
        report.to_json()
    );
}

#[test]
fn resource_callable_fields_keep_field_precedence_and_do_not_add_receiver_writes() {
    native(
        &[(
            "main",
            r#"
fn answer()->i32=>42
resource R{call:fn()->i32}
fn read(r:R)->i32=>r.call()
fn make()->impl Fn()->i32{let r=R{call:answer};return move()=>read(r)}
fn main()->i32{let f=make();return f()+f()-84}
"#,
        )],
        b"",
        0,
    );
}

#[test]
fn resource_method_effects_include_arguments_after_the_receiver() {
    native(
        &[(
            "main",
            r#"
resource R{n:i32}
impl R{fn write_to(self:Self,other:R){other.n+=1}}
fn update(a:R,b:R){a.write_to(b)}
fn make()->impl FnMut()->i32{let a=R{n:0};let b=R{n:40};return move()=>{update(a,b);return a.n+b.n}}
fn main()->i32{var f=make();if f()!=41||f()!=42{return 1};return 0}
"#,
        )],
        b"",
        0,
    );
}

#[test]
fn method_effect_dependencies_wake_earlier_callers_in_the_worklist_path() {
    let mut source = String::from("resource R{n:i32}impl R(self:Self){");
    for i in 0..19 {
        source.push_str(&format!("fn m{i}(){{self.m{}()}}", i + 1));
    }
    source.push_str("fn m19(){.n+=1}fn get()->i32=>.n}fn make()->impl FnMut()->i32{let r=R{n:0};return move()=>{r.m0();return r.get()}}fn main()->i32{var f=make();if f()!=1||f()!=2{return 1};return 0}");
    native(&[("main", &source)], b"", 0);
}
