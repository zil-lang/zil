//! Actual shared resource heads through independent verification and native code.
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
        "resource-heads171-{}-{}",
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
resource R{code:i32;n:i32}
impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
impl R{fn peek(self:&Self)->i32=>.n;fn consume(give self:Self)->i32=>.n}
fn read(r:&R)->i32=>r.n
fn write(r:R,n:i32){r.n=n}
"#;

#[test]
fn original_159_resource_address_receiver_probe_is_unchanged_and_now_supported() {
    let report = check(&[(
        "main",
        "trait Read{fn read(self:&Self)->i32;}resource C{x:i32}impl Read for C{fn read()->i32=>.x}fn main(){}",
    )]);
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
}

#[test]
fn actual_head_trait_and_inherent_methods_and_plain_parameters() {
    native(
        &[(
            "main",
            r#"
trait Read{fn read(self:&Self)->i32;}
resource C{x:i32}
impl Read for C{fn read()->i32=>.x}
impl C{fn twice(self:&Self)->i32=>.x+.x}
fn inspect(p:&C)->i32=>p.read()+p.twice()
fn main()->i32{let c=C{x:7};return inspect(&c)-21}
"#,
        )],
        b"",
        0,
    );
    let source = format!(
        "{RESOURCE} fn main()->i32{{let r=R{{code:65,n:42}};let p=&r;if p.peek()!=42||read(p)!=42{{return 1}};return (give r).consume()-42}}"
    );
    native(&[("main", &source)], b"A", 0);
}

#[test]
fn last_use_then_descriptor_write_is_allowed() {
    let source = format!(
        "{RESOURCE} fn main()->i32{{let r=R{{code:65,n:1}};let p=&r;if p.n!=1{{return 1}};write(r,42);return r.peek()-42}}"
    );
    native(&[("main", &source)], b"A", 0);
}

#[test]
fn active_head_prevents_write_move_replacement_and_old_address_revival() {
    for action in [
        "write(r,43)",
        "r.n=43",
        "let d=r;write(d,43)",
        "let n=(give r).consume()",
    ] {
        let source = format!(
            "{RESOURCE} fn main()->i32{{var r=R{{code:65,n:42}};let p=&r;{action};return p.n}}"
        );
        let report = check(&[("main", &source)]);
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{action}: {}",
            report.to_json()
        );
        assert!(
            report
                .diagnostics
                .iter()
                .any(|d| d.code == "E-BORROW-CONFLICT"),
            "{action}: {}",
            report.to_json()
        );
    }
}

#[test]
fn heads_are_not_descriptors_mutable_windows_returns_or_captures() {
    for body in [
        "fn main(){let r=R{code:65,n:42};let d=r;let p=&d}",
        "fn main(){var r=R{code:65,n:42};let p=&mut r}",
        "fn bad(p:&R)->&R{return p}fn main(){}",
        "fn bad(p:&R)->&i32{return &p.n}fn main(){}",
        "fn main(){let r=R{code:65,n:42};let p=&r;let f=()=>p.n}",
        "fn main(){let r=R{code:65,n:42};let f=move()=>{let p=&r;return p.n}}",
        "fn main(){let b=bytes_new();let p=&b}",
        "fn main(){var r=R{code:65,n:42};let p=&r;r=R{code:66,n:43};let x=p.n}",
    ] {
        let source = format!("{RESOURCE}{body}");
        let report = check(&[("main", &source)]);
        assert_eq!(
            report.status,
            ResultCategory::Unsupported,
            "{body}: {}",
            report.to_json()
        );
        assert_eq!(
            report.phase,
            zil_base::Phase::Types,
            "{body}: {}",
            report.to_json()
        );
    }
}

#[test]
fn descriptor_and_head_parameters_do_not_assume_distinct_origins() {
    for relay in [
        "fn effect(d:R,h:&R)->i32{write(d,43);return h.n}",
        "fn helper(d:R){write(d,43)}fn effect(d:R,h:&R)->i32{helper(d);return h.n}",
    ] {
        let source = format!(
            "{RESOURCE}{relay}fn main()->i32{{let r=R{{code:65,n:42}};return effect(r,&r)}}"
        );
        let report = check(&[("main", &source)]);
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
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
        let disjoint = source.replace(
            "return effect(r,&r)",
            "let other=R{code:66,n:42};return effect(r,&other)-42",
        );
        native(&[("main", &disjoint)], b"BA", 0);
    }
}

