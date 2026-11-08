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
fn tagged_resource_success_preserves_declared_modes_and_actual_creation_sources() {
    use zil_base::{Limits, TargetProfile};
    use zil_ir::PassingMode;
    let source = r#"
extern "C" fn putchar(n:i32)->i32;
resource R{code:i32}
impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
package P{manages r:R}
enum E{Fail}
fn fresh()->E!R contract{result(create)}{return error_ok::<E,R>(R{code:65})}
fn passed(give r:R)->E!R contract{result(give)}{return error_ok::<E,R>(give r)}
fn unavailable()->E!R contract{result(create)}{return error_err::<E,R>(E.Fail)}
fn generic_fresh<T:StoreElement>(value:T)->E!Store<T> contract{result(create)}{
 let s:Store<T>=store_new();if !store_reserve(s,1){return E.Fail};
 store_push(s,value);return give s
}
fn generic_pass<T:StoreElement>(give r:Store<T>)->E!Store<T> contract{result(give)}{return give r}
fn generic_unavailable<T:StoreElement>(give r:Store<T>)->E!Store<T> contract{result(create)}{return E.Fail}
fn main()->E!i32{
 let a=fresh()?;let b=passed(R{code:66})?;
 let c=generic_fresh(P{r:R{code:67}})?;let d=generic_pass(generic_fresh(P{r:R{code:68}})?)?;
 let e=unavailable();let f=generic_unavailable(generic_fresh(P{r:R{code:69}})?);
 return 0
}
"#;
    let input = SourceUnit::new(FileId(0), ModuleId(0), "tagged-resource-modes", source);
    let syntax = zil_syntax::parse(&input).into_result().unwrap();
    let core = zil_expand::expand(&input, &syntax).unwrap();
    let target = TargetProfile::default();
    let limits = Limits::default();
    let raw = zil_types::check(&core, &target, &limits).unwrap();
    let plan = zil_analyze::analyze(&raw, &limits).unwrap();
    zil_verify::verify_module(raw.clone(), plan.clone(), &target, &limits).unwrap();
    let wire = zil_ir::wire::encode_module_candidate(&raw, &plan, &limits).unwrap();
    assert_eq!(
        zil_verify::verify_module_bytes(&wire, &target, &limits)
            .unwrap()
            .module(),
        &raw
    );

    let mut wrong = raw.clone();
    wrong
        .functions
        .iter_mut()
        .find(|f| f.name == "passed")
        .unwrap()
        .result_mode = PassingMode::Create;
    assert!(zil_analyze::analyze(&wrong, &limits).is_err());
    let mut claim = plan;
    claim.module_fingerprint = wrong.fingerprint();
    assert!(zil_verify::verify_module(wrong.clone(), claim.clone(), &target, &limits).is_err());
    let wire = zil_ir::wire::encode_module_candidate(&wrong, &claim, &limits).unwrap();
    assert!(zil_verify::verify_module_bytes(&wire, &target, &limits).is_err());

    let temp = Temp::new();
    let input = temp.0.join("tagged-resource-modes.zil");
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
                    output: Some(temp.0.join(format!("modes-{backend:?}-{optimization:?}"))),
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
            run(&report.artifacts["executable"], b"", 0, "EDCBA");
        }
    }

    for (name, definition) in [
        (
            "old-as-create",
            "fn bad(give r:R)->E!R contract{result(create)}{return error_ok::<E,R>(give r)}",
        ),
        (
            "descriptor-as-owner",
            "fn bad(r:R)->E!R contract{result(give)}{return error_ok::<E,R>(r)}",
        ),
        (
            "descriptor-alias-as-owner",
            "fn bad()->E!R contract{result(create)}{let r=R{code:1};let desc=r;return error_ok::<E,R>(desc)}",
        ),
        (
            "explicit-descriptor-success",
            "fn bad()->E!R contract{result(give)}{let r=R{code:1};return error_ok::<E,Ref<R>>(give r)}",
        ),
        (
            "explicit-descriptor-failure",
            "fn bad()->E!R contract{result(give)}{return error_err::<E,Ref<R>>(E.Fail)}",
        ),
        (
            "explicit-store-descriptor-success",
            "package P{manages r:R} fn bad()->E!Store<P> contract{result(create)}{let s:Store<P>=store_new();return error_ok::<E,Ref<Store<P>>>(give s)}",
        ),
        (
            "explicit-store-descriptor-failure",
            "package P{manages r:R} fn bad()->E!Store<P> contract{result(create)}{return error_err::<E,Ref<Store<P>>>(E.Fail)}",
        ),
        (
            "wrong-resource-identity",
            "resource S{code:i32} fn bad()->E!R contract{result(create)}{return error_ok::<E,S>(S{code:1})}",
        ),
    ] {
        let source = format!("resource R{{code:i32}} enum E{{Fail}} {definition} fn main(){{}}");
        let report = compile_source(
            &SourceUnit::new(FileId(0), ModuleId(0), name, source),
            &CompileOptions::default(),
        );
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{name}: {}",
            report.to_json()
        );
        assert!(!report.artifacts.contains_key("executable"));
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

