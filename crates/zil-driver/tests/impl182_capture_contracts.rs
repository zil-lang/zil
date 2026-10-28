//! Per-capture resource accesses: origins, current calls and independent raw/Wire checks.
use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    path::PathBuf,
    process::{Command, Stdio},
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_base::{FileId, Limits, ModuleId, Phase, ResultCategory, SourceUnit, TargetProfile};
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
        "mixed-capture182-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn check(source: &str) -> CompileReport {
    check_files(&[("main", source)])
}

fn check_files(files: &[(&str, &str)]) -> CompileReport {
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

fn native(files: &[(&str, &str)], stdout: &[u8], exit: i32) -> Vec<PathBuf> {
    native_input(files, b"", stdout, exit)
}

fn native_input(files: &[(&str, &str)], input: &[u8], stdout: &[u8], exit: i32) -> Vec<PathBuf> {
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
        let mut child = Command::new(&executable)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child.stdin.take().unwrap().write_all(input).unwrap();
        let output = child.wait_with_output().unwrap();
        fs::write(dir.join("stdout"), &output.stdout).unwrap();
        fs::write(dir.join("stderr"), &output.stderr).unwrap();
        fs::write(
            dir.join("exit.txt"),
            format!("{:?}\n", output.status.code()),
        )
        .unwrap();
        fs::write(dir.join("native.json"), format!("{{\"optimization\":\"{optimization:?}\",\"exit_code\":{},\"stdout\":{:?},\"stderr\":{:?}}}\n", output.status.code().map_or("null".into(), |code| code.to_string()), String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr))).unwrap();
        assert_eq!(output.status.code(), Some(exit), "{output:?}");
        assert_eq!(output.stdout, stdout);
        assert!(output.stderr.is_empty(), "{output:?}");
        dirs.push(dir);
    }
    dirs
}

fn candidate(source: &str) -> (zil_ir::TypedModule, zil_ir::CandidatePlan) {
    let source = SourceUnit::new(FileId(0), ModuleId(0), "raw182.zil", source);
    let syntax = zil_syntax::parse(&source).into_result().unwrap();
    let core = zil_expand::expand(&source, &syntax).unwrap();
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    let module = zil_types::check(&core, &target, &limits).unwrap();
    let plan = zil_analyze::analyze(&module, &limits).unwrap();
    zil_verify::verify_module(module.clone(), plan.clone(), &target, &limits).unwrap();
    (module, plan)
}

fn reject_candidate(
    label: &str,
    module: zil_ir::TypedModule,
    mut plan: zil_ir::CandidatePlan,
    code: &str,
    message: &str,
) {
    let target = TargetProfile::host_llvm();
    let limits = Limits::default();
    plan.module_fingerprint = module.fingerprint();
    let bytes = zil_ir::wire::encode_module_candidate(&module, &plan, &limits).unwrap();
    let dir = directory();
    fs::write(dir.join("label.txt"), label).unwrap();
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
        fs::write(dir.join(format!("{route}.txt")), format!("{errors:#?}\n")).unwrap();
        assert!(errors.iter().all(|d| d.phase == zil_base::Phase::Verify));
        assert!(
            errors
                .iter()
                .any(|d| d.code == code && d.message.contains(message) || d.code == message),
            "{label}/{route}: {errors:?}"
        );
    }
}

fn rejected(source: &str, category: ResultCategory, phase: Phase, message: &str) {
    let report = check(source);
    assert_eq!(report.status, category, "{}", report.to_json());
    assert_eq!(report.phase, phase, "{}", report.to_json());
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.message.contains(message) || d.code == message),
        "{}",
        report.to_json()
    );
}

const MODEL: &str = r#"
extern "C" fn putchar(x:i32)->i32;
resource R{code:i32;n:i32}
impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
fn write(d:R,n:i32)->i32{d.n=n;return n}
fn apply<F:Fn()->i32>(f:&F)->i32{return f()}
fn apply_mut<F:FnMut()->i32>(f:&mut F)->i32{return f()}
fn once<F:FnOnce()->i32>(f:F)->i32{return f()}
"#;

