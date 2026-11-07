//! Independent ownership, empty-error and cleanup probes for language 183.
use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_base::{FileId, Limits, ModuleId, Phase, ResultCategory, SourceUnit, TargetProfile};
use zil_driver::{CompileOptions, Optimization, Stage, compile_source};
use zil_ir::InstructionKind as I;
static NEXT: AtomicUsize = AtomicUsize::new(0);
fn directory(label: &str) -> PathBuf {
    let p = std::env::var_os("ZIL_TEST_REPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join(format!(
            "review183-{label}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
    fs::create_dir_all(&p).unwrap();
    p
}
const MODEL: &str = r#"extern "C" fn putchar(n:i32)->i32;
resource R{code:i32}impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}enum E{Fail}
fn make(ok:bool)->E!R contract{result(create)}{if ok{return R{code:65}};return E.Fail}
"#;
fn native(label: &str, source: &str, stdout: &[u8], exit: i32) {
    for optimization in [Optimization::O0, Optimization::O2] {
        let p = directory(label);
        fs::write(p.join("main.zil"), source).unwrap();
        let unit = SourceUnit::new(FileId(0), ModuleId(0), "review183.zil", source);
        let output = p.join("program");
        let r = compile_source(
            &unit,
            &CompileOptions {
                stop_after: Stage::Build,
                optimization,
                output: Some(output.clone()),
                emit_dir: Some(p.join("phases")),
                ..CompileOptions::default()
            },
        );
        fs::write(p.join("compile.json"), r.to_json()).unwrap();
        assert_eq!(r.status, ResultCategory::Success, "{}", r.to_json());
        assert_eq!(r.verification_runs, 1);
        let result = Command::new(&output).output().unwrap();
        fs::write(p.join("stdout.log"), &result.stdout).unwrap();
        fs::write(p.join("stderr.log"), &result.stderr).unwrap();
        fs::write(p.join("native.json"),format!("{{\"optimization\":\"{optimization:?}\",\"exit\":{},\"expected_exit\":{exit},\"stdout\":{:?},\"expected_stdout\":{:?},\"command\":{:?}}}\n",result.status.code().unwrap(),String::from_utf8_lossy(&result.stdout),String::from_utf8_lossy(stdout),output)).unwrap();
        assert_eq!(result.status.code(), Some(exit));
        assert_eq!(result.stdout, stdout);
        assert!(result.stderr.is_empty());
    }
}
#[test]
fn alternative_incoming_carriers_keep_one_owner_on_both_tags() {
    for (ok, choose, stdout, exit) in [
        (true, true, b"LBAZ".as_slice(), 0),
        (true, false, b"LABZ".as_slice(), 0),
        (false, true, b"LBZ".as_slice(), 1),
        (false, false, b"LBZ".as_slice(), 0),
    ] {
        let source = format!(
            "{MODEL}fn select(give incoming:E!R,give spare:R,choose:bool)->E!R contract{{result(give)}}{{let local=R{{code:76}};if choose{{return incoming}};return give spare}}fn main()->E!i32{{let guard=R{{code:90}};let r=select(make({ok}),R{{code:66}},{choose})?;return 0}}"
        );
        native("alternative", &source, stdout, exit);
    }
    let source = format!(
        "{MODEL}fn select(give incoming:E!R,choose:bool)->E!R contract{{result(create)}}{{if choose{{return R{{code:66}}}};return incoming}}fn main(){{select(make(true),false)}}"
    );
    let p = directory("not-fresh");
    fs::write(p.join("main.zil"), &source).unwrap();
    let unit = SourceUnit::new(FileId(0), ModuleId(0), "review183.zil", source);
    let r = compile_source(&unit, &CompileOptions::default());
    fs::write(p.join("compile.json"), r.to_json()).unwrap();
    assert_eq!(r.status, ResultCategory::SafeReject, "{}", r.to_json());
    assert_eq!(r.phase, Phase::Analyze);
    assert!(
        r.diagnostics
            .iter()
            .any(|d| d.code == "E-ANALYZE-IR" && d.message.contains("incoming responsibility")),
        "{}",
        r.to_json()
    );
}
#[test]
fn error_only_create_after_question_has_no_success_responsibility() {
    for (ok, stdout) in [(true, b"AHZ".as_slice()), (false, b"HZ".as_slice())] {
        let source = format!(
            "{MODEL}fn discard(give x:E!R)->E!R contract{{result(create)}}{{let guard=R{{code:72}};let y=x?;return E.Fail}}fn main()->E!i32{{let guard=R{{code:90}};let r=discard(make({ok}))?;return r.code}}"
        );
        native("error-only", &source, stdout, 1);
    }
}
fn candidate(source: &str) -> (zil_ir::TypedModule, zil_ir::CandidatePlan) {
    let unit = SourceUnit::new(FileId(0), ModuleId(0), "review183.zil", source);
    let syntax = zil_syntax::parse(&unit).into_result().unwrap();
    let core = zil_expand::expand(&unit, &syntax).unwrap();
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    let module = zil_types::check(&core, &target, &limits).unwrap();
    let plan = zil_analyze::analyze(&module, &limits).unwrap();
    zil_verify::verify_module(module.clone(), plan.clone(), &target, &limits).unwrap();
    (module, plan)
}
#[test]
fn raw_and_wire_reject_duplicate_cleanup_even_when_call_returns_an_error() {
    for ok in [true, false] {
        let source = format!("{MODEL}fn main(){{let pending=make({ok})}}");
        let (mut module, mut plan) = candidate(&source);
        let f = module
            .functions
            .iter_mut()
            .find(|f| f.name == "main")
            .unwrap();
        let mut changed = false;
        for block in &mut f.blocks {
            if let Some(i) = block
                .instructions
                .iter()
                .position(|i| matches!(i.kind, I::DropErrorUnion { .. }))
            {
                let duplicate = block.instructions[i].clone();
                block.instructions.insert(i + 1, duplicate);
                changed = true;
                break;
            }
        }
        assert!(changed);
        plan.module_fingerprint = module.fingerprint();
        let limits = Limits::default();
        let target = module.target.clone();
        let bytes = zil_ir::wire::encode_module_candidate(&module, &plan, &limits).unwrap();
        let p = directory("duplicate-cleanup");
        fs::write(p.join("main.zil"), source).unwrap();
        fs::write(p.join("candidate.bin"), &bytes).unwrap();
        for (route, result) in [
            (
                "raw",
                zil_verify::verify_module(module, plan, &target, &limits),
            ),
            (
                "wire",
                zil_verify::verify_module_bytes(&bytes, &target, &limits),
            ),
        ] {
            let errors = result.unwrap_err();
            fs::write(p.join(format!("{route}.txt")), format!("{errors:#?}\n")).unwrap();
            assert!(errors.iter().all(|d| d.phase == Phase::Verify));
            assert!(
                errors.iter().any(|d| d.code == "E-VERIFY-RESOURCE"
                    && d.message.contains("duplicate automatic cleanup")),
                "{route}: {errors:#?}"
            );
        }
    }
}