const LOCAL_OWNING_ERROR_TAGS: &str = r#"
extern "C" fn putchar(n:i32)->i32;
resource R{code:i32}
impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
package P{manages r:R}
fn local_ok()->i32{
 let value:P!P=error_ok::<P,P>(P{r:R{code:65}})
 match value{.ok(p)=>{return p.r.code-65},.err(_)=>{return 1}}
}
fn local_err()->i32{
 let value:P!P=error_err::<P,P>(P{r:R{code:66}})
 match value{.ok(_)=>{return 1},.err(p)=>{return p.r.code-66}}
}
fn reversed_ok()->i32{
 let value:P!P=error_ok::<P,P>(P{r:R{code:67}})
 match value{.err(_)=>{return 1},.ok(p)=>{return p.r.code-67}}
}
fn reversed_err()->i32{
 let value:P!P=error_err::<P,P>(P{r:R{code:68}})
 match value{.err(p)=>{return p.r.code-68},.ok(_)=>{return 1}}
}
fn joined(flag:bool)->i32{
 var value:P!P=error_ok::<P,P>(P{r:R{code:69}})
 if flag{value=error_err::<P,P>(P{r:R{code:70}})}
 match value{.ok(p)=>{return p.r.code-69},.err(p)=>{return p.r.code-70}}
}
fn main()->i32{return local_ok()+local_err()+reversed_ok()+reversed_err()+joined(false)+joined(true)}
"#;