#[test]
fn descriptor_capturing_callbacks_cannot_write_under_a_live_head() {
    for make_call in [
        "let d=r;let f=()=>write(d,43);f()",
        "let d=r;let f=()=>write(d,43);let g=move()=>f();g()",
    ] {
        let source = format!(
            "{RESOURCE}fn main()->i32{{let r=R{{code:65,n:42}};let p=&r;{make_call};return p.n}}"
        );
        let report = check(&[("main", &source)]);
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
fn unrelated_owned_callback_and_shared_head_compose() {
    let source = format!(
        "{RESOURCE}fn factory()->impl Fn()->i32{{let r=R{{code:66,n:2}};return move()=>r.n}}fn main()->i32{{let r=R{{code:65,n:40}};let p=&r;let f=factory();return p.n+f()-42}}"
    );
    native(&[("main", &source)], b"BA", 0);
}

#[test]
fn imported_heads_keep_nominal_identity_visibility_and_actual_storage() {
    native(
        &[
            (
                "main",
                "import model;import user;fn main()->i32{let r=model.make(42);return user.inspect(&r)-42}",
            ),
            (
                "model",
                r#"extern "C" fn putchar(x:i32)->i32;
pub trait Read{fn read(self:&Self)->i32;}
pub resource R{pub n:i32}
impl Read for R{pub fn read()->i32=>.n}
impl Drop for R{fn drop(self:Self){unsafe{putchar(65)}}}
pub fn make(n:i32)->R contract{result(create)}{return R{n:n}}"#,
            ),
            (
                "user",
                "import model;pub fn inspect(r:&model.R)->i32=>r.read()",
            ),
        ],
        b"A",
        0,
    );
    let report = check(&[
        (
            "main",
            "import model;fn main()->i32{let r=model.make();let p=&r;return p.secret}",
        ),
        (
            "model",
            "pub resource R{secret:i32}pub fn make()->R contract{result(create)}{return R{secret:42}}",
        ),
    ]);
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.code == "E-FIELD-PRIVATE"),
        "{}",
        report.to_json()
    );
    let report = check(&[(
        "main",
        "resource R{n:i32}resource S{n:i32}fn read(r:&R)->i32=>r.n;fn main()->i32{let s=S{n:42};return read(&s)}",
    )]);
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.code == "E-TYPE-MISMATCH"),
        "{}",
        report.to_json()
    );
}

#[test]
fn desc_only_imported_helpers_export_writes_into_caller_head_window() {
    let model =
        "pub resource R{pub n:i32}pub fn make()->R contract{result(create)}{return R{n:42}}";
    let helpers = "import model;fn inner(r:model.R){r.n=43}pub fn write(r:model.R){inner(r)}";
    let main = "import model;import helpers;fn main()->i32{let r=model.make();let p=&r;helpers.write(r);return p.n}";
    let report = check(&[("main", main), ("model", model), ("helpers", helpers)]);
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
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
    let after = "import model;import helpers;fn main()->i32{let r=model.make();let p=&r;if p.n!=42{return 1};helpers.write(r);return r.n-43}";
    native(
        &[("main", after), ("model", model), ("helpers", helpers)],
        b"",
        0,
    );
}

#[test]
fn resource_trait_nonshared_receivers_and_head_indirect_calls_stay_closed() {
    for source in [
        "trait T{fn f(self:Self)->i32;}resource R{n:i32}impl T for R{fn f()->i32=>.n}fn main(){}",
        "trait T{fn f(self:&mut Self)->i32;}resource R{n:i32}impl T for R{fn f()->i32=>.n}fn main(){}",
        "resource R{n:i32}fn read(r:&R)->i32=>r.n;fn main(){let f=read}",
    ] {
        let report = check(&[("main", source)]);
        assert_eq!(
            report.status,
            ResultCategory::Unsupported,
            "{}",
            report.to_json()
        );
    }
}

