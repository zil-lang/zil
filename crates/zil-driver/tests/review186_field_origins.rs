//! Independent review of186's direct Managed Package field birth reconstruction.
use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_base::{FileId, Limits, ModuleId, Phase, ResultCategory, SourceUnit, TargetProfile};
use zil_driver::{CompileOptions, Optimization, Stage, compile_source};
use zil_ir::{CandidatePlan, InstructionKind as I, PassingMode, TypedModule};
static NEXT: AtomicUsize = AtomicUsize::new(0);
const MODEL: &str =
    "resource R{n:i32}package P{pad:i32;manages old:R;wide:[i32;2];manages fresh:R}";
const CONSTRUCT: &str = "fn origin(give old:R)->R contract{result(create)}{let p=P{pad:7,old:give old,wide:[11,13],fresh:R{n:29}};let q=p;return give q.fresh}";
fn directory(label: &str) -> PathBuf {
    let base = std::env::var_os("ZIL_TEST_REPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let dir = base.join(format!(
        "review186-{label}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&dir).unwrap();
    dir
}
fn unit(text: &str) -> SourceUnit {
    SourceUnit::new(FileId(0), ModuleId(0), "review186.zil", text)
}
fn typed(text: &str) -> TypedModule {
    let source = unit(text);
    let ast = zil_syntax::parse(&source).into_result().unwrap();
    let core = zil_expand::expand(&source, &ast).unwrap();
    zil_types::check(&core, &TargetProfile::host_llvm(), &Limits::default()).unwrap()
}
fn native(text: &str) {
    for optimization in [Optimization::O0, Optimization::O2] {
        let dir = directory("native");
        fs::write(dir.join("source.zil"), text).unwrap();
        let executable = dir.join("program");
        let report = compile_source(
            &unit(text),
            &CompileOptions {
                stop_after: Stage::Build,
                optimization,
                emit_dir: Some(dir.join("phases")),
                output: Some(executable.clone()),
                ..CompileOptions::default()
            },
        );
        fs::write(dir.join("report.json"), report.to_json()).unwrap();
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1);
        let result = Command::new(executable).output().unwrap();
        fs::write(dir.join("stdout.log"), &result.stdout).unwrap();
        fs::write(dir.join("stderr.log"), &result.stderr).unwrap();
        fs::write(dir.join("exit.txt"), format!("{:?}", result.status.code())).unwrap();
        assert_eq!(result.status.code(), Some(0));
        assert!(result.stdout.is_empty() && result.stderr.is_empty());
    }
}
fn reject_source(text: &str) {
    let dir = directory("source-rejection");
    fs::write(dir.join("source.zil"), text).unwrap();
    let report = compile_source(
        &unit(text),
        &CompileOptions {
            emit_dir: Some(dir.join("phases")),
            ..CompileOptions::default()
        },
    );
    fs::write(dir.join("report.json"), report.to_json()).unwrap();
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
    assert_eq!(report.phase, Phase::Analyze, "{}", report.to_json());
    assert!(
        report.diagnostics.iter().any(|d| d
            .message
            .contains("create result forwards an incoming responsibility")),
        "{}",
        report.to_json()
    );
}
fn baseline() -> (TypedModule, CandidatePlan) {
    let m = typed(&format!("{MODEL}{CONSTRUCT}fn main(){{}}"));
    let p = zil_analyze::analyze(&m, &Limits::default()).unwrap();
    zil_verify::verify_module(m.clone(), p.clone(), &m.target, &Limits::default()).unwrap();
    (m, p)
}
fn reject_raw(label: &str, m: TypedModule, mut p: CandidatePlan) {
    p.module_fingerprint = m.fingerprint();
    let bytes = zil_ir::wire::encode_module_candidate(&m, &p, &Limits::default()).unwrap();
    assert_eq!(
        zil_ir::wire::decode_module_candidate(&bytes, &Limits::default()).unwrap(),
        (m.clone(), p.clone())
    );
    let dir = directory(label);
    fs::write(dir.join("module.txt"), format!("{m:#?}")).unwrap();
    fs::write(dir.join("candidate.txt"), format!("{p:#?}")).unwrap();
    for (route, result) in [
        (
            "raw",
            zil_verify::verify_module(m.clone(), p.clone(), &m.target, &Limits::default()),
        ),
        (
            "wire",
            zil_verify::verify_module_bytes(&bytes, &m.target, &Limits::default()),
        ),
    ] {
        let errors = result.unwrap_err();
        fs::write(dir.join(format!("{route}.txt")), format!("{errors:#?}")).unwrap();
        assert!(
            errors.iter().any(|d| d.phase == Phase::Verify
                && d.category == ResultCategory::SafeReject
                && d.code == "E-VERIFY-RESOURCE"
                && d.message.contains("proven fresh")),
            "{label}/{route}: {errors:?}"
        );
    }
}

#[test]
fn reviewer_mixed_field_positions_whole_moves_and_input_field_replacement_keep_creation() {
    native(&format!(
        "{MODEL}{CONSTRUCT}fn main()->i32{{let value=origin(R{{n:17}});return value.n-29}}"
    ));
    native(&format!(
        "{MODEL}fn regenerate(p:P)->R contract{{result(create)}}{{var current=p;current.fresh=R{{n:31}};let moved=current;return give moved.fresh}}fn main()->i32{{let p=P{{pad:7,old:R{{n:17}},wide:[11,13],fresh:R{{n:19}}}};let v=regenerate(p);return v.n-31}}"
    ));
    native(
        "resource R{n:i32}package Mixed{pad:i32;manages old:R;wide:[i32;2];manages bytes:ByteBuffer}fn create(give old:R)->ByteBuffer contract{result(create)}{let p=Mixed{pad:7,old:give old,wide:[11,13],bytes:bytes_new()};let moved=p;return give moved.bytes}fn main()->i32{let b=create(R{n:17});return i32(bytes_len(b))}",
    );
}

