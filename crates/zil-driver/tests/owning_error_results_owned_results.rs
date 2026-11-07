//! Owning error results through independent verification and native cleanup.
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
        "owned-results166-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn check(source: &str) -> CompileReport {
    let dir = directory();
    fs::write(dir.join("main.zil"), source).unwrap();
    let report = compile_project(
        &project(&[("main", source)]),
        &CompileOptions {
            emit_dir: Some(dir.join("phases")),
            ..CompileOptions::default()
        },
    );
    fs::write(dir.join("compile.json"), report.to_json()).unwrap();
    report
}

fn native(files: &[(&str, &str)], stdout: &[u8], exit: i32) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
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
        fs::write(
            dir.join("exit.txt"),
            format!("{:?}\n", output.status.code()),
        )
        .unwrap();
        assert_eq!(output.status.code(), Some(exit), "{output:?}");
        assert_eq!(output.stdout, stdout);
        dirs.push(dir);
    }
    dirs
}

const RESOURCE: &str = r#"
extern "C" fn putchar(x:i32)->i32;
resource R{code:i32}
impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
package P{manages r:R}
enum E{Fail}
"#;

#[test]
fn named_success_payload_crosses_two_ordinary_functions_then_cleans_once() {
    let source = format!(
        r#"{RESOURCE}
fn make()->E!P{{let p=P{{r:R{{code:65}}}};return p}}
fn identity(x:E!P)->E!P=>x
fn forward(x:E!P)->E!P=>identity(x)
fn main()->E!i32{{let p=forward(make())?;return p.r.code-65}}
"#
    );
    native(&[("main", &source)], b"A", 0);
}

#[test]
fn both_owned_arms_propagate_and_native_boundary_cleans_only_returned_error() {
    for (flag, output, exit) in [
        ("true", b"ALM".as_slice(), 0),
        ("false", b"LME".as_slice(), 1),
    ] {
        let source = format!(
            r#"{RESOURCE}
package Q{{manages r:R}}
fn make(flag:bool)->Q!P{{
 if flag{{return P{{r:R{{code:65}}}}}};
 return Q{{r:R{{code:69}}}}
}}
fn middle(flag:bool)->Q!i32{{let local=R{{code:76}};let p=make(flag)?;return p.r.code-65}}
fn main()->Q!i32{{let local=R{{code:77}};return middle({flag})?}}
"#
        );
        native(&[("main", &source)], output, exit);
    }
}

#[test]
fn stored_and_discarded_unions_drop_active_payloads_in_lexical_order() {
    let source = format!(
        r#"{RESOURCE}
package Q{{manages r:R}}
fn success()->Q!P=>P{{r:R{{code:65}}}}
fn failure()->Q!P=>Q{{r:R{{code:69}}}}
fn main()->i32{{success();failure();let a=success();let b=failure();return 0}}
"#
    );
    native(&[("main", &source)], b"AEEA", 0);
}

#[test]
fn whole_local_reassignment_preserves_outer_slot_and_cleanup_order() {
    let source = format!(
        r#"{RESOURCE}
fn main()->i32{{var v:E!P=P{{r:R{{code:65}}}};let later=R{{code:90}};v=P{{r:R{{code:66}}}};return 0}}
"#
    );
    // §8.1.5 promotes the unchanged original source and fixed cleanup oracle.
    native(&[("main", &source)], b"AZB", 0);
}

#[test]
fn failing_later_argument_cleans_staged_union_before_earlier_local() {
    for (flag, expected, exit) in [
        ("true", b"CAZ".as_slice(), 0),
        ("false", b"AZ".as_slice(), 1),
    ] {
        let source = format!(
            r#"{RESOURCE}
fn make()->E!P=>P{{r:R{{code:65}}}}
fn next(ok:bool)->E!i32{{if ok{{return 0}};return E.Fail}}
fn consume(x:E!P,n:i32)->i32{{unsafe{{putchar(67)}};return n}}
fn main()->E!i32{{let guard=R{{code:90}};return consume(make(),next({flag})? )}}
"#
        );
        native(&[("main", &source)], expected, exit);
    }
}