fn raw_head() -> (zil_ir::TypedModule, zil_ir::CandidatePlan) {
    let source = SourceUnit::new(
        FileId(0),
        ModuleId(0),
        "raw171.zil",
        "resource R{n:i32}fn read(r:&R)->i32=>r.n;fn main()->i32{let r=R{n:42};let d=r;let p=&r;return read(p)-42}",
    );
    let syntax = zil_syntax::parse(&source).into_result().unwrap();
    let core = zil_expand::expand(&source, &syntax).unwrap();
    let limits = Limits::default();
    let module = zil_types::check(&core, &TargetProfile::host_llvm(), &limits).unwrap();
    let plan = zil_analyze::analyze(&module, &limits).unwrap();
    zil_verify::verify_module(
        module.clone(),
        plan.clone(),
        &TargetProfile::host_llvm(),
        &limits,
    )
    .unwrap();
    assert!(
        module
            .functions
            .iter()
            .flat_map(|f| &f.values)
            .any(zil_ir::Type::is_shared_resource_head_address)
    );
    (module, plan)
}

fn reject_raw_and_wire(
    label: &str,
    module: zil_ir::TypedModule,
    mut plan: zil_ir::CandidatePlan,
    code: &str,
) {
    plan.module_fingerprint = module.fingerprint();
    let target = TargetProfile::host_llvm();
    let limits = Limits::default();
    let bytes = zil_ir::wire::encode_module_candidate(&module, &plan, &limits).unwrap();
    let dir = directory();
    fs::write(dir.join(format!("{label}.bin")), &bytes).unwrap();
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
        let errors = result.expect_err(label);
        fs::write(
            dir.join(format!("{label}-{route}.txt")),
            format!("{errors:#?}"),
        )
        .unwrap();
        assert!(
            errors.iter().all(|d| d.phase == zil_base::Phase::Verify),
            "{label}/{route}: {errors:?}"
        );
        assert!(
            errors.iter().any(|d| d.code == code),
            "{label}/{route}: {errors:?}"
        );
    }
}