#[test]
fn reviewer_whole_package_branch_joins_keep_each_new_field() {
    for flag in [false, true] {
        native(&format!(
            "{MODEL}fn choose(give old:R,flag:bool)->R contract{{result(create)}}{{let p=if flag{{P{{pad:7,old:give old,wide:[11,13],fresh:R{{n:29}}}}}}else{{P{{pad:9,old:give old,wide:[17,19],fresh:R{{n:29}}}}}};let q=p;return give q.fresh}}fn main()->i32{{let v=choose(R{{n:17}},{flag});return v.n-29}}"
        ));
    }
}

#[test]
fn reviewer_incoming_fields_zero_iteration_paths_and_give_calls_stay_nonfresh() {
    for definition in [
        "fn bad(give old:R)->R contract{result(create)}{let p=P{pad:7,old:give old,wide:[11,13],fresh:R{n:29}};let q=p;return give q.old}",
        "fn bad(p:P,count:i32)->R contract{result(create)}{var q=p;var i=0;while i<count{q.fresh=R{n:29};i+=1}return give q.fresh}",
        "fn bad(give old:R)->R contract{result(create)}{var p=P{pad:7,old:give old,wide:[11,13],fresh:R{n:29}};let incoming=give p.old;p.fresh=give incoming;return give p.fresh}",
        "fn forward(give v:R)->R contract{result(give)}{return give v}fn bad()->R contract{result(create)}{let p=P{pad:7,old:R{n:17},wide:[11,13],fresh:forward(R{n:29})};let q=p;return give q.fresh}",
        "fn opaque()->P{return P{pad:7,old:R{n:17},wide:[11,13],fresh:R{n:29}}}fn bad()->R contract{result(create)}{let p=opaque();let q=p;return give q.fresh}",
    ] {
        reject_source(&format!("{MODEL}{definition}fn main(){{}}"));
    }
}

#[test]
fn reviewer_raw_and_wire_must_follow_construction_operand_and_physical_field_index() {
    let (mut m, p) = baseline();
    let f = m.functions.iter_mut().find(|f| f.name == "origin").unwrap();
    let mut changed = false;
    for i in f.blocks.iter_mut().flat_map(|b| &mut b.instructions) {
        if let I::MakePackage { fields } = &mut i.kind
            && fields.len() == 4
        {
            fields.swap(1, 3);
            changed = true;
        }
    }
    assert!(changed);
    reject_raw("swapped-construction-origins", m, p);
    let (mut m, p) = baseline();
    let f = m.functions.iter_mut().find(|f| f.name == "origin").unwrap();
    let mut changed = false;
    for i in f.blocks.iter_mut().flat_map(|b| &mut b.instructions) {
        if let I::TakePackageField { index, .. } = &mut i.kind
            && *index == 3
        {
            *index = 1;
            changed = true;
        }
    }
    assert!(changed);
    reject_raw("retargeted-physical-field", m, p);
}

#[test]
fn reviewer_zero_iteration_may_incoming_survives_forged_create_contract() {
    let source = format!(
        "{MODEL}fn maybe(p:P,count:i32)->R contract{{result(give)}}{{var q=p;var i=0;while i<count{{q.fresh=R{{n:29}};i+=1}}return give q.fresh}}fn main(){{}}"
    );
    let mut m = typed(&source);
    let p = zil_analyze::analyze(&m, &Limits::default()).unwrap();
    zil_verify::verify_module(m.clone(), p.clone(), &m.target, &Limits::default()).unwrap();
    m.functions
        .iter_mut()
        .find(|f| f.name == "maybe")
        .unwrap()
        .result_mode = PassingMode::Create;
    reject_raw("zero-iteration-input", m, p);
}

#[test]
fn reviewer_package_origin_tables_have_exact_analyze_and_verify_budget_boundaries() {
    let (m, p) = baseline();
    let mut boundaries = Vec::new();
    for verify in [false, true] {
        let mut low = 0;
        let mut high = Limits::default().max_analysis_steps;
        while low + 1 < high {
            let middle = low + (high - low) / 2;
            let limits = Limits {
                max_analysis_steps: middle,
                ..Limits::default()
            };
            let r = if verify {
                zil_verify::verify_module(m.clone(), p.clone(), &m.target, &limits).map(|_| ())
            } else {
                zil_analyze::analyze(&m, &limits).map(|_| ())
            };
            match r {
                Ok(()) => high = middle,
                Err(ds) => {
                    assert!(
                        ds.iter()
                            .all(|d| d.category == ResultCategory::ResourceLimit),
                        "{ds:?}"
                    );
                    low = middle;
                }
            }
        }
        let limits = Limits {
            max_analysis_steps: high,
            ..Limits::default()
        };
        if verify {
            zil_verify::verify_module(m.clone(), p.clone(), &m.target, &limits).unwrap();
        } else {
            zil_analyze::analyze(&m, &limits).unwrap();
        }
        boundaries.push((verify, low, high));
    }
    let dir = directory("budgets");
    fs::write(dir.join("boundaries.txt"), format!("{boundaries:?}")).unwrap();
}