#[test]
fn original_probes_keep_inputs_and_remove_the_unrelated_head_switch() {
    for source in [
        include_str!(
            "../../../examples/mainline/mixed_capture182/probes/mixed_no_head_control.zil"
        ),
        include_str!(
            "../../../examples/mainline/mixed_capture182/probes/mixed_disjoint_actual_head.zil"
        ),
        include_str!(
            "../../../examples/mainline/mixed_capture182/probes/desc_read_current_head.zil"
        ),
        include_str!(
            "../../../examples/mainline/mixed_capture182/probes/hidden_desc_read_head.zil"
        ),
    ] {
        native(&[("main", source)], b"", 0);
    }
    rejected(
        include_str!(
            "../../../examples/mainline/mixed_capture182/probes/desc_write_current_head.zil"
        ),
        ResultCategory::SafeReject,
        Phase::Verify,
        "E-BORROW-CONFLICT",
    );
    rejected(
        include_str!(
            "../../../examples/mainline/mixed_capture182/probes/current_call_address_parameter.zil"
        ),
        ResultCategory::Unsupported,
        Phase::Types,
        "finite Copy type",
    );
    rejected(
        include_str!(
            "../../../examples/mainline/mixed_capture182/probes/helper_address_parameter.zil"
        ),
        ResultCategory::Unsupported,
        Phase::Expand,
        "declared bound",
    );
}

#[test]
fn same_origin_shared_desc_and_copy_address_read_on_the_final_call() {
    for invocation in ["f()", "apply(&f)"] {
        let source = format!(
            "{MODEL}fn main()->i32{{let r=R{{code:65,n:21}};let d=r;let p=&r;let q=&p.n;let f=()=>d.n+*q;return {invocation}-42}}"
        );
        native(&[("main", &source)], b"A", 0);
    }
}

#[test]
fn final_call_cannot_use_its_shared_capture_as_desc_write_permission() {
    for invocation in ["f()", "apply(&f)", "once(f)"] {
        let source = format!(
            "{MODEL}fn main()->i32{{let r=R{{code:65,n:21}};let d=r;let p=&r;let q=&p.n;let f=()=>{{write(d,42);return *q}};return {invocation}}}"
        );
        rejected(
            &source,
            ResultCategory::SafeReject,
            Phase::Verify,
            "E-BORROW-CONFLICT",
        );
    }
}

#[test]
fn each_desc_capture_keeps_its_own_read_or_write_origin() {
    for invocation in ["f()", "apply(&f)"] {
        let source = format!(
            "{MODEL}fn main()->i32{{let a=R{{code:65,n:1}};let b=R{{code:66,n:2}};let read=a;let edit=b;let p=&a;let q=&p.n;let f=()=>{{edit.n=40;return read.n+*q+edit.n}};return {invocation}-42}}"
        );
        native(&[("main", &source)], b"BA", 0);
        let alias = source.replace("let edit=b", "let edit=a");
        rejected(
            &alias,
            ResultCategory::SafeReject,
            Phase::Verify,
            "E-BORROW-CONFLICT",
        );
    }
}

#[test]
fn mutable_address_capture_reborrows_and_releases_after_last_call() {
    let source = format!(
        "{MODEL}fn main()->i32{{let r=R{{code:65,n:0}};let d=r;var n=0;let q=&mut n;var f=()=>{{d.n+=1;*q+=20;return *q+d.n}};let first=apply_mut(&mut f);let second=f();n+=1;return first+second+n-104}}"
    );
    native(&[("main", &source)], b"A", 0);
    let conflicting = format!(
        "{MODEL}fn main()->i32{{let r=R{{code:65,n:0}};let d=r;var n=0;let q=&mut n;var f=()=>{{d.n+=1;*q+=20;return *q}};n=1;return f()}}"
    );
    rejected(
        &conflicting,
        ResultCategory::SafeReject,
        Phase::Verify,
        "E-BORROW-CONFLICT",
    );
}