#[test]
fn failed_binding_cleans_partial_preparation_then_existing_union_in_order() {
    let source = format!(
        r#"{RESOURCE}
package Pair{{manages first:R;manages second:R}}
fn fail()->E!i32=>E.Fail
fn main()->E!i32{{
 let u:E!Pair=Pair{{first:R{{code:65}},second:R{{code:67}}}};
 let guard=R{{code:90}};
 let prepared:E!Pair=Pair{{first:R{{code:66}},second:R{{code:fail()?}}}};
 return 0
}}
"#
    );
    native(&[("main", &source)], b"BZAC", 1);
}

#[test]
fn continue_and_break_discharge_each_actual_loop_union() {
    let source = format!(
        r#"{RESOURCE}
fn main()->i32{{var i=0;while i<3{{
 let u:E!P=P{{r:R{{code:65+i}}}};i+=1;if i==1{{continue}};break
}};return 0}}
"#
    );
    native(&[("main", &source)], b"AB", 0);
}

#[test]
fn public_owned_result_crosses_module_boundaries() {
    let model = r#"
extern "C" fn putchar(x:i32)->i32;
pub resource R{code:i32}
impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
pub package P{manages r:R}
pub enum E{Fail}
pub fn make()->E!P=>P{r:R{code:65}}
pub fn read(p:P)->i32=>p.r.code
"#;
    native(
        &[
            (
                "main",
                "import model;import adapter;fn main()->model.E!i32{let p=adapter.forward(model.make())?;return model.read(p)-65}",
            ),
            ("model", model),
            (
                "adapter",
                "import model;pub fn forward(x:model.E!model.P)->model.E!model.P=>x",
            ),
        ],
        b"A",
        0,
    );
}

#[test]
fn success_and_returned_error_buffers_are_really_freed_before_later_fields() {
    let source = format!(
        r#"{RESOURCE}
package Data{{manages data:ByteBuffer;manages marker:R}}
fn make()->E!Data{{
 let b=bytes_new();if !bytes_reserve(b,16)||!bytes_push(b,42u8){{return E.Fail}};
 return Data{{data:give b,marker:R{{code:65}}}}
}}
fn main()->E!i32{{let p=make()?;return i32(bytes_view(p.data)[0])-42}}
"#
    );
    let failure_source = format!(
        r#"{RESOURCE}
package Data{{manages data:ByteBuffer;manages marker:R}}
fn make_error()->Data!i32{{
 let b=bytes_new();if !bytes_reserve(b,16)||!bytes_push(b,42u8){{return 91}};
 return Data{{data:give b,marker:R{{code:65}}}}
}}
fn main()->Data!i32{{let local=R{{code:77}};return make_error()?}}
"#
    );
    let probe = r#"
#include <stdio.h>
#include <stdlib.h>
extern void *__real_realloc(void *, size_t);
extern void __real_free(void *);
static void *live;
static unsigned allocations, releases;
void *__wrap_realloc(void *p, size_t n) {
    if (p && p != live) _Exit(91);
    void *q = __real_realloc(p, n);
    if (q) { if (!p) ++allocations; live = q; }
    return q;
}
void __wrap_free(void *p) {
    if (p) {
        if (p != live) _Exit(92);
        live = NULL; ++releases; putchar('F');
    }
    __real_free(p);
}
__attribute__((destructor)) static void balance(void) {
    if (live || allocations != 1 || releases != 1) _Exit(93);
}
"#;
    for (dirs, expected, exit) in [
        (native(&[("main", &source)], b"A", 0), b"FA".as_slice(), 0),
        (
            native(&[("main", &failure_source)], b"MA", 1),
            b"MFA".as_slice(),
            1,
        ),
    ] {
        for (index, dir) in dirs.iter().enumerate() {
            let c = dir.join("allocator.c");
            fs::write(&c, probe).unwrap();
            let executable = dir.join("observed");
            let output =
                Command::new(std::env::var_os("ZIL_CLANG").unwrap_or_else(|| "clang".into()))
                    .arg(if index == 0 { "-O0" } else { "-O2" })
                    .arg(dir.join("program.ll"))
                    .arg(&c)
                    .arg("-Wl,--wrap=realloc")
                    .arg("-Wl,--wrap=free")
                    .arg("-o")
                    .arg(&executable)
                    .output()
                    .unwrap();
            fs::write(dir.join("observer-link.stderr"), &output.stderr).unwrap();
            assert!(output.status.success(), "{output:?}");
            let output = Command::new(executable).output().unwrap();
            fs::write(dir.join("observer.stdout"), &output.stdout).unwrap();
            fs::write(dir.join("observer.stderr"), &output.stderr).unwrap();
            fs::write(
                dir.join("observer.exit.txt"),
                format!("{:?}\n", output.status.code()),
            )
            .unwrap();
            assert_eq!(output.status.code(), Some(exit), "{output:?}");
            assert_eq!(output.stdout, expected);
        }
    }
}