#[test]
fn raw_and_wire_cannot_copy_overwrite_forge_or_relabel_a_head() {
    use zil_ir::{Instruction, InstructionKind as I, Type, ValueId};
    let (original, plan) = raw_head();
    let main = original
        .functions
        .iter()
        .position(|f| f.name == "main")
        .unwrap();
    let head = original.functions[main]
        .blocks
        .iter()
        .flat_map(|b| &b.instructions)
        .find_map(|i| {
            if let I::BorrowLocal { local } = i.kind {
                Some((local, i.result.unwrap()))
            } else {
                None
            }
        })
        .unwrap();
    let mut forged = original.clone();
    for instruction in forged.functions[main]
        .blocks
        .iter_mut()
        .flat_map(|b| &mut b.instructions)
    {
        if matches!(instruction.kind, I::BorrowLocal { .. }) {
            instruction.kind = I::Load(head.0);
            break;
        }
    }
    reject_raw_and_wire("copy-head", forged, plan.clone(), "E-VERIFY-RESOURCE");

    let mut forged = original.clone();
    let function = &mut forged.functions[0];
    let instruction = function
        .blocks
        .iter_mut()
        .flat_map(|b| &mut b.instructions)
        .find(|i| matches!(i.kind, I::AddressField { .. }))
        .unwrap();
    let I::AddressField { address, .. } = instruction.kind else {
        unreachable!()
    };
    instruction.kind = I::ReadAddress { address };
    reject_raw_and_wire("whole-head-read", forged, plan.clone(), "E-VERIFY-ADDRESS");

    let mut forged = original.clone();
    let instruction = forged.functions[0]
        .blocks
        .iter_mut()
        .flat_map(|b| &mut b.instructions)
        .find(|i| matches!(i.kind, I::AddressField { .. }))
        .unwrap();
    let I::AddressField { address, .. } = instruction.kind else {
        unreachable!()
    };
    instruction.kind = I::WriteAddress {
        address,
        value: address,
    };
    reject_raw_and_wire("whole-head-write", forged, plan.clone(), "E-VERIFY-ADDRESS");

    let mut forged = original.clone();
    let desc = forged.functions[main]
        .locals
        .iter()
        .find(|l| matches!(l.ty, Type::Desc(_)))
        .unwrap()
        .id;
    let instruction = forged.functions[main]
        .blocks
        .iter_mut()
        .flat_map(|b| &mut b.instructions)
        .find(|i| matches!(i.kind, I::BorrowLocal { .. }))
        .unwrap();
    instruction.kind = I::BorrowLocal { local: desc };
    reject_raw_and_wire("desc-is-not-head", forged, plan.clone(), "E-UNSUPPORTED");

    let mut forged = original.clone();
    if let Type::Record { name, .. } = &mut forged.resources[0].payload {
        *name = "@module0:Other".into()
    }
    reject_raw_and_wire("nominal-payload", forged, plan.clone(), "E-VERIFY-RESOURCE");

    let mut changed_plan = plan.clone();
    changed_plan.functions[main].holds[head.1.0 as usize]
        .roots
        .clear();
    reject_raw_and_wire("fake-hold", original.clone(), changed_plan, "E-VERIFY-HOLD");

    for revive in [false, true] {
        let mut forged = original.clone();
        let function = &mut forged.functions[main];
        let block = &mut function.blocks[0];
        let position = block
            .instructions
            .iter()
            .position(|i| i.result == Some(head.1))
            .unwrap()
            + 1;
        let span = block.instructions[position - 1].span;
        let mut inserted = vec![Instruction {
            result: None,
            kind: I::DropManaged { local: head.0 },
            span,
        }];
        if revive {
            let mut make = block
                .instructions
                .iter()
                .find(|i| matches!(i.kind, I::MakeManaged { .. }))
                .unwrap()
                .clone();
            let result = ValueId(function.values.len() as u32);
            function
                .values
                .push(function.locals[head.0.0 as usize].ty.clone());
            make.result = Some(result);
            inserted.push(make);
            inserted.push(Instruction {
                result: None,
                kind: I::Store {
                    local: head.0,
                    value: result,
                },
                span,
            });
        }
        block.instructions.splice(position..position, inserted);
        let mut altered_plan = plan.clone();
        while altered_plan.functions[main].holds.len() < forged.functions[main].values.len() {
            let value = ValueId(altered_plan.functions[main].holds.len() as u32);
            altered_plan.functions[main].holds.push(zil_ir::ValueHold {
                value,
                roots: vec![],
            });
        }
        reject_raw_and_wire(
            if revive {
                "old-head-after-revival"
            } else {
                "drop-with-live-head"
            },
            forged,
            altered_plan,
            "E-BORROW-CONFLICT",
        );
    }
}

#[test]
fn head_metadata_work_has_an_exact_finite_budget_boundary() {
    let (module, plan) = raw_head();
    let target = TargetProfile::host_llvm();
    let mut low = 0usize;
    let mut high = Limits::default().max_analysis_steps;
    while low + 1 < high {
        let middle = (low + high) / 2;
        let limits = Limits {
            max_analysis_steps: middle,
            ..Limits::default()
        };
        match zil_verify::verify_module(module.clone(), plan.clone(), &target, &limits) {
            Ok(_) => high = middle,
            Err(errors) => {
                assert!(
                    errors
                        .iter()
                        .all(|d| d.category == ResultCategory::ResourceLimit),
                    "{errors:?}"
                );
                low = middle;
            }
        }
    }
    let enough = Limits {
        max_analysis_steps: high,
        ..Limits::default()
    };
    zil_verify::verify_module(module.clone(), plan.clone(), &target, &enough).unwrap();
    let short = Limits {
        max_analysis_steps: high - 1,
        ..Limits::default()
    };
    let errors = zil_verify::verify_module(module, plan, &target, &short).unwrap_err();
    assert!(
        errors
            .iter()
            .all(|d| d.category == ResultCategory::ResourceLimit)
    );
    fs::write(
        directory().join("head-budget.txt"),
        format!("N={high}: Pass\nN-1={}: ResourceLimit\n", high - 1),
    )
    .unwrap();
}