#[test]
fn copy_argument_preparation_still_observes_the_captured_shared_window() {
    let source = format!(
        "{MODEL}fn main()->i32{{let r=R{{code:65,n:21}};let p=&r;let q=&p.n;let f=(value:i32)=>*q+value;return f(write(r,42))}}"
    );
    rejected(
        &source,
        ResultCategory::SafeReject,
        Phase::Verify,
        "E-BORROW-CONFLICT",
    );
    let independent = format!(
        "{MODEL}fn main()->i32{{let r=R{{code:65,n:21}};let other=R{{code:66,n:0}};let p=&r;let q=&p.n;let f=(value:i32)=>*q+value;return f(write(other,21))-42}}"
    );
    native(&[("main", &independent)], b"BA", 0);
}

#[test]
fn current_explicit_arguments_do_not_end_before_a_direct_helper_returns() {
    for body in ["d.n=43;return p.n", "let old=p.n;d.n=43;return old"] {
        let source = format!(
            "{MODEL}fn helper(d:R,p:&R)->i32{{{body}}}fn main()->i32{{let r=R{{code:65,n:42}};return helper(r,&r)}}"
        );
        rejected(
            &source,
            ResultCategory::SafeReject,
            Phase::Verify,
            "E-BORROW-CONFLICT",
        );
    }
    let source = format!(
        "{MODEL}fn helper(d:R,p:&R)->i32{{return d.n+p.n}}fn main()->i32{{let r=R{{code:65,n:21}};return helper(r,&r)-42}}"
    );
    native(&[("main", &source)], b"A", 0);
}

#[test]
fn two_layers_of_desc_helpers_export_internal_callback_accesses() {
    let model = r#"extern "C" fn putchar(x:i32)->i32;pub resource R{code:i32;pub n:i32}impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}pub fn make(code:i32,n:i32)->R contract{result(create)}{return R{code:code,n:n}}"#;
    let helper = "import model;fn hidden(d:model.R)->i32{let f=()=>d.n;return f()}pub fn relay(d:model.R)->i32{return hidden(d)}";
    let consumer = "import model;import helper;fn main()->i32{let r=model.make(65,21);let p=&r;return helper.relay(r)+p.n-42}";
    native(
        &[("main", consumer), ("model", model), ("helper", helper)],
        b"A",
        0,
    );
    let writer = helper.replace("let f=()=>d.n", "let f=()=>{d.n=22;return d.n}");
    let report = check_files(&[("main", consumer), ("model", model), ("helper", &writer)]);
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
            .any(|d| d.code == "E-BORROW-CONFLICT")
    );
    let independent = consumer
        .replace("let p=&r", "let other=model.make(66,21);let p=&other")
        .replace("-42", "-43");
    native(
        &[
            ("main", &independent),
            ("model", model),
            ("helper", &writer),
        ],
        b"BA",
        0,
    );
}

#[test]
fn mixed_capture_contracts_substitute_actual_normal_function_parameters() {
    let source = format!(
        "{MODEL}fn mixed(d:R,p:&R)->i32{{let q=&p.n;let f=()=>{{write(d,42);return *q}};return apply(&f)}}fn main()->i32{{let a=R{{code:65,n:21}};let b=R{{code:66,n:0}};return mixed(b,&a)-21}}"
    );
    native(&[("main", &source)], b"BA", 0);
    let alias = source.replace("mixed(b,&a)", "mixed(a,&a)");
    rejected(
        &alias,
        ResultCategory::SafeReject,
        Phase::Verify,
        "E-BORROW-CONFLICT",
    );
}

#[test]
fn recursive_helper_effects_include_its_reachable_callback_prefix() {
    let source = format!(
        "{MODEL}fn relay(d:R,again:bool)->i32{{if again{{return relay(d,false)}};let f=()=>d.n;return f()}}fn main()->i32{{let r=R{{code:65,n:21}};let p=&r;return relay(r,true)+p.n-42}}"
    );
    native(&[("main", &source)], b"A", 0);
    let writer = source.replace("let f=()=>d.n", "let f=()=>{d.n=22;return d.n}");
    rejected(
        &writer,
        ResultCategory::SafeReject,
        Phase::Verify,
        "E-BORROW-CONFLICT",
    );
}

