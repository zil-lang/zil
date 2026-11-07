//! Source synchronous errors compared with an independent Rust lifecycle oracle.
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};
use zil_base::{FileId, ModuleId, Phase, ResultCategory, SourceUnit};
use zil_driver::{
    CompileOptions, NativeBackend, Optimization, Stage, compile_path, compile_source,
};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "zil-error-cleanup-{}-{}",
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
fn build(command: &mut Command) {
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "{command:?}\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
fn run(path: &Path, input: &[u8], expected_code: i32, expected: &str) {
    let mut child = Command::new(path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(input).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while child.try_wait().unwrap().is_none() {
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("error application timed out: {}", path.display());
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    let output = child.wait_with_output().unwrap();
    assert_eq!(
        output.status.code(),
        Some(expected_code),
        "{}: {output:?}",
        path.display()
    );
    assert_eq!(output.stdout, expected.as_bytes(), "{}", path.display());
    assert!(output.stderr.is_empty(), "{}: {output:?}", path.display());
}

#[test]
fn synchronous_errors_and_cleanup_match_rust_at_o0_and_o2() {
    let temp = Temp::new();
    let app = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/mainline/error_cleanup");
    let report = compile_path(
        &app.join("main.zil"),
        &CompileOptions {
            stop_after: Stage::Build,
            output: Some(temp.0.join("zil-O0")),
            ..CompileOptions::default()
        },
    );
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert!(report.verification_runs > 0);
    assert_eq!(report.source_files.len(), 4);
    let llvm = fs::read_to_string(&report.artifacts["llvm"]).unwrap();
    assert!(llvm.contains("@zil.closure.drop."));
    assert!(llvm.contains("%closure.once."));
    assert!(!llvm.contains("@malloc("));
    assert!(!llvm.contains(" noalias "));
    let mut binaries = vec![report.artifacts["executable"].clone()];
    let optimized = temp.0.join("zil-O2");
    build(
        Command::new("clang")
            .args(["-x", "ir", "-O2"])
            .arg(&report.artifacts["llvm"])
            .arg("-o")
            .arg(&optimized),
    );
    binaries.push(optimized);
    for level in ["0", "2"] {
        let binary = temp.0.join(format!("rust-O{level}"));
        build(
            Command::new("rustc")
                .arg(app.join("main.rs"))
                .args([
                    "--edition=2024",
                    "-C",
                    &format!("opt-level={level}"),
                    "-C",
                    "overflow-checks=yes",
                    "-D",
                    "warnings",
                    "-o",
                ])
                .arg(&binary),
        );
        binaries.push(binary);
    }
    for binary in &binaries {
        for (input, code, output) in [
            (b"0", 1, "A"),
            (b"1", 1, "BA"),
            (b"2", 1, "CBA"),
            (b"3", 7, "CBA"),
            (b"A", 1, "vBA"),
            (b"a", 0, "vXBA"),
            (b"F", 1, "vBA"),
            (b"f", 0, "vXBA"),
            (b"P", 1, "vA"),
            (b"p", 0, "vBA"),
            (b"C", 1, "BA"),
            (b"c", 0, "XBA"),
            (b"L", 1, "BCA"),
            (b"l", 0, "BCDA"),
            (b"?", 1, ""),
        ] {
            run(binary, input, code, output);
        }
    }
}

#[test]
fn source_error_boundaries_are_rejected_without_fabricated_success() {
    for (name, source, code) in [
        (
            "plain-question",
            "fn bad()->bool!i32{return 1?}",
            "E-TRY-NOT-ERROR",
        ),
        (
            "outside-error",
            "fn f()->bool!i32{return true} fn bad()->i32{return f()?}",
            "E-TRY-RETURN-TYPE",
        ),
        (
            "different-error",
            "fn f()->bool!i32{return true} fn bad()->i64!i32{return f()?}",
            "E-TRY-ERROR-TYPE",
        ),
        (
            "ambiguous-injection",
            "fn bad()->i32!i32{return 1}",
            "E-ERROR-INJECTION-AMBIGUOUS",
        ),
        (
            "unrelated-payload",
            "fn bad()->bool!i32{let value:i64=3;return value}",
            "E-ERROR-INJECTION",
        ),
        (
            "error-is-not-success",
            "fn f()->bool!i32{return true} fn bad()->i32{return f()}",
            "E-TYPE-MISMATCH",
        ),
    ] {
        let report = compile_source(
            &SourceUnit::new(FileId(0), ModuleId(0), name, source),
            &CompileOptions::default(),
        );
        assert_ne!(
            report.status,
            ResultCategory::Success,
            "{name}: {}",
            report.to_json()
        );
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{name}: {}",
            report.to_json()
        );
        assert_eq!(report.diagnostics[0].phase, Phase::Types, "{name}");
        assert_eq!(report.diagnostics[0].code, code, "{name}");
        assert!(!report.artifacts.contains_key("executable"));
    }
}

#[test]
fn explicit_equal_error_payloads_select_both_tags_on_real_machines() {
    let source = r#"
fn success()->i32!i32{return error_ok::<i32,i32>(42)}
fn failure()->i32!i32{return error_err::<i32,i32>(7)}
fn choose<E:Copy,T:Copy>(failure:E,success:T,fail:bool)->E!T{
 if fail{return error_err::<E,T>(failure)}
 return error_ok::<E,T>(success)
}
fn choose_implicit<E:Copy,T:Copy>(failure:E,success:T,fail:bool)->E!T{
 if fail{return failure}
 return success
}
fn local_implicit<E:Copy,T:Copy>(failure:E,success:T,fail:bool)->E!T{
 if fail{let value:E!T=failure;return value}
 let value:E!T=success;return value
}
fn arrow_implicit<E:Copy,T:Copy>(success:T)->E!T => success
trait Read{fn value(self:&Self)->i32;}
struct Cell{n:i32}
impl Read for Cell{fn value()->i32=>.n}
fn trait_selected<E:Copy,T>(x:&T)->E!i32 where T:Read{return x.value()}

fn good()->i32{match success(){.ok(value)=>{return value},.err(_)=>{return 0}}}
fn bad()->i32{match failure(){.ok(_)=>{return 0},.err(value)=>{return value}}}
fn generic_good()->i32{match choose::<i32,i32>(7,42,false){.ok(value)=>{return value},.err(_)=>{return 0}}}
fn generic_bad()->i32{match choose::<i32,i32>(7,42,true){.ok(_)=>{return 0},.err(value)=>{return value}}}
fn implicit_good()->i32{match choose_implicit::<i32,i32>(7,42,false){.ok(value)=>{return value},.err(_)=>{return 0}}}
fn implicit_bad()->i32{match choose_implicit::<i32,i32>(7,42,true){.ok(_)=>{return 0},.err(value)=>{return value}}}
fn local_good()->i32{match local_implicit::<i32,i32>(7,42,false){.ok(value)=>{return value},.err(_)=>{return 0}}}
fn local_bad()->i32{match local_implicit::<i32,i32>(7,42,true){.ok(_)=>{return 0},.err(value)=>{return value}}}
fn arrow_good()->i32{match arrow_implicit::<i32,i32>(42){.ok(value)=>{return value},.err(_)=>{return 0}}}
fn trait_good()->i32{let c=Cell{n:42};match trait_selected::<i32,Cell>(&c){.ok(value)=>{return value},.err(_)=>{return 0}}}
fn main()->i32{return good()+bad()+generic_good()+generic_bad()+implicit_good()+implicit_bad()+local_good()+local_bad()+arrow_good()+trait_good()-280}
"#;
    let temp = Temp::new();
    let input = temp.0.join("explicit-error-construction.zil");
    fs::write(&input, source).unwrap();
    for backend in [NativeBackend::Clang, NativeBackend::LlvmApi] {
        if backend == NativeBackend::LlvmApi && !cfg!(feature = "llvm-api") {
            continue;
        }
        for optimization in [Optimization::O0, Optimization::O2] {
            let report = compile_path(
                &input,
                &CompileOptions {
                    stop_after: Stage::Build,
                    backend,
                    optimization,
                    output: Some(
                        temp.0
                            .join(format!("explicit-{backend:?}-{optimization:?}")),
                    ),
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
            assert!(report.native_codegen.as_ref().unwrap().fallback.is_none());
            run(&report.artifacts["executable"], b"", 0, "");
        }
    }
}

#[test]
fn error_constructor_names_cannot_be_rebound_or_choose_the_wrong_payload() {
    for (name, source) in [
        (
            "declaration",
            "fn error_ok(n:i32)->i32{return n} fn main()->i32{return 0}",
        ),
        (
            "parameter",
            "fn read(error_err:i32)->i32{return error_err} fn main()->i32{return 0}",
        ),
        ("local", "fn main()->i32{let error_ok=7;return error_ok}"),
        (
            "wrong-payload",
            "fn make()->i32!i32{return error_ok::<i32,i32>(true)} fn main()->i32{return 0}",
        ),
        (
            "wrong-arity",
            "fn make()->i32!i32{return error_err::<i32,i32>(1,2)} fn main()->i32{return 0}",
        ),
        (
            "missing-types",
            "fn make()->i32!i32{return error_ok(1)} fn main()->i32{return 0}",
        ),
    ] {
        let report = compile_source(
            &SourceUnit::new(FileId(0), ModuleId(0), name, source),
            &CompileOptions {
                stop_after: Stage::Verify,
                ..CompileOptions::default()
            },
        );
        assert!(
            matches!(
                report.status,
                ResultCategory::SafeReject | ResultCategory::Unsupported
            ),
            "{name}: {}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 0, "{name}: {}", report.to_json());
    }
}

#[test]
fn equal_owning_error_arms_transfer_each_entry_once() {
    let source = r#"
extern "C" fn putchar(n:i32)->i32;
resource Ticket{code:i32}
impl Drop for Ticket{fn drop(self:Self){unsafe{putchar(.code)}}}
package Item{manages ticket:Ticket}
package Entry<K:StoreElement,V:StoreElement>{key:K;value:V}
fn entry(a:i32,b:i32)->Entry<Item,Item> {return Entry::<Item,Item>{key:Item{ticket:Ticket{code:a}},value:Item{ticket:Ticket{code:b}}}}
fn explicit_ok()->Entry<Item,Item>!Entry<Item,Item>{return error_ok::<Entry<Item,Item>,Entry<Item,Item>>(entry(65,66))}
fn explicit_err()->Entry<Item,Item>!Entry<Item,Item>{return error_err::<Entry<Item,Item>,Entry<Item,Item>>(entry(67,68))}
fn generic<E:StoreElement,T:StoreElement>(failure:E,success:T,fail:bool)->E!T{
 if fail{return failure}
 return success
}

fn observe(value:Entry<Item,Item>!Entry<Item,Item>)->i32{
 match value{.ok(_)=>{return 0},.err(_)=>{return 1}}
}
fn main()->i32{
 let a=observe(explicit_ok());
 let b=observe(explicit_err());
 let c=observe(generic::<Entry<Item,Item>,Entry<Item,Item>>(entry(69,70),entry(71,72),false));
 let d=observe(generic::<Entry<Item,Item>,Entry<Item,Item>>(entry(73,74),entry(75,76),true));
 return a+b+d-2
}
"#;
    let temp = Temp::new();
    let input = temp.0.join("equal-owning-error.zil");
    fs::write(&input, source).unwrap();
    for optimization in [Optimization::O0, Optimization::O2] {
        let report = compile_path(
            &input,
            &CompileOptions {
                stop_after: Stage::Build,
                optimization,
                output: Some(temp.0.join(format!("owning-{optimization:?}"))),
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
        run(&report.artifacts["executable"], b"", 0, "ABCDEFGHKLIJ");
    }
}

#[test]
fn explicit_equal_owning_tag_survives_raw_wire_and_wrong_arm_rejects() {
    use zil_base::{Limits, TargetProfile};
    use zil_ir::{InstructionKind, PlaceProjection};
    let source = r#"
resource R{code:i32}
package P{manages r:R}
fn make()->P!P{return error_ok::<P,P>(P{r:R{code:65}})}
fn observe(value:P!P)->i32{match value{.ok(p)=>{return p.r.code-65},.err(_)=>{return 1}}}
fn main()->i32{return observe(make())}
"#;
    let input = SourceUnit::new(FileId(0), ModuleId(0), "equal-owning-wire", source);
    let syntax = zil_syntax::parse(&input).into_result().unwrap();
    let core = zil_expand::expand(&input, &syntax).unwrap();
    let target = TargetProfile::default();
    let limits = Limits::default();
    let raw = zil_types::check(&core, &target, &limits).unwrap();
    let constructions: Vec<_> = raw
        .functions
        .iter()
        .flat_map(|f| &f.blocks)
        .flat_map(|b| &b.instructions)
        .filter_map(|i| match i.kind {
            InstructionKind::MakeErrorUnion { is_error, .. } => Some(is_error),
            _ => None,
        })
        .collect();
    assert_eq!(constructions, [false]);
    let plan = zil_analyze::analyze(&raw, &limits).unwrap();
    let wire = zil_ir::wire::encode_module_candidate(&raw, &plan, &limits).unwrap();
    assert_eq!(
        zil_verify::verify_module_bytes(&wire, &target, &limits)
            .unwrap()
            .module(),
        &raw
    );

    let mut wrong = raw.clone();
    let extraction = wrong
        .functions
        .iter_mut()
        .flat_map(|f| &mut f.blocks)
        .flat_map(|b| &mut b.instructions)
        .find(|i| {
            matches!(&i.kind, InstructionKind::TakeVariantField { path, .. }
            if path == &[PlaceProjection::ErrorArm(false)])
        })
        .unwrap();
    let InstructionKind::TakeVariantField { path, .. } = &mut extraction.kind else {
        unreachable!()
    };
    path[0] = PlaceProjection::ErrorArm(true);
    assert!(zil_analyze::analyze(&wrong, &limits).is_err());
    let mut claim = plan;
    claim.module_fingerprint = wrong.fingerprint();
    assert!(zil_verify::verify_module(wrong.clone(), claim.clone(), &target, &limits).is_err());
    let forged = zil_ir::wire::encode_module_candidate(&wrong, &claim, &limits).unwrap();
    assert!(zil_verify::verify_module_bytes(&forged, &target, &limits).is_err());
}

#[test]
fn source_nested_copy_errors_and_void_entry_execute_without_unwind() {
    for (name, source, code) in [
        ("unit-ok", "fn main()->bool!void{return ()}", 0),
        ("unit-error", "fn main()->bool!void{return true}", 1),
        (
            "aggregate-result",
            "struct Pair{x:i32;y:bool;} fn f()->bool!Pair{return Pair{x:42,y:true}} fn main()->bool!i32{let p=f()?;return p.x}",
            42,
        ),
        (
            "aggregate-error",
            "struct Fault{x:i32;} fn f()->Fault!i32{return Fault{x:9}} fn main()->Fault!i32{return f()?}",
            1,
        ),
        (
            "function-pointer",
            "fn f()->bool!i32{return 42} fn main()->bool!i32{let g:fn()->bool!i32=f;return g()?}",
            42,
        ),
    ] {
        let temp = Temp::new();
        let report = compile_source(
            &SourceUnit::new(FileId(0), ModuleId(0), name, source),
            &CompileOptions {
                stop_after: Stage::Build,
                output: Some(temp.0.join(name)),
                ..CompileOptions::default()
            },
        );
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{name}: {}",
            report.to_json()
        );
        run(&report.artifacts["executable"], b"", code, "");
    }
}

#[test]
fn runtime_try_does_not_hide_comptime_work_or_execute_runtime_errors_at_ct() {
    let report = compile_source(
        &SourceUnit::new(
            FileId(0),
            ModuleId(0),
            "ct-under-try",
            "fn f(v:i32)->bool!i32{return v} fn main()->bool!i32{return f(#(21+21))?}",
        ),
        &CompileOptions::default(),
    );
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    let report = compile_source(
        &SourceUnit::new(
            FileId(0),
            ModuleId(0),
            "try-at-ct",
            "fn f()->bool!i32{return 42} fn main()->i32{return #(f()?)}",
        ),
        &CompileOptions::default(),
    );
    assert_ne!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert!(!report.artifacts.contains_key("executable"));
}