#[test]
fn known_local_owning_error_tags_select_only_the_actual_arm() {
    let temp = Temp::new();
    for backend in [NativeBackend::Clang, NativeBackend::LlvmApi] {
        if backend == NativeBackend::LlvmApi && !cfg!(feature = "llvm-api") {
            continue;
        }
        for optimization in [Optimization::O0, Optimization::O2] {
            let report = compile_source(
                &SourceUnit::new(
                    FileId(0),
                    ModuleId(0),
                    "local-owning-error-tags",
                    LOCAL_OWNING_ERROR_TAGS,
                ),
                &CompileOptions {
                    stop_after: Stage::Build,
                    backend,
                    optimization,
                    output: Some(temp.0.join(format!("local-{backend:?}-{optimization:?}"))),
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
            run(&report.artifacts["executable"], b"", 0, "ABCDEEF");
        }
    }
}

#[test]
fn known_local_error_tag_cannot_hide_a_reachable_wrong_payload_raw_or_wire() {
    use zil_base::{Limits, TargetProfile};
    use zil_ir::{InstructionKind, PlaceProjection};
    let input = SourceUnit::new(
        FileId(0),
        ModuleId(0),
        "local-owning-error-raw",
        LOCAL_OWNING_ERROR_TAGS,
    );
    let syntax = zil_syntax::parse(&input).into_result().unwrap();
    let core = zil_expand::expand(&input, &syntax).unwrap();
    let target = TargetProfile::default();
    let limits = Limits::default();
    let original = zil_types::check(&core, &target, &limits).unwrap();
    let plan = zil_analyze::analyze(&original, &limits).unwrap();
    zil_verify::verify_module(original.clone(), plan.clone(), &target, &limits).unwrap();
    let wire = zil_ir::wire::encode_module_candidate(&original, &plan, &limits).unwrap();
    zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap();
    for mutation in [
        "wrong-arm",
        "false-observation",
        "joined-wrong-arm",
        "live-debt",
        "stale-observation",
        "stale-call",
    ] {
        let mut wrong = original.clone();
        let joined = mutation == "joined-wrong-arm";
        let function = wrong
            .functions
            .iter_mut()
            .find(|f| f.name == if joined { "joined" } else { "local_ok" })
            .unwrap();
        if mutation == "live-debt" {
            let mut extra = function
                .blocks
                .iter()
                .flat_map(|b| &b.instructions)
                .find(|i| matches!(i.kind, InstructionKind::MakeManaged { .. }))
                .unwrap()
                .clone();
            let value = zil_ir::ValueId(function.values.len() as u32);
            function
                .values
                .push(function.values[extra.result.unwrap().0 as usize].clone());
            extra.result = Some(value);
            let live = function
                .blocks
                .iter_mut()
                .find(|b| {
                    b.instructions.iter().any(|i| {
                        matches!(&i.kind, InstructionKind::TakeVariantField { path, .. }
                    if path == &[PlaceProjection::ErrorArm(false)])
                    })
                })
                .unwrap();
            let at = live
                .instructions
                .iter()
                .position(|i| matches!(i.kind, InstructionKind::TakeVariantField { .. }))
                .unwrap();
            // An additional real Managed producer in the feasible arm has no
            // consumer. Pruning its opposite edge must not hide that SSA debt.
            live.instructions.insert(at + 1, extra);
        } else if matches!(mutation, "stale-observation" | "stale-call") {
            let (block, index) = function
                .blocks
                .iter()
                .enumerate()
                .find_map(|(b, block)| {
                    block
                        .instructions
                        .iter()
                        .position(|i| matches!(i.kind, InstructionKind::PlaceTagLocal { .. }))
                        .map(|i| (b, i))
                })
                .unwrap();
            let mut read = function.blocks[block].instructions[index].clone();
            let InstructionKind::PlaceTagLocal { local, .. } = read.kind else {
                unreachable!()
            };
            let value = zil_ir::ValueId(function.values.len() as u32);
            read.result = Some(value);
            if mutation == "stale-call" {
                // Even an ordinary call invalidates a previously read tag.
                function.values.push(zil_ir::Type::I32);
                read.kind = InstructionKind::Call {
                    function: original
                        .functions
                        .iter()
                        .find(|f| f.name == "local_err")
                        .unwrap()
                        .id,
                    arguments: Vec::new(),
                };
                function.blocks[block].instructions.insert(index + 1, read);
            } else {
                // Restoring the same actual carrier cannot revive the earlier
                // tag observation across Take/Store invalidation.
                function
                    .values
                    .push(function.locals[local.0 as usize].ty.clone());
                read.kind = InstructionKind::TakeLocal { local };
                let mut store = read.clone();
                store.result = None;
                store.kind = InstructionKind::Store { local, value };
                function.blocks[block]
                    .instructions
                    .splice(index + 1..index + 1, [read, store]);
            }
        } else {
            let extraction = function
                .blocks
                .iter_mut()
                .flat_map(|b| &mut b.instructions)
                .find(|i| {
                    matches!(&i.kind, InstructionKind::TakeVariantField { path, .. }
                if path == &[PlaceProjection::ErrorArm(joined)])
                })
                .unwrap();
            let InstructionKind::TakeVariantField { path, .. } = &mut extraction.kind else {
                unreachable!()
            };
            path[0] = PlaceProjection::ErrorArm(!joined);
            if mutation == "false-observation" {
                let tag = function
                    .blocks
                    .iter_mut()
                    .flat_map(|b| &mut b.instructions)
                    .find(|i| matches!(i.kind, InstructionKind::PlaceTagLocal { .. }))
                    .unwrap();
                // A numerically matching constant is not an observation of this
                // carrier and cannot establish an impossible-edge proof.
                tag.kind = InstructionKind::Constant(zil_ir::Constant::I32(0));
            }
        }
        assert!(zil_analyze::analyze(&wrong, &limits).is_err(), "{mutation}");
        let mut claim = plan.clone();
        if matches!(mutation, "live-debt" | "stale-observation" | "stale-call") {
            let function = wrong
                .functions
                .iter()
                .find(|f| f.name == "local_ok")
                .unwrap();
            claim.functions[function.id.0 as usize]
                .holds
                .push(zil_ir::ValueHold {
                    value: zil_ir::ValueId(function.values.len() as u32 - 1),
                    roots: Vec::new(),
                });
        }
        claim.module_fingerprint = wrong.fingerprint();
        let wire = zil_ir::wire::encode_module_candidate(&wrong, &claim, &limits).unwrap();
        for result in [
            zil_verify::verify_module(wrong, claim, &target, &limits),
            zil_verify::verify_module_bytes(&wire, &target, &limits),
        ] {
            let errors = result.expect_err(mutation);
            assert!(
                errors.iter().any(|d| d.phase == Phase::Verify
                    && d.code
                        == if mutation == "live-debt" {
                            "E-VERIFY-RESOURCE"
                        } else {
                            "E-VERIFY-SUM"
                        }),
                "{mutation}: {errors:#?}"
            );
            assert!(
                errors
                    .iter()
                    .all(|d| !matches!(d.code.as_str(), "E-VERIFY-PLAN" | "E-VERIFY-HOLD")),
                "{mutation}: {errors:#?}"
            );
        }
    }
}

#[test]
fn trait_default_and_impl_recipe_keep_symbolic_error_arm_at_equal_instance() {
    let source = r#"
trait DefaultChoice<E:Copy,T:Copy>{
 fn default_pick(self:&Self,failure:E,success:T,fail:bool)->E!T{
  if fail{return failure}
  return success
 }
}
trait RecipeChoice<E:Copy,T:Copy>{
 fn recipe_pick(self:&Self,failure:E,success:T,fail:bool)->E!T;
}
struct Box<U:Copy>{value:U}
impl DefaultChoice<i32,i32> for Box<i32>{}
impl<E:Copy,T:Copy,U:Copy> RecipeChoice<E,T> for Box<U>{
 fn recipe_pick(failure:E,success:T,fail:bool)->E!T{
  if fail{return failure}
  return success
 }
}
fn recipe<E:Copy,T:Copy,X:RecipeChoice<E,T>>(x:&X,failure:E,success:T,flag:bool)->E!T{
 return x.recipe_pick(failure,success,flag)
}
fn observe(value:i32!i32)->i32{match value{.ok(v)=>{return v},.err(v)=>{return -v}}}
fn main()->i32{
 let x=Box::<i32>{value:1};
 let a=observe(x.default_pick(7,42,false));
 let b=observe(x.default_pick(7,42,true));
 let c=observe(recipe::<i32,i32,Box<i32>>(&x,7,42,false));
 let d=observe(recipe::<i32,i32,Box<i32>>(&x,7,42,true));
 return a+b+c+d-70
}
"#;
    let temp = Temp::new();
    let input = temp.0.join("trait-equal-error.zil");
    fs::write(&input, source).unwrap();
    for optimization in [Optimization::O0, Optimization::O2] {
        let report = compile_path(
            &input,
            &CompileOptions {
                stop_after: Stage::Build,
                optimization,
                output: Some(temp.0.join(format!("trait-{optimization:?}"))),
                ..CompileOptions::default()
            },
        );
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        run(&report.artifacts["executable"], b"", 0, "");
    }
    for (name, source) in [
        (
            "trait-ambiguous-formal",
            "trait Ambiguous<T:Copy>{fn value(self:&Self,x:T)->T!T{return x}}struct Host{}impl Ambiguous<i32> for Host{}fn main()->i32{return 0}",
        ),
        (
            "trait-wrong-explicit-arm",
            "trait Wrong<E:Copy,T:Copy>{fn value(self:&Self,x:E)->E!T{return error_ok::<E,T>(x)}}struct Host{}impl Wrong<i32,bool> for Host{}fn main()->i32{return 0}",
        ),
    ] {
        let report = compile_source(
            &SourceUnit::new(FileId(0), ModuleId(0), name, source),
            &CompileOptions {
                stop_after: Stage::Verify,
                ..CompileOptions::default()
            },
        );
        assert_ne!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 0, "{}", report.to_json());
    }
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

#[test]
fn generic_call_arguments_keep_definition_selected_error_tags_at_equal_instances() {
    for (definition, expected) in [
        (
            "fn sink<T:Copy>(v:T!i32,w:T)->i32{match v{.ok(n)=>{return n},.err(_)=>{return 19}}}fn relay<U:Copy>(w:U)->i32{return sink(7,w)}",
            7,
        ),
        (
            "fn sink<T:Copy>(w:T,v:T!i32)->i32{match v{.ok(n)=>{return n},.err(_)=>{return 19}}}fn relay<U:Copy>(w:U)->i32{return sink(w,7)}",
            7,
        ),
        (
            "fn sink<T:Copy>(v:T!i32,w:T)->i32{match v{.ok(_)=>{return 19},.err(_)=>{return 7}}}fn relay<U:Copy>(w:U)->i32{return sink(w,w)}",
            7,
        ),
        (
            "fn sink<T:Copy>(w:T,v:T!i32)->i32{match v{.ok(_)=>{return 19},.err(_)=>{return 7}}}fn relay<U:Copy>(w:U)->i32{return sink(w,w)}",
            7,
        ),
    ] {
        let text = format!("{definition}fn result()->i32{{return relay(0)}}fn main(){{}}");
        let input = SourceUnit::new(FileId(0), ModuleId(0), "generic-error-argument.zil", text);
        // The raw and Wire consumers independently reconstruct the actual
        // tagged arguments before either native backend observes the result.
        let syntax = zil_syntax::parse(&input).into_result().unwrap();
        let core = zil_expand::expand(&input, &syntax).unwrap();
        let limits = zil_base::Limits::default();
        let target = zil_base::TargetProfile::default();
        let raw = zil_types::check(&core, &target, &limits).unwrap();
        let plan = zil_analyze::analyze(&raw, &limits).unwrap();
        zil_verify::verify_module(raw.clone(), plan.clone(), &target, &limits).unwrap();
        let wire = zil_ir::wire::encode_module_candidate(&raw, &plan, &limits).unwrap();
        zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap();
        let temp = Temp::new();
        for backend in [NativeBackend::Clang, NativeBackend::LlvmApi] {
            if backend == NativeBackend::LlvmApi && !cfg!(feature = "llvm-api") {
                continue;
            }
            for optimization in [Optimization::O0, Optimization::O2] {
                let report = compile_source(
                    &input,
                    &CompileOptions {
                        stop_after: Stage::Build,
                        entry: Some("result".into()),
                        backend,
                        optimization,
                        output: Some(
                            temp.0
                                .join(format!("argument-{backend:?}-{optimization:?}")),
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
                let codegen = report.native_codegen.as_ref().unwrap();
                assert_eq!(codegen.backend, backend);
                assert!(codegen.fallback.is_none());
                run(
                    &report.artifacts["executable"],
                    b"",
                    0,
                    &format!("{expected}\n"),
                );
            }
        }
    }
}

#[test]
fn generic_error_argument_choices_survive_ct_and_keep_nearest_rejections() {
    let temp = Temp::new();
    let text = "fn sink<T:Copy>(v:T!i32,w:T)->i32{match v{.ok(n)=>{return n},.err(_)=>{return 19}}}fn relay<U:Copy>(w:U)->i32{return sink(#(7),w)}fn result()->i32{return relay(0)}fn main(){}";
    let report = compile_source(
        &SourceUnit::new(FileId(0), ModuleId(0), "ct-error-argument.zil", text),
        &CompileOptions {
            stop_after: Stage::Build,
            entry: Some("result".into()),
            output: Some(temp.0.join("ct-argument")),
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
    run(&report.artifacts["executable"], b"", 0, "7\n");
    // Base9f already rejects both complete CT calls: distinct bool!i32 is
    // not a serializable CT value, while i32!i32 lacks the old argument choice.
    // Retaining explicit tags does not enable execution of type applications.
    for actual in ["0", "false"] {
        let text = format!(
            "fn sink<T:Copy>(v:T!i32,w:T)->i32{{match v{{.ok(n)=>{{return n}},.err(_)=>{{return 19}}}}}}fn relay<U:Copy>(w:U)->i32{{return sink(7,w)}}fn main()->i32{{return #(relay({actual}))}}"
        );
        let report = compile_source(
            &SourceUnit::new(FileId(0), ModuleId(0), "ct-constructor-boundary.zil", text),
            &CompileOptions::default(),
        );
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 0);
        assert!(
            report
                .diagnostics
                .iter()
                .any(|e| e.phase == Phase::Ct && e.code == "E-CT-FORBIDDEN"),
            "{}",
            report.to_json()
        );
    }
    for text in [
        "fn sink<T:Copy>(v:T!T,w:T)->i32{return 0}fn relay<U:Copy>(w:U)->i32{return sink(w,w)}fn main(){}",
        "fn sink<T:Copy>(w:T,v:T!T)->i32{return 0}fn relay<U:Copy>(w:U)->i32{return sink(w,w)}fn main(){}",
        "struct A{n:i32}struct B{n:i32}fn sink<T:Copy>(v:T!A,w:T)->i32{return 0}fn relay<U:Copy>(w:U)->i32{return sink(B{n:7},w)}fn main(){}",
    ] {
        let report = compile_source(
            &SourceUnit::new(FileId(0), ModuleId(0), "bad-error-argument.zil", text),
            &CompileOptions::default(),
        );
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 0);
        assert!(
            report
                .diagnostics
                .iter()
                .any(|e| e.phase == Phase::Expand && e.code == "E-GENERIC-ARGUMENT"),
            "{}",
            report.to_json()
        );
    }
}

#[test]
fn generic_error_argument_anchors_survive_trait_materialization() {
    let text = r#"
fn sink<T:Copy>(v:T!i32,w:T)->i32{match v{.ok(n)=>{return n},.err(_)=>{return 19}}}
trait DefaultArgument<E:Copy>{fn pick(self:&Self,w:E)->i32{return sink(7,w)}}
struct Host{}
impl DefaultArgument<i32> for Host{}
trait RecipeArgument<E:Copy>{fn pick(self:&Self,w:E)->i32;}
struct Box<T:Copy>{value:T}
impl<E:Copy> RecipeArgument<E> for Box<E>{fn pick(w:E)->i32{return sink(7,w)}}
trait Read{fn value(self:&Self)->i32;}
impl Read for Host{fn value()->i32{return 7}}
fn callee_argument<U:Copy,X:Read>(w:U,x:&X)->i32{return sink(x.value(),w)}
fn result()->i32{let h=Host{};let b=Box::<i32>{value:0};return h.pick(0)+b.pick(0)+callee_argument(0,&h)}
fn main(){}
"#;
    let temp = Temp::new();
    for backend in [NativeBackend::Clang, NativeBackend::LlvmApi] {
        if backend == NativeBackend::LlvmApi && !cfg!(feature = "llvm-api") {
            continue;
        }
        for optimization in [Optimization::O0, Optimization::O2] {
            let report = compile_source(
                &SourceUnit::new(FileId(0), ModuleId(0), "trait-error-arguments.zil", text),
                &CompileOptions {
                    stop_after: Stage::Build,
                    entry: Some("result".into()),
                    backend,
                    optimization,
                    output: Some(
                        temp.0
                            .join(format!("trait-arguments-{backend:?}-{optimization:?}")),
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
            let codegen = report.native_codegen.as_ref().unwrap();
            assert_eq!(codegen.backend, backend);
            assert!(codegen.fallback.is_none());
            run(&report.artifacts["executable"], b"", 0, "21\n");
        }
    }
}

#[test]
fn explicit_generic_store_results_require_actual_success_ownership_and_freshness() {
    let text = r#"
fn fresh<T:StoreElement>()->bool!Store<T> contract{result(create)}{return error_ok::<bool,Store<T>>(store_new())}
fn passed<T:StoreElement>(give items:Store<T>)->bool!Store<T> contract{result(give)}{return error_ok::<bool,Store<T>>(give items)}
fn failed<T:StoreElement>()->bool!Store<T> contract{result(create)}{return error_err::<bool,Store<T>>(true)}
fn attempt()->bool!i32{let items=fresh::<i32>()?;store_reserve(items,1);store_push(items,7);let moved=passed(give items)?;match failed::<i32>(){.ok(_)=>{return 19},.err(_)=>{return i32(store_len(moved))}}}
fn result()->i32{match attempt(){.ok(n)=>{return n},.err(_)=>{return 19}}}
fn main(){}
"#;
    let input = SourceUnit::new(FileId(0), ModuleId(0), "generic-store-raw.zil", text);
    let syntax = zil_syntax::parse(&input).into_result().unwrap();
    let core = zil_expand::expand(&input, &syntax).unwrap();
    let limits = zil_base::Limits::default();
    let target = zil_base::TargetProfile::default();
    let raw = zil_types::check(&core, &target, &limits).unwrap();
    let plan = zil_analyze::analyze(&raw, &limits).unwrap();
    zil_verify::verify_module(raw.clone(), plan.clone(), &target, &limits).unwrap();
    let wire = zil_ir::wire::encode_module_candidate(&raw, &plan, &limits).unwrap();
    zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap();
    let temp = Temp::new();
    for backend in [NativeBackend::Clang, NativeBackend::LlvmApi] {
        if backend == NativeBackend::LlvmApi && !cfg!(feature = "llvm-api") {
            continue;
        }
        for optimization in [Optimization::O0, Optimization::O2] {
            let report = compile_source(
                &SourceUnit::new(FileId(0), ModuleId(0), "explicit-generic-store.zil", text),
                &CompileOptions {
                    stop_after: Stage::Build,
                    entry: Some("result".into()),
                    backend,
                    optimization,
                    output: Some(
                        temp.0
                            .join(format!("generic-store-{backend:?}-{optimization:?}")),
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
            let codegen = report.native_codegen.as_ref().unwrap();
            assert_eq!(codegen.backend, backend);
            assert!(codegen.fallback.is_none());
            run(&report.artifacts["executable"], b"", 0, "1\n");
        }
    }
    for text in [
        "fn bad<T:StoreElement>(items:Store<T>)->bool!Store<T> contract{result(give)}{return error_ok::<bool,Store<T>>(items)}fn main(){}",
        "fn bad<T:StoreElement>(give items:Store<T>)->bool!Store<T> contract{result(create)}{return error_ok::<bool,Store<T>>(give items)}fn main(){}",
        "fn bad<T:StoreElement>()->bool!Store<T> contract{result(create)}{let items:Store<T>=store_new();let desc=items;return error_ok::<bool,Store<T>>(desc)}fn main(){}",
    ] {
        let report = compile_source(
            &SourceUnit::new(FileId(0), ModuleId(0), "invalid-tagged-store.zil", text),
            &CompileOptions::default(),
        );
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 0);
        assert!(
            report
                .diagnostics
                .iter()
                .any(|e| e.phase == Phase::Expand && e.code == "E-GENERIC-ARGUMENT"),
            "{}",
            report.to_json()
        );
    }
}
