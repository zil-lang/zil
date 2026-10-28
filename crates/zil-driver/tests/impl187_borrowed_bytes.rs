//! Local ByteBuffer Desc captures: actual windows, lifetime and independent raw/Wire checks.
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
        "borrowed-bytes187-{}-{}",
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
        assert_eq!(report.source_files.len(), files.len());
        let mut child = Command::new(&executable)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child.stdin.take().unwrap().write_all(input).unwrap();
        let output = child.wait_with_output().unwrap();
        fs::write(dir.join("stdin"), input).unwrap();
        fs::write(
            dir.join("native-command.txt"),
            format!("{}\n", executable.display()),
        )
        .unwrap();
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
    let source = SourceUnit::new(FileId(0), ModuleId(0), "raw187.zil", source);
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
        let category = if code == "E-UNSUPPORTED" {
            ResultCategory::Unsupported
        } else {
            ResultCategory::SafeReject
        };
        assert!(
            errors.iter().all(|d| d.category == category),
            "{label}/{route}: {errors:?}"
        );
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

const SUPPORT: &str = r#"
fn call<F:Fn()->i32>(f:&F)->i32=>f();
fn call_mut<F:FnMut()->i32>(f:&mut F)->i32=>f();
fn consume(give b:ByteBuffer){}
"#;

#[test]
fn original_local_byte_description_capture_reaches_native_execution() {
    for source in [
        "fn main()->i32{let b=bytes_new();let f=()=>i32(bytes_len(b));return f()}",
        "fn main()->i32{let b=bytes_new();let d=b;let f=move()=>i32(bytes_len(d));return f()}",
    ] {
        native(&[("main", source)], b"", 0);
    }
}

#[test]
fn actual_readonly_capture_composes_with_a_live_shared_byte_view() {
    for invocation in ["f()", "call(&f)"] {
        for (read, expected) in [
            ("i32(bytes_len(d)+bytes_capacity(d))", 67),
            (
                "i32(bytes_len(d)+bytes_capacity(d))+i32(bytes_view(d)[0])",
                132,
            ),
        ] {
            let source = format!(
                "{SUPPORT}fn main()->i32{{let b=bytes_new();if !bytes_reserve(b,1){{return 7}};bytes_push(b,u8(65));let d=b;let view=bytes_view(b);let f=()=>{read};return {invocation}+i32(view[0])-{expected}}}"
            );
            native(&[("main", &source)], b"", 0);
        }
    }
}