#[test]
fn source_cannot_reuse_partial_or_aliased_payloads_or_infer_resource_mode() {
    use zil_base::Phase;
    let base = "resource R{n:i32}package P{manages r:R}enum E{Fail}";
    for (tail, category, phase, code, message) in [
        (
            "fn main(){let p=P{r:R{n:1}};let u:E!P=p;let a=u;let b=u}",
            ResultCategory::SafeReject,
            Phase::Analyze,
            "E-UNINITIALIZED",
            "not initialized",
        ),
        (
            "fn main(){let p=P{r:R{n:1}};let r=give p.r;let u:E!P=p}",
            ResultCategory::SafeReject,
            Phase::Analyze,
            "E-ANALYZE-IR",
            "every owned element or field live",
        ),
        (
            "fn main()->i32{let p=P{r:R{n:1}};let old=p.r;let u:E!P=p;return old.n}",
            ResultCategory::SafeReject,
            Phase::Analyze,
            "E-ANALYZE-IR",
            "invalidates a description",
        ),
        (
            "fn bad()->E!R=>R{n:1};fn main(){}",
            ResultCategory::Unsupported,
            Phase::Types,
            "E-UNSUPPORTED",
            "finite Copy values or complete owned Packages",
        ),
        (
            "fn main(){let u:E!P=P{r:R{n:1}};let v=give u}",
            ResultCategory::SafeReject,
            Phase::Types,
            "E-GIVE-TYPE",
            "managed resource carrier",
        ),
        (
            "fn bad()->P!P=>P{r:R{n:1}};fn main(){}",
            ResultCategory::SafeReject,
            Phase::Types,
            "E-ERROR-INJECTION-AMBIGUOUS",
            "matches both error and success",
        ),
        (
            "enum F{Fail}fn make()->E!P=>P{r:R{n:1}};fn bad()->F!i32{let p=make()?;return p.r.n}fn main(){}",
            ResultCategory::SafeReject,
            Phase::Types,
            "E-TRY-ERROR-TYPE",
            "exactly the enclosing error type",
        ),
        (
            "fn main(){let u:E!P=P{r:R{n:1}};let f=move()=>{let v=u}}",
            ResultCategory::Unsupported,
            Phase::Types,
            "E-UNSUPPORTED",
            "capture requires a finite Copy value",
        ),
        (
            "fn make()->E!P=>P{r:R{n:1}};fn main(){let u=#(make())}",
            ResultCategory::SafeReject,
            Phase::Ct,
            "E-CT-FORBIDDEN",
            "serializable pure Copy values",
        ),
    ] {
        let report = check(&format!("{base}{tail}"));
        assert_eq!(report.status, category, "{}", report.to_json());
        assert_eq!(report.phase, phase, "{}", report.to_json());
        assert!(
            report
                .diagnostics
                .iter()
                .any(|d| d.code == code && d.message.contains(message)),
            "{}",
            report.to_json()
        );
        assert!(!report.artifacts.contains_key("executable"));
    }
    let report = check(
        "enum E{Fail}package P{manages b:ByteBuffer}fn main()->i32{let p=P{b:bytes_new()};bytes_push(p.b,42u8);let old=bytes_view(p.b);let u:E!P=p;return i32(old[0])}",
    );
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
fn native_entry_keeps_void_or_i32_success_boundary() {
    let source = "resource R{n:i32}package P{manages r:R}enum E{Fail}fn main()->E!P=>P{r:R{n:1}}";
    let dir = directory();
    fs::write(dir.join("main.zil"), source).unwrap();
    let report = compile_project(
        &project(&[("main", source)]),
        &CompileOptions {
            stop_after: Stage::Build,
            output: Some(dir.join("forbidden")),
            emit_dir: Some(dir.join("phases")),
            ..CompileOptions::default()
        },
    );
    fs::write(dir.join("compile.json"), report.to_json()).unwrap();
    assert_eq!(
        report.status,
        ResultCategory::Unsupported,
        "{}",
        report.to_json()
    );
    assert_eq!(report.phase, zil_base::Phase::Codegen);
    assert_eq!(report.verification_runs, 1);
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.code == "E-UNSUPPORTED" && d.message.contains("native main must return")),
        "{}",
        report.to_json()
    );
    assert!(!dir.join("forbidden").exists());
}