#[test]
fn conditional_and_looping_callback_effects_cover_every_reachable_path() {
    let source = format!(
        "{MODEL}fn hidden(d:R,flag:bool)->i32{{let f=()=>{{if flag{{d.n=22}};return d.n}};return f()}}fn main()->i32{{let r=R{{code:65,n:21}};let p=&r;return hidden(r,false)+p.n}}"
    );
    rejected(
        &source,
        ResultCategory::SafeReject,
        Phase::Verify,
        "E-BORROW-CONFLICT",
    );
    let source = format!(
        "{MODEL}fn main()->i32{{let r=R{{code:65,n:7}};let d=r;let p=&r;let q=&p.n;let f=()=>d.n+*q;var total=0;for i in 0..3{{total+=apply(&f)}};return total-42}}"
    );
    native(&[("main", &source)], b"A", 0);
}

#[test]
fn owning_once_cleanup_and_error_exit_keep_other_capture_dependencies() {
    let source = format!(
        "{MODEL}fn main()->i32{{let r=R{{code:65,n:1}};let d=r;let token=R{{code:66,n:1}};let n=40;let q=&n;let f=move()=>{{d.n=1;return token.n+d.n+*q}};return once(f)-42}}"
    );
    native(&[("main", &source)], b"BA", 0);
    let source = format!(
        "{MODEL}enum E{{Fail}}fn main()->E!i32{{let r=R{{code:65,n:1}};let d=r;let token=R{{code:66,n:1}};let n=40;let q=&n;let f=move()=>{{d.n=1;return token.n+d.n+*q}};let value=once(f);if value==42{{return E.Fail}};return value}}"
    );
    native(&[("main", &source)], b"BA", 1);
}

#[test]
fn unused_cleanup_does_not_turn_a_desc_into_a_strong_hold() {
    let source = format!(
        "{MODEL}fn consume(give r:R){{}}fn main()->i32{{let r=R{{code:65,n:1}};let d=r;let token=R{{code:66,n:1}};let n=40;let q=&n;let f=move()=>token.n+d.n+*q;consume(give r);return 0}}"
    );
    native(&[("main", &source)], b"AB", 0);
    let late = source.replace("consume(give r);return 0", "consume(give r);return f()");
    let report = check(&late);
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
            .any(|d| d.message.contains("description") || d.message.contains("resource")),
        "{}",
        report.to_json()
    );
}

#[test]
fn callback_error_and_late_argument_failure_cleanup_once() {
    let source = format!(
        "{MODEL}enum E{{Fail}}fn invoke<F:Fn()->E!i32>(f:&F)->E!i32{{return f()?}}fn main()->E!i32{{let r=R{{code:65,n:1}};let d=r;let token=R{{code:66,n:1}};let n=40;let q=&n;let f=move()->E!i32=>{{d.n=token.n+*q;return E.Fail}};return invoke(&f)?}}"
    );
    native(&[("main", &source)], b"BA", 1);
    let source = format!(
        "{MODEL}enum E{{Fail}}fn fail()->E!i32=>E.Fail;fn invoke<F:FnOnce()->i32>(f:F,n:i32)->i32{{return f()+n}}fn main()->E!i32{{let r=R{{code:65,n:1}};let d=r;let token=R{{code:66,n:1}};let n=40;let q=&n;let f=move()=>{{unsafe{{putchar(88)}};d.n=1;return token.n+d.n+*q}};return invoke(f,fail()?);}}"
    );
    native(&[("main", &source)], b"BA", 1);
}

#[test]
fn zero_call_contract_does_not_execute_a_captured_write() {
    let source = format!(
        "{MODEL}fn discard<F:FnOnce()->i32>(f:F)->i32{{return 0}}fn main()->i32{{let r=R{{code:65,n:21}};let d=r;let token=R{{code:66,n:1}};let p=&r;let q=&p.n;let f=move()=>{{d.n=token.n;return *q}};let n=discard(f);return n+r.n-21}}"
    );
    native(&[("main", &source)], b"BA", 0);
}