#[test]
fn every_actual_storage_write_conflicts_with_a_live_same_source_view() {
    for operation in [
        "bytes_reserve(d,1)",
        "bytes_push(d,u8(66))",
        "bytes_truncate(d,0)",
        "bytes_view_mut(d)",
    ] {
        for invocation in ["f()", "call(&f)"] {
            let source = format!(
                "{SUPPORT}fn main()->i32{{let b=bytes_new();bytes_reserve(b,1);bytes_push(b,u8(65));let d=b;let view=bytes_view(b);let f=()=>{{{operation};return 0}};let answer={invocation};return answer+i32(view[0])-65}}"
            );
            rejected(
                &source,
                ResultCategory::SafeReject,
                Phase::Verify,
                "E-BORROW-CONFLICT",
            );
        }
    }
    for operation in ["bytes_len(d)", "bytes_capacity(d)", "bytes_view(d)"] {
        let source = format!(
            "{SUPPORT}fn main()->i32{{let b=bytes_new();bytes_reserve(b,1);bytes_push(b,u8(65));let d=b;let view=bytes_view_mut(b);let f=()=>{{{operation};return 0}};let answer=call(&f);return answer+i32(view[0])-65}}"
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
fn independent_buffer_sources_do_not_inherit_an_unrelated_window() {
    let source = format!(
        "{SUPPORT}fn main()->i32{{let a=bytes_new();let b=bytes_new();bytes_reserve(a,1);bytes_push(a,u8(65));bytes_reserve(b,1);let d=b;let view=bytes_view(a);let f=()=>{{bytes_push(d,u8(66));return i32(bytes_len(d))}};return call(&f)+i32(view[0])-66}}"
    );
    native(&[("main", &source)], b"", 0);
    rejected(
        &source.replace("let d=b", "let d=a"),
        ResultCategory::SafeReject,
        Phase::Verify,
        "E-BORROW-CONFLICT",
    );
}

#[test]
fn helper_argument_views_are_checked_with_the_actual_capture_effects() {
    let source = "fn inspect<F:Fn()->i32>(f:&F,b:ByteBuffer)->i32{let view=bytes_view(b);let answer=f();return answer+i32(view[0])}fn main()->i32{let a=bytes_new();let b=bytes_new();bytes_reserve(a,1);bytes_push(a,u8(65));bytes_reserve(b,2);bytes_push(b,u8(66));let f=()=>{bytes_push(b,u8(67));return i32(bytes_len(b))};return inspect(&f,a)-67}";
    native(&[("main", source)], b"", 0);
    rejected(
        &source.replace("inspect(&f,a)", "inspect(&f,b)"),
        ResultCategory::SafeReject,
        Phase::Verify,
        "E-BORROW-CONFLICT",
    );
    let read = source
        .replace("bytes_push(b,u8(67));return", "return")
        .replace("inspect(&f,a)", "inspect(&f,b)");
    native(&[("main", &read)], b"", 0);
}

#[test]
fn internal_capture_windows_propagate_separation_to_each_actual_call() {
    for invocation in ["f()", "call(&f)"] {
        for operation in [
            "bytes_reserve(edit,2)",
            "bytes_push(edit,u8(66))",
            "bytes_truncate(edit,0)",
            "bytes_view_mut(edit)",
        ] {
            let source = format!(
                "{SUPPORT}fn main()->i32{{let a=bytes_new();let b=bytes_new();bytes_reserve(a,1);bytes_push(a,u8(65));bytes_reserve(b,2);let read=a;let edit=b;let f=()=>{{let view=bytes_view(read);{operation};return i32(view[0])}};return {invocation}-65}}"
            );
            native(&[("main", &source)], b"", 0);
            rejected(
                &source.replace("let edit=b", "let edit=a"),
                ResultCategory::SafeReject,
                Phase::Verify,
                "E-BORROW-CONFLICT",
            );
        }
    }
    let exclusive = format!(
        "{SUPPORT}fn main()->i32{{let a=bytes_new();let b=bytes_new();bytes_reserve(a,1);bytes_push(a,u8(65));let read=a;let other=b;let f=()=>{{let view=bytes_view_mut(read);bytes_len(other);return i32(view[0])}};return call(&f)-65}}"
    );
    native(&[("main", &exclusive)], b"", 0);
    rejected(
        &exclusive.replace("let other=b", "let other=a"),
        ResultCategory::SafeReject,
        Phase::Verify,
        "E-BORROW-CONFLICT",
    );
    let owned = "fn main()->i32{let a=bytes_new();let b=bytes_new();bytes_reserve(a,1);bytes_push(a,u8(65));var f=move()=>{let view=bytes_view(a);bytes_reserve(b,2);return i32(view[0])};return f()+f()-130}";
    native(&[("main", owned)], b"", 0);
}

#[test]
fn helper_body_accesses_do_not_disappear_behind_its_callback_contract() {
    for (body, answer) in [
        ("bytes_reserve(b,4);return f()", 66),
        ("bytes_reserve(b,4);return 0", 65),
    ] {
        let source = format!(
            "fn touch<F:Fn()->i32>(f:&F,b:ByteBuffer)->i32{{{body}}}fn main()->i32{{let a=bytes_new();let b=bytes_new();bytes_reserve(a,1);bytes_push(a,u8(65));let view=bytes_view(a);let f=()=>i32(bytes_len(a));return touch(&f,b)+i32(view[0])-{answer}}}"
        );
        native(&[("main", &source)], b"", 0);
        rejected(
            &source.replace("touch(&f,b)", "touch(&f,a)"),
            ResultCategory::SafeReject,
            Phase::Verify,
            "E-BORROW-CONFLICT",
        );
    }
}

#[test]
fn internal_capture_obligations_cross_two_ordinary_description_helpers() {
    let source = "fn inner(a:ByteBuffer,b:ByteBuffer)->i32{let f=()=>{let view=bytes_view(a);bytes_reserve(b,2);return i32(view[0])};return f()}fn outer(a:ByteBuffer,b:ByteBuffer)->i32{return inner(a,b)}fn main()->i32{let a=bytes_new();let b=bytes_new();bytes_reserve(a,1);bytes_push(a,u8(65));return outer(a,b)-65}";
    native(&[("main", source)], b"", 0);
    rejected(
        &source.replace("outer(a,b)-65", "outer(a,a)-65"),
        ResultCategory::SafeReject,
        Phase::Verify,
        "E-BORROW-CONFLICT",
    );
}

#[test]
fn ordinary_helper_layers_preserve_readonly_and_hidden_clear_effects() {
    let source = format!(
        "{SUPPORT}fn read(b:ByteBuffer)->i32=>i32(bytes_len(b));fn forward(b:ByteBuffer)->i32=>read(b);fn clear(b:ByteBuffer)->bool=>bytes_truncate(b,0);fn clear_again(b:ByteBuffer)->bool=>clear(b);fn main()->i32{{let b=bytes_new();bytes_reserve(b,1);bytes_push(b,u8(65));let d=b;let view=bytes_view(b);let f=()=>forward(d);return call(&f)+i32(view[0])-66}}"
    );
    native(&[("main", &source)], b"", 0);
    rejected(
        &source.replace(
            "fn read(b:ByteBuffer)->i32=>i32(bytes_len(b))",
            "fn read(b:ByteBuffer)->i32{clear_again(b);return 1}",
        ),
        ResultCategory::SafeReject,
        Phase::Verify,
        "E-BORROW-CONFLICT",
    );
}

#[test]
fn mutable_environment_position_and_last_call_leave_separate_byte_permissions() {
    let source = format!(
        "{SUPPORT}fn main()->i32{{let b=bytes_new();bytes_reserve(b,1);bytes_push(b,u8(65));let d=b;let view=bytes_view(b);var n=0;var f=()=>{{n+=1;return i32(bytes_len(d))+n}};let one=call_mut(&mut f);let two=f();let byte=i32(view[0]);if !bytes_reserve(b,4){{return 7}};bytes_truncate(b,0);n+=1;return one+two+byte+n-73}}"
    );
    native(&[("main", &source)], b"", 0);
}

#[test]
fn unused_capture_does_not_own_or_extend_a_buffer_after_transfer() {
    for use_before in ["", "f();"] {
        let source = format!(
            "{SUPPORT}fn main()->i32{{let b=bytes_new();bytes_reserve(b,1);bytes_push(b,u8(65));let f=()=>i32(bytes_len(b));{use_before}consume(give b);return 0}}"
        );
        native(&[("main", &source)], b"", 0);
        rejected(
            &source.replace("consume(give b);return 0", "consume(give b);return f()"),
            ResultCategory::SafeReject,
            Phase::Analyze,
            "E-ANALYZE-IR",
        );
    }
}

#[test]
fn borrowed_slices_and_retained_escape_stay_closed() {
    rejected(
        "fn main()->i32{let b=bytes_new();bytes_reserve(b,1);bytes_push(b,u8(65));let view=bytes_view(b);let f=()=>i32(view[0]);return f()-65}",
        ResultCategory::Unsupported,
        Phase::Types,
        "E-UNSUPPORTED",
    );
    rejected(
        "fn escape()->impl Fn()->i32{let b=bytes_new();return ()=>i32(bytes_len(b))}fn main(){}",
        ResultCategory::Unsupported,
        Phase::Types,
        "E-UNSUPPORTED",
    );
    rejected(
        "resource R{n:i32}fn inspect<F:Fn()->i32>(f:&F,r:R)->i32{return f()+r.n}fn main()->i32{let a=R{n:21};let f=()=>a.n;return inspect(&f,a)-42}",
        ResultCategory::Unsupported,
        Phase::Types,
        "E-UNSUPPORTED",
    );
}

#[test]
fn an_explicit_shared_parameter_and_local_byte_capture_have_a_verified_signature() {
    // This is the original187 input, unchanged. Slice189 supplies its admitted
    // signature; the local ByteBuffer description remains nonescaping.
    let report = check("fn main(){let b=bytes_new();let f=(n:&i32)=>i32(bytes_len(b))+*n}");
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.phase, Phase::Verify, "{}", report.to_json());
    assert_eq!(report.verification_runs, 1);
}

#[test]
fn raw_and_wire_reconstruct_internal_capture_and_parameter_window_obligations() {
    use zil_ir::InstructionKind as I;
    let source = format!(
        "{SUPPORT}fn main()->i32{{let a=bytes_new();let b=bytes_new();bytes_reserve(a,1);bytes_push(a,u8(65));let read=a;let edit=b;let f=()=>{{let view=bytes_view(read);bytes_reserve(edit,2);return i32(view[0])}};return call(&f)-65}}"
    );
    let (mut module, plan) = candidate(&source);
    for instruction in module
        .functions
        .iter_mut()
        .flat_map(|f| &mut f.blocks)
        .flat_map(|b| &mut b.instructions)
    {
        if let I::MakeClosure { captures, .. } = &mut instruction.kind {
            assert_eq!(captures.len(), 2);
            captures[1] = captures[0];
        }
    }
    reject_candidate(
        "internal-window-capture-capture-alias",
        module,
        plan,
        "E-BORROW-CONFLICT",
        "window contract requires separation",
    );

    let source = "fn inspect<F:Fn()->i32>(f:&F,b:ByteBuffer)->i32{let view=bytes_view(b);let answer=f();return answer+i32(view[0])}fn main()->i32{let a=bytes_new();let b=bytes_new();bytes_reserve(a,1);bytes_push(a,u8(65));bytes_reserve(b,2);bytes_push(b,u8(66));let f=()=>{bytes_push(b,u8(67));return i32(bytes_len(b))};return inspect(&f,a)-67}";
    let (mut module, plan) = candidate(source);
    let helper = module
        .functions
        .iter()
        .find(|f| f.callable_contract.is_some())
        .unwrap()
        .id;
    let function = module
        .functions
        .iter_mut()
        .find(|f| f.name == "main")
        .unwrap();
    let source = function
        .blocks
        .iter()
        .flat_map(|b| &b.instructions)
        .find_map(|i| match &i.kind {
            I::MakeClosure { captures, .. } => Some(captures[0]),
            _ => None,
        })
        .unwrap();
    for instruction in function.blocks.iter_mut().flat_map(|b| &mut b.instructions) {
        if let I::Call {
            function,
            arguments,
        } = &mut instruction.kind
            && *function == helper
        {
            assert_eq!(arguments.len(), 2);
            arguments[1] = source;
        }
    }
    reject_candidate(
        "internal-window-capture-argument-alias",
        module,
        plan,
        "E-BORROW-CONFLICT",
        "window contract requires separation",
    );
}

#[test]
fn internal_window_separation_work_has_a_bounded_raw_and_wire_limit() {
    let source = format!(
        "{SUPPORT}fn main()->i32{{let a=bytes_new();let b=bytes_new();bytes_reserve(a,1);bytes_push(a,u8(65));let f=()=>{{let view=bytes_view(a);bytes_reserve(b,2);return i32(view[0])}};return call(&f)-65}}"
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
            let middle = lower + (upper - lower) / 2;
            match run(middle) {
                Ok(_) => upper = middle,
                Err(errors) => {
                    assert!(
                        errors
                            .iter()
                            .all(|d| d.category == ResultCategory::ResourceLimit),
                        "{errors:?}"
                    );
                    lower = middle;
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

#[test]
fn raw_and_wire_reconstruct_an_inserted_storage_write_without_analyze() {
    use zil_ir::{Constant, InstructionKind as I, Type};
    let source = format!(
        "{SUPPORT}fn main()->i32{{let b=bytes_new();bytes_reserve(b,1);bytes_push(b,u8(65));let d=b;let view=bytes_view(b);let f=()=>{{let capacity:i64=1;bytes_len(d);return i32(capacity)-1}};return call(&f)+i32(view[0])-65}}"
    );
    let (mut module, plan) = candidate(&source);
    let entry = module.closures[0].function;
    let function = &mut module.functions[entry.0 as usize];
    let block = function
        .blocks
        .iter_mut()
        .find(|b| {
            b.instructions
                .iter()
                .any(|i| matches!(i.kind, I::ByteBufferLen { .. }))
        })
        .unwrap();
    let capacity = block
        .instructions
        .iter()
        .find_map(|i| match i.kind {
            I::Constant(Constant::I64(1)) => i.result,
            _ => None,
        })
        .unwrap();
    let instruction = block
        .instructions
        .iter_mut()
        .find(|i| matches!(i.kind, I::ByteBufferLen { .. }))
        .unwrap();
    let I::ByteBufferLen { buffer } = instruction.kind else {
        unreachable!()
    };
    instruction.kind = I::ByteBufferReserve { buffer, capacity };
    function.values[instruction.result.unwrap().0 as usize] = Type::Bool;
    reject_candidate(
        "hidden-reserve-with-original-plan",
        module,
        plan,
        "E-BORROW-CONFLICT",
        "access overlaps",
    );
}

#[test]
fn raw_and_wire_reconstruct_the_actual_capture_source_not_just_its_type() {
    use zil_ir::InstructionKind as I;
    let source = format!(
        "{SUPPORT}fn main()->i32{{let a=bytes_new();let b=bytes_new();bytes_reserve(a,1);bytes_push(a,u8(65));bytes_reserve(b,1);bytes_len(a);let view=bytes_view(a);let f=()=>{{bytes_push(b,u8(66));return 1}};return call(&f)+i32(view[0])-66}}"
    );
    let (mut module, plan) = candidate(&source);
    let function = module
        .functions
        .iter_mut()
        .find(|f| f.name == "main")
        .unwrap();
    let source = function
        .blocks
        .iter()
        .flat_map(|b| &b.instructions)
        .find_map(|i| match i.kind {
            I::ByteBufferLen { buffer } => Some(buffer),
            _ => None,
        })
        .unwrap();
    let captures = function
        .blocks
        .iter_mut()
        .flat_map(|b| &mut b.instructions)
        .find_map(|i| match &mut i.kind {
            I::MakeClosure { captures, .. } => Some(captures),
            _ => None,
        })
        .unwrap();
    assert_eq!(captures.len(), 1);
    captures[0] = source;
    reject_candidate(
        "retarget-buffer-to-live-view-source",
        module,
        plan,
        "E-BORROW-CONFLICT",
        "access overlaps",
    );
}

#[test]
fn raw_and_wire_cannot_capture_a_description_after_its_owner_was_given() {
    use zil_ir::InstructionKind as I;
    let source = format!(
        "{SUPPORT}fn main()->i32{{let a=bytes_new();let b=bytes_new();let old=b;bytes_len(old);consume(give b);let f=()=>i32(bytes_len(a));return f()}}"
    );
    let (mut module, plan) = candidate(&source);
    let function = module
        .functions
        .iter_mut()
        .find(|f| f.name == "main")
        .unwrap();
    let old = function
        .blocks
        .iter()
        .flat_map(|b| &b.instructions)
        .find_map(|i| match i.kind {
            I::ByteBufferLen { buffer } => Some(buffer),
            _ => None,
        })
        .unwrap();
    let captures = function
        .blocks
        .iter_mut()
        .flat_map(|b| &mut b.instructions)
        .find_map(|i| match &mut i.kind {
            I::MakeClosure { captures, .. } => Some(captures),
            _ => None,
        })
        .unwrap();
    assert_eq!(captures.len(), 1);
    captures[0] = old;
    reject_candidate(
        "capture-already-given-buffer-instance",
        module,
        plan,
        "E-VERIFY-RESOURCE",
        "description",
    );
}

#[test]
fn raw_and_wire_do_not_promote_weak_buffer_captures_or_erase_real_view_holds() {
    use zil_ir::{HoldRoot, InstructionKind as I, LocalId};
    let source = format!(
        "{SUPPORT}fn main()->i32{{let b=bytes_new();bytes_reserve(b,1);bytes_push(b,u8(65));let view=bytes_view(b);let f=()=>i32(bytes_len(b));return call(&f)+i32(view[0])-66}}"
    );
    let (module, plan) = candidate(&source);
    let function = module.functions.iter().find(|f| f.name == "main").unwrap();
    let closure = function
        .blocks
        .iter()
        .flat_map(|b| &b.instructions)
        .find_map(|i| {
            matches!(i.kind, I::MakeClosure { .. })
                .then_some(i.result)
                .flatten()
        })
        .unwrap();
    let view = function
        .blocks
        .iter()
        .flat_map(|b| &b.instructions)
        .find_map(|i| {
            matches!(i.kind, I::ByteBufferBorrow { .. })
                .then_some(i.result)
                .flatten()
        })
        .unwrap();
    assert!(
        plan.functions[function.id.0 as usize].holds[closure.0 as usize]
            .roots
            .is_empty()
    );
    assert!(
        !plan.functions[function.id.0 as usize].holds[view.0 as usize]
            .roots
            .is_empty()
    );
    let mut fake = plan.clone();
    fake.functions[function.id.0 as usize].holds[closure.0 as usize].roots =
        vec![HoldRoot::Local(LocalId(0))];
    reject_candidate(
        "weak-buffer-environment-fabricated-strong-hold",
        module.clone(),
        fake,
        "E-VERIFY-HOLD",
        "independently reconstructed provenance",
    );
    let mut empty = plan;
    empty.functions[function.id.0 as usize].holds[view.0 as usize]
        .roots
        .clear();
    reject_candidate(
        "real-buffer-view-erased-hold",
        module,
        empty,
        "E-VERIFY-HOLD",
        "independently reconstructed provenance",
    );
}

#[test]
fn raw_and_wire_cannot_forge_a_self_contained_factory_return() {
    use zil_ir::{InstructionKind as I, Terminator, Type};
    let source = "fn owned()->impl Fn()->i32{return ()=>0}fn local()->i32{let b=bytes_new();let f=()=>i32(bytes_len(b));return f()}fn main()->i32=>local();";
    let (mut module, plan) = candidate(source);
    let mut contract = module
        .functions
        .iter()
        .find(|f| f.name == "owned")
        .unwrap()
        .callable_return_contract
        .clone()
        .unwrap();
    let borrowed = module
        .closures
        .iter()
        .find(|c| {
            c.captures
                .iter()
                .any(|t| matches!(t, Type::Desc(inner) if **inner == Type::ByteBuffer))
        })
        .unwrap();
    let id = borrowed.id;
    contract.closure = id;
    contract.kind = borrowed.kind;
    contract.cleanup.clear();
    let function = module
        .functions
        .iter_mut()
        .find(|f| f.name == "local")
        .unwrap();
    function.return_type = Type::Closure(id);
    function.callable_return_contract = Some(contract);
    let value = function
        .blocks
        .iter()
        .flat_map(|b| &b.instructions)
        .find_map(|i| {
            matches!(i.kind, I::MakeClosure { closure, .. } if closure == id)
                .then_some(i.result)
                .flatten()
        })
        .unwrap();
    let block = function
        .blocks
        .iter_mut()
        .find(|b| matches!(b.terminator, Terminator::Return(_)))
        .unwrap();
    block.terminator = Terminator::Return(Some(value));
    reject_candidate(
        "borrowed-buffer-forged-factory-escape",
        module,
        plan,
        "E-UNSUPPORTED",
        "returned callable captures must be self-contained",
    );
}

#[test]
fn a_zero_call_helper_does_not_execute_its_buffer_capture_effects() {
    let source = "fn skip<F:Fn()->i32>(f:&F)->i32{return 0}fn main()->i32{let b=bytes_new();bytes_reserve(b,1);bytes_push(b,u8(65));let view=bytes_view(b);let f=()=>{bytes_truncate(b,0);return 0};return skip(&f)+i32(view[0])-65}";
    native(&[("main", source)], b"", 0);
    rejected(
        &source.replace("skip(&f)", "f()"),
        ResultCategory::SafeReject,
        Phase::Verify,
        "E-BORROW-CONFLICT",
    );
}

#[test]
fn live_buffer_windows_still_block_give_with_an_unused_weak_environment() {
    for view in ["bytes_view(b)", "bytes_view_mut(b)"] {
        let source = format!(
            "{SUPPORT}fn main()->i32{{let b=bytes_new();bytes_reserve(b,1);bytes_push(b,u8(65));let f=()=>i32(bytes_len(b));let view={view};consume(give b);return i32(view[0])-65}}"
        );
        rejected(
            &source,
            ResultCategory::SafeReject,
            Phase::Verify,
            "E-BORROW-CONFLICT",
        );
        native(
            &[(
                "main",
                &source.replace(
                    "consume(give b);return i32(view[0])-65",
                    "let value=i32(view[0]);consume(give b);return value-65",
                ),
            )],
            b"",
            0,
        );
    }
}

#[test]
fn raw_and_wire_cannot_move_cleanup_before_the_last_captured_use() {
    use zil_ir::InstructionKind as I;
    let source = "fn main()->i32{let b=bytes_new();let f=()=>i32(bytes_len(b));return f()}";
    let (mut module, plan) = candidate(source);
    let function = module
        .functions
        .iter_mut()
        .find(|f| f.name == "main")
        .unwrap();
    let block = function
        .blocks
        .iter_mut()
        .find(|b| {
            b.instructions
                .iter()
                .any(|i| matches!(i.kind, I::CallClosure { .. }))
        })
        .unwrap();
    let position = block
        .instructions
        .iter()
        .position(|i| matches!(i.kind, I::DropManaged { .. }))
        .unwrap();
    let drop = block.instructions.remove(position);
    let before_call = block
        .instructions
        .iter()
        .position(|i| matches!(i.kind, I::CallClosure { .. }))
        .unwrap();
    block.instructions.insert(before_call, drop);
    reject_candidate(
        "drop-before-captured-buffer-last-call",
        module,
        plan,
        "E-VERIFY-RESOURCE",
        "invalidate a live description",
    );
}

#[test]
fn four_module_buffer_consumer_checks_runtime_input_and_cleanup_order() {
    let files = [
        (
            "main",
            include_str!("../../../examples/mainline/borrowed_bytes187/main.zil"),
        ),
        (
            "model",
            include_str!("../../../examples/mainline/borrowed_bytes187/model.zil"),
        ),
        (
            "operations",
            include_str!("../../../examples/mainline/borrowed_bytes187/operations.zil"),
        ),
        (
            "visits",
            include_str!("../../../examples/mainline/borrowed_bytes187/visits.zil"),
        ),
    ];
    for (input, output, exit) in [
        (b"0".as_slice(), b"TSZ".as_slice(), 0),
        (b"1", b"1FTSZ", 0),
        (b"2", b"1F2FTSZ", 0),
        (b"3", b"1F2F3FTSZ", 0),
        (b"9", b"", 9),
    ] {
        native_input(&files, input, output, exit);
    }
}