fn candidate(source: &str) -> (zil_ir::TypedModule, zil_ir::CandidatePlan) {
    let source = SourceUnit::new(FileId(0), ModuleId(0), "raw166.zil", source);
    let syntax = zil_syntax::parse(&source).into_result().unwrap();
    let core = zil_expand::expand(&source, &syntax).unwrap();
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    let original = zil_types::check(&core, &target, &limits).unwrap();
    let plan = zil_analyze::analyze(&original, &limits).unwrap();
    zil_verify::verify_module(original.clone(), plan.clone(), &target, &limits).unwrap();
    (original, plan)
}

fn reject_candidate(
    label: &str,
    module: zil_ir::TypedModule,
    mut plan: zil_ir::CandidatePlan,
    code: &str,
    message: &str,
) {
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    // Fingerprints and empty candidate Hold rows are untrusted claims. Never
    // call Analyze on a mutation to manufacture a replacement proof.
    plan.module_fingerprint = module.fingerprint();
    let bytes = zil_ir::wire::encode_module_candidate(&module, &plan, &limits).unwrap();
    let dir = directory();
    fs::write(dir.join("label"), label).unwrap();
    fs::write(dir.join("candidate.bin"), &bytes).unwrap();
    for (route, errors) in [
        (
            "raw",
            zil_verify::verify_module(module, plan, &target, &limits).unwrap_err(),
        ),
        (
            "wire",
            zil_verify::verify_module_bytes(&bytes, &target, &limits).unwrap_err(),
        ),
    ] {
        fs::write(
            dir.join(format!("{route}-errors.txt")),
            format!("{errors:#?}\n"),
        )
        .unwrap();
        assert!(
            errors.iter().all(|d| d.phase == zil_base::Phase::Verify),
            "{label} {route}: {errors:?}"
        );
        assert!(
            errors
                .iter()
                .any(|d| d.code == code && d.message.contains(message)),
            "{label} {route}: {errors:?}"
        );
    }
}

fn locate(
    module: &zil_ir::TypedModule,
    predicate: impl Fn(&zil_ir::TypedFunction, &zil_ir::Instruction) -> bool,
) -> (usize, usize, usize) {
    for (f, function) in module.functions.iter().enumerate() {
        for (b, block) in function.blocks.iter().enumerate() {
            for (i, instruction) in block.instructions.iter().enumerate() {
                if predicate(function, instruction) {
                    return (f, b, i);
                }
            }
        }
    }
    panic!("mutation target absent");
}

fn duplicate_result(
    module: &mut zil_ir::TypedModule,
    plan: &mut zil_ir::CandidatePlan,
    position: (usize, usize, usize),
) {
    let (f, b, i) = position;
    let function = &mut module.functions[f];
    let mut duplicate = function.blocks[b].instructions[i].clone();
    let old = duplicate.result.unwrap();
    let value = zil_ir::ValueId(function.values.len() as u32);
    function
        .values
        .push(function.values[old.0 as usize].clone());
    duplicate.result = Some(value);
    function.blocks[b].instructions.insert(i + 1, duplicate);
    let mut hold = plan.functions[f]
        .holds
        .iter()
        .find(|h| h.value == old)
        .unwrap()
        .clone();
    hold.value = value;
    plan.functions[f].holds.push(hold);
}