#[test]
fn raw_and_wire_preserve_capture_indices_hold_and_public_upper_bounds() {
    let source = format!(
        "{MODEL}fn main()->i32{{let a=R{{code:65,n:1}};let b=R{{code:66,n:2}};let read=a;let edit=b;let p=&a;let q=&p.n;let f=()=>{{edit.n=40;return read.n+*q+edit.n}};return apply(&f)-42}}"
    );
    let (original, original_plan) = candidate(&source);
    let closure = original
        .closures
        .iter()
        .find(|c| {
            c.captures
                .iter()
                .filter(|t| matches!(t, zil_ir::Type::Desc(_)))
                .count()
                == 2
        })
        .unwrap();
    let id = closure.id;
    let function = &original.functions[closure.function.0 as usize];
    let write_desc = function
        .blocks
        .iter()
        .flat_map(|b| &b.instructions)
        .find_map(|i| match i.kind {
            zil_ir::InstructionKind::WriteDesc { desc, .. } => Some(desc),
            _ => None,
        })
        .unwrap();
    let write_local = function
        .blocks
        .iter()
        .flat_map(|b| &b.instructions)
        .find_map(|i| (i.result == Some(write_desc)).then_some(&i.kind))
        .and_then(|k| match k {
            zil_ir::InstructionKind::Load(local) => Some(*local),
            _ => None,
        })
        .unwrap();
    let captured_value = function
        .blocks
        .iter()
        .flat_map(|b| &b.instructions)
        .find_map(|i| match i.kind {
            zil_ir::InstructionKind::Store { local, value } if local == write_local => Some(value),
            _ => None,
        })
        .unwrap();
    let write_index = function
        .blocks
        .iter()
        .flat_map(|b| &b.instructions)
        .find_map(|i| match i.kind {
            zil_ir::InstructionKind::ReadCapture { index } if i.result == Some(captured_value) => {
                Some(index)
            }
            _ => None,
        })
        .unwrap();
    let read_index = closure
        .captures
        .iter()
        .enumerate()
        .find_map(|(i, t)| {
            (matches!(t, zil_ir::Type::Desc(_)) && i != write_index as usize).then_some(i)
        })
        .unwrap();
    let mut module = original.clone();
    for instruction in module
        .functions
        .iter_mut()
        .flat_map(|f| &mut f.blocks)
        .flat_map(|b| &mut b.instructions)
    {
        if let zil_ir::InstructionKind::MakeClosure { closure, captures } = &mut instruction.kind
            && *closure == id
        {
            captures[write_index as usize] = captures[read_index];
        }
    }
    let plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    reject_candidate(
        "actual-capture-alias",
        module,
        plan,
        "E-BORROW-CONFLICT",
        "access overlaps",
    );

    let mut module = original.clone();
    for function in &mut module.functions {
        if let Some(contract) = &mut function.callable_contract {
            contract.writes.clear();
        }
    }
    reject_candidate(
        "erase-public-write-upper-bound",
        module,
        original_plan.clone(),
        "E-VERIFY-CALLABLE-CONTRACT",
        "contract",
    );

    let mut plan = original_plan.clone();
    let main = original
        .functions
        .iter()
        .find(|f| f.name == "main")
        .unwrap();
    let value = main
        .blocks
        .iter()
        .flat_map(|b| &b.instructions)
        .find_map(|i| {
            matches!(i.kind, zil_ir::InstructionKind::MakeClosure { .. })
                .then_some(i.result)
                .flatten()
        })
        .unwrap();
    plan.functions[main.id.0 as usize].holds[value.0 as usize]
        .roots
        .clear();
    reject_candidate(
        "erase-capture-hold",
        original.clone(),
        plan,
        "E-VERIFY-HOLD",
        "provenance",
    );

    let mut module = original.clone();
    for function in &mut module.functions {
        if let Some(contract) = &mut function.callable_contract {
            for trace in &mut contract.traces {
                trace.calls.clear();
            }
        }
    }
    reject_candidate(
        "forge-zero-call",
        module,
        original_plan,
        "E-VERIFY-CALLABLE-CONTRACT",
        "contract",
    );
}