#[test]
fn raw_and_wire_reconstruct_union_debt_and_exact_active_tag() {
    use zil_ir::{Constant, InstructionKind as I, Terminator, Type};
    let source = "resource R{n:i32}package P{manages r:R}enum E{Fail}fn make()->E!P=>P{r:R{n:1}};fn main()->E!i32{let pending=make();let p=pending?;return p.r.n-1}";
    let (original, plan) = candidate(source);
    let take = locate(&original, |_, i| matches!(i.kind, I::TakeErrorUnion { .. }));
    let local = locate(
        &original,
        |f, i| matches!(i.kind, I::TakeLocal { local } if matches!(f.locals[local.0 as usize].ty, Type::ErrorUnion { .. })),
    );
    let make = locate(
        &original,
        |f, i| matches!(i.kind, I::MakeErrorUnion { value, .. } if matches!(f.values[value.0 as usize], Type::Package { .. })),
    );
    for (label, position) in [("duplicate-take", take), ("duplicate-injection", make)] {
        let mut module = original.clone();
        let mut claims = plan.clone();
        duplicate_result(&mut module, &mut claims, position);
        reject_candidate(
            label,
            module,
            claims,
            "E-VERIFY-RESOURCE",
            "not available exactly once",
        );
    }
    let mut module = original.clone();
    let (f, b, i) = take;
    let I::TakeErrorUnion { value, is_error } = module.functions[f].blocks[b].instructions[i].kind
    else {
        unreachable!()
    };
    module.functions[f].blocks[b].instructions[i].kind = I::ReadErrorUnion { value, is_error };
    reject_candidate(
        "nonconsuming-read",
        module,
        plan.clone(),
        "E-VERIFY-ERROR",
        "consuming TakeErrorUnion",
    );

    let mut module = original.clone();
    let (f, b, i) = local;
    let I::TakeLocal { local } = module.functions[f].blocks[b].instructions[i].kind else {
        unreachable!()
    };
    module.functions[f].blocks[b].instructions[i].kind = I::Load(local);
    reject_candidate(
        "load-copy",
        module,
        plan.clone(),
        "E-VERIFY-ERROR",
        "TakeLocal",
    );

    let tag = locate(&original, |_, i| matches!(i.kind, I::ErrorTag { .. }));
    let mut module = original.clone();
    let (f, b, _) = tag;
    let Terminator::Branch {
        then_block,
        else_block,
        ..
    } = &mut module.functions[f].blocks[b].terminator
    else {
        unreachable!()
    };
    std::mem::swap(then_block, else_block);
    reject_candidate(
        "opposite-tag-edge",
        module,
        plan.clone(),
        "E-VERIFY-ERROR-TAG",
        "exact active tag",
    );

    let mut module = original.clone();
    let (f, b, i) = tag;
    module.functions[f].blocks[b].instructions[i].kind = I::Constant(Constant::Bool(true));
    reject_candidate(
        "unproved-tag",
        module,
        plan.clone(),
        "E-VERIFY-ERROR-TAG",
        "exact active tag",
    );

    let mut module = original;
    let mut claims = plan;
    duplicate_result(&mut module, &mut claims, take);
    let (f, b, i) = take;
    let I::TakeErrorUnion { value, .. } = module.functions[f].blocks[b].instructions[i].kind else {
        unreachable!()
    };
    let result = module.functions[f].blocks[b].instructions[i + 1]
        .result
        .unwrap();
    module.functions[f].values[result.0 as usize] = Type::Bool;
    module.functions[f].blocks[b].instructions[i + 1].kind = I::ErrorTag { value };
    reject_candidate(
        "tag-after-transfer",
        module,
        claims,
        "E-VERIFY-RESOURCE",
        "live owned union",
    );

    let source =
        "resource R{n:i32}package P{manages r:R}enum E{Fail}fn main(){let pending:E!P=P{r:R{n:1}}}";
    let (mut module, plan) = candidate(source);
    let (f, b, i) = locate(&module, |_, i| matches!(i.kind, I::DropErrorUnion { .. }));
    module.functions[f].blocks[b].instructions.remove(i);
    reject_candidate(
        "omitted-cleanup",
        module,
        plan,
        "E-VERIFY-RESOURCE",
        "responsibility",
    );
}

#[test]
fn raw_and_wire_do_not_extend_ordinary_union_admission_to_closure_capture() {
    use zil_ir::Type;
    let source = "resource R{n:i32}package P{manages r:R}enum E{Fail}fn make()->E!P=>P{r:R{n:1}};fn main()->i32{let n=42;let f=move()=>n;return f()-42}";
    let (mut module, plan) = candidate(source);
    let union = module
        .functions
        .iter()
        .find_map(|f| {
            matches!(f.return_type, Type::ErrorUnion { .. }).then(|| f.return_type.clone())
        })
        .unwrap();
    assert_eq!(module.closures.len(), 1);
    assert_eq!(module.closures[0].captures.len(), 1);
    module.closures[0].captures[0] = union;
    reject_candidate(
        "owned-union-capture",
        module,
        plan,
        "E-UNSUPPORTED",
        "owning error-union captures",
    );
}