#[test]
fn raw_and_wire_reconstruct_new_desc_writes_in_the_actual_entry() {
    let source = format!(
        "{MODEL}fn main()->i32{{let r=R{{code:65,n:21}};let d=r;let p=&r;let q=&p.n;let f=()=>{{let marker=7;return d.n+*q+marker}};return apply(&f)-49}}"
    );
    let (mut module, plan) = candidate(&source);
    let entry = module
        .closures
        .iter()
        .find(|c| {
            c.captures
                .iter()
                .any(|t| matches!(t, zil_ir::Type::Desc(_)))
        })
        .unwrap()
        .function;
    let function = &mut module.functions[entry.0 as usize];
    let block = function
        .blocks
        .iter_mut()
        .find(|b| {
            b.instructions
                .iter()
                .any(|i| matches!(i.kind, zil_ir::InstructionKind::ReadDesc { .. }))
        })
        .unwrap();
    let value = block
        .instructions
        .iter()
        .find_map(|i| match i.kind {
            zil_ir::InstructionKind::Constant(zil_ir::Constant::I32(7)) => i.result,
            _ => None,
        })
        .unwrap();
    let position = block
        .instructions
        .iter()
        .position(|i| matches!(i.kind, zil_ir::InstructionKind::ReadDesc { .. }))
        .unwrap();
    let zil_ir::InstructionKind::ReadDesc { desc, index } = block.instructions[position].kind
    else {
        unreachable!()
    };
    block.instructions.insert(
        position,
        zil_ir::Instruction {
            result: None,
            kind: zil_ir::InstructionKind::WriteDesc { desc, index, value },
            span: block.instructions[position].span,
        },
    );
    reject_candidate(
        "entry-write-not-public-capability",
        module,
        plan,
        "E-BORROW-CONFLICT",
        "access overlaps",
    );
}

#[test]
fn runtime_mixed_capture_consumer_uses_four_actual_modules() {
    let files = [
        (
            "main",
            include_str!("../../../examples/mainline/mixed_capture182/main.zil"),
        ),
        (
            "model",
            include_str!("../../../examples/mainline/mixed_capture182/model.zil"),
        ),
        (
            "operations",
            include_str!("../../../examples/mainline/mixed_capture182/operations.zil"),
        ),
        (
            "visits",
            include_str!("../../../examples/mainline/mixed_capture182/visits.zil"),
        ),
    ];
    for (input, output, exit) in [
        (b"0".as_slice(), b"BA".as_slice(), 0),
        (b"1", b"1BA", 0),
        (b"3", b"123BA", 0),
        (b"9", b"", 9),
    ] {
        native_input(&files, input, output, exit);
    }
}

#[test]
fn per_capture_verification_work_has_an_actual_bounded_failure() {
    let source = format!(
        "{MODEL}fn main()->i32{{let a=R{{code:65,n:1}};let b=R{{code:66,n:2}};let read=a;let edit=b;let p=&a;let q=&p.n;let f=()=>{{edit.n=40;return read.n+*q+edit.n}};return apply(&f)-42}}"
    );
    let (module, plan) = candidate(&source);
    let target = TargetProfile::host_llvm();
    let bytes = zil_ir::wire::encode_module_candidate(&module, &plan, &Limits::default()).unwrap();
    let dir = directory();
    fs::write(dir.join("main.zil"), &source).unwrap();
    fs::write(dir.join("candidate.bin"), &bytes).unwrap();
    for wire in [false, true] {
        let run = |work| {
            let limits = Limits {
                max_analysis_steps: work,
                ..Limits::default()
            };
            if wire {
                zil_verify::verify_module_bytes(&bytes, &target, &limits)
            } else {
                zil_verify::verify_module(module.clone(), plan.clone(), &target, &limits)
            }
        };
        let (mut lower, mut upper) = (0, Limits::default().max_analysis_steps);
        assert!(run(upper).is_ok());
        while lower + 1 < upper {
            let mid = lower + (upper - lower) / 2;
            match run(mid) {
                Ok(_) => upper = mid,
                Err(errors) => {
                    assert!(
                        errors
                            .iter()
                            .all(|d| d.category == ResultCategory::ResourceLimit),
                        "{errors:?}"
                    );
                    lower = mid;
                }
            }
        }
        assert!(run(upper).is_ok());
        let errors = run(lower).unwrap_err();
        assert!(
            errors
                .iter()
                .all(|d| d.category == ResultCategory::ResourceLimit)
        );
        fs::write(
            dir.join(if wire {
                "wire-budget.txt"
            } else {
                "raw-budget.txt"
            }),
            format!("success={upper}\nResourceLimit={lower}\n{errors:#?}\n"),
        )
        .unwrap();
    }
}
