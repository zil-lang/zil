//! Exclusive initialized Store elements: real Copy writes, loans and independent rejection.
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
        "store-mut180-{}-{}",
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
    native_input(files, b"", stdout, exit)
}

fn native_input(files: &[(&str, &str)], input: &[u8], stdout: &[u8], exit: i32) -> Vec<PathBuf> {
    native_input_backend(files, input, stdout, exit, zil_driver::NativeBackend::Clang)
}

fn replacement_native(files: &[(&str, &str)], stdout: &[u8], exit: i32) {
    for backend in [
        zil_driver::NativeBackend::Clang,
        zil_driver::NativeBackend::LlvmApi,
    ] {
        if backend == zil_driver::NativeBackend::LlvmApi && !cfg!(feature = "llvm-api") {
            continue;
        }
        native_input_backend(files, b"", stdout, exit, backend);
    }
}

fn native_input_backend(
    files: &[(&str, &str)],
    input: &[u8],
    stdout: &[u8],
    exit: i32,
    backend: zil_driver::NativeBackend,
) -> Vec<PathBuf> {
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
                backend,
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
    let source = SourceUnit::new(FileId(0), ModuleId(0), "raw180.zil", source);
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

const MODEL: &str = r#"
extern "C" fn putchar(n:i32)->i32;
resource R{code:i32}
impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
struct Point{x:i32;y:i32}
package Cell{manages owner:R;samples:[i32;3];point:Point;tag:i32}
fn cell(n:i32)->Cell=>Cell{owner:R{code:n},samples:[1,2,3],point:Point{x:4,y:5},tag:n}
fn empty()->Store<Cell> contract{result(create)}{return store_new()}
fn read(v:&Cell)->i32=>v.tag
fn bump(v:&mut Cell, delta:i32)->i32{v.tag+=delta;return v.tag}
"#;
const SETUP: &str =
    "let s=empty();store_reserve(s,8);store_push(s,cell(65));store_push(s,cell(66));";
const STATE: &str = r#"
package State{manages head:R;manages left:Store<Cell>;manages right:Store<Cell>}
fn state()->State{let a=empty();let b=empty();store_reserve(a,4);store_reserve(b,2);store_push(a,cell(65));store_push(b,cell(66));return State{head:R{code:90},left:give a,right:give b}}
fn relay(s:State)->State=>s
"#;

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

#[test]
fn actual_copy_scalar_array_and_record_fields_update_in_place() {
    let text = format!(
        "{MODEL}fn main()->i32{{{SETUP}let v=store_at_mut(s,1);v.tag=71;v.samples[1]=11;v.point.x=9;let total=read(v)+v.samples[1]+v.point.x;store_at_mut(s,0).tag=70;return total+read(store_at(s,0))-161}}"
    );
    let dirs = native(&[("main", &text)], b"AB", 0);
    for dir in dirs {
        let llvm = fs::read_to_string(dir.join("phases/llvm.ll")).unwrap();
        assert!(llvm.contains("store.element.ok."));
        assert!(llvm.contains("icmp ult i64"));
        assert!(!llvm.contains(" noalias "));
    }
}

#[test]
fn synchronous_helper_reborrows_then_last_use_allows_reserve_and_pop() {
    let text = format!(
        "{MODEL}fn relay_bump(v:&mut Cell)->i32{{return bump(v,2)}}fn main()->void!i32{{{SETUP}let v=store_at_mut(s,0);let first=relay_bump(v);let second=bump(v,3);let shared=read(v);let child=&mut v.samples[2];*child=7;let sample=*child;store_reserve(s,16);let last=store_pop(s)?;return first+second+shared+sample+last.tag-280}}"
    );
    native(&[("main", &text)], b"BA", 0);
}

#[test]
fn separate_store_fields_and_parameter_apart_checks_preserve_updates() {
    let text = format!(
        "{MODEL}{STATE}fn update(v:&mut Cell,other:Store<Cell>)->i32{{store_reserve(other,9);v.tag+=1;return v.tag}}fn main()->i32{{let s=state();let v=store_at_mut(s.left,0);let n=update(v,s.right);let moved=relay(s);return n-66}}"
    );
    native(&[("main", &text)], b"ZAB", 0);
}

#[test]
fn whole_store_exclusive_loan_blocks_shared_and_other_exclusive_views() {
    for body in [
        "let a=store_at_mut(s,0);let b=store_at(s,1);return a.tag+b.tag",
        "let a=store_at(s,0);let b=store_at_mut(s,1);return a.tag+b.tag",
        "let a=store_at_mut(s,0);let b=store_at_mut(s,1);return a.tag+b.tag",
        "let a=store_at_mut(s,0);let same=s;let b=store_at(same,0);return a.tag+b.tag",
        "let a=store_at_mut(s,0);let point=&a.tag;a.tag=12;return *point",
        "let a=store_at_mut(s,0);let child=&mut a.tag;let parent=read(a);return *child+parent",
    ] {
        rejected(
            &format!("{MODEL}fn main()->i32{{{SETUP}{body}}}"),
            ResultCategory::SafeReject,
            Phase::Verify,
            "E-BORROW-CONFLICT",
        );
    }
}

#[test]
fn live_exclusive_window_blocks_structure_metadata_and_responsibility_changes() {
    for change in [
        "store_reserve(s,16);",
        "store_push(s,cell(67));",
        "store_pop(s);",
        "store_len(s);",
        "store_capacity(s);",
        "consume(give s);",
    ] {
        rejected(
            &format!(
                "{MODEL}fn consume(give s:Store<Cell>){{}}fn main()->i32{{{SETUP}let v=store_at_mut(s,0);{change}return v.tag}}"
            ),
            ResultCategory::SafeReject,
            Phase::Verify,
            "E-BORROW-CONFLICT",
        );
    }
    for change in [
        "s.left=empty();",
        "let moved=relay(s);",
        "let moved=give s.left;",
    ] {
        rejected(
            &format!(
                "{MODEL}{STATE}fn main()->i32{{var s=state();let v=store_at_mut(s.left,0);{change}return v.tag}}"
            ),
            ResultCategory::SafeReject,
            Phase::Verify,
            "E-BORROW-CONFLICT",
        );
    }
}

#[test]
fn cross_function_aliases_cannot_hide_mutation_or_second_window() {
    for helper in [
        "fn update(a:&mut Cell,b:Store<Cell>)->i32{store_reserve(b,16);return a.tag}",
        "fn update(a:&mut Cell,b:Store<Cell>)->i32{let other=store_at(b,1);return a.tag+other.tag}",
        "fn update(a:&mut Cell,b:Store<Cell>)->i32{let other=store_at_mut(b,1);return a.tag+other.tag}",
    ] {
        rejected(
            &format!(
                "{MODEL}{helper}fn main()->i32{{{SETUP}let v=store_at_mut(s,0);return update(v,s)}}"
            ),
            ResultCategory::SafeReject,
            Phase::Verify,
            "E-BORROW-CONFLICT",
        );
    }
}

#[test]
fn shared_windows_cannot_be_upgraded_or_written() {
    for body in [
        "let v=store_at(s,0);v.tag=9;return 0",
        "let v=store_at(s,0);return bump(v,1)",
    ] {
        rejected(
            &format!("{MODEL}fn main()->i32{{{SETUP}{body}}}"),
            ResultCategory::SafeReject,
            Phase::Types,
            "exclusive",
        );
    }
}

#[test]
fn managed_fields_whole_packages_and_escaping_windows_remain_closed() {
    // Original source unchanged: mutate the actual Copy payload through the
    // exclusive head, then observe normal Store element cleanup in order.
    let text =
        format!("{MODEL}fn main()->i32{{{SETUP}let v=store_at_mut(s,0);v.owner.code=90;return 0}}");
    native(&[("main", &text)], b"ZB", 0);
    // The original exclusive owning-write source now has independent Store
    // responsibility consumers. The retained address drops A, stores Z, then
    // ordinary Store cleanup observes Z and the unchanged sibling B.
    let replacement = format!(
        "{MODEL}fn main()->i32{{{SETUP}let v=store_at_mut(s,0);v.owner=R{{code:90}};return 0}}"
    );
    let unit = SourceUnit::new(FileId(0), ModuleId(0), "replacement180.zil", &replacement);
    let syntax = zil_syntax::parse(&unit).into_result().unwrap();
    let core = zil_expand::expand(&unit, &syntax).unwrap();
    let typed = zil_types::check(&core, &TargetProfile::host_llvm(), &Limits::default()).unwrap();
    let instructions = typed
        .functions
        .iter()
        .flat_map(|f| &f.blocks)
        .flat_map(|b| &b.instructions)
        .collect::<Vec<_>>();
    assert!(
        instructions
            .iter()
            .any(|i| matches!(i.kind, zil_ir::InstructionKind::OwnedStoreBorrowMut { .. }))
    );
    let reservations = instructions
        .iter()
        .filter_map(|i| match &i.kind {
            zil_ir::InstructionKind::ReservePlaceAddress {
                reservation, path, ..
            } => {
                assert_eq!(path, &[zil_ir::PlaceProjection::Field(0)]);
                Some(*reservation)
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(reservations.len(), 1);
    let commits = instructions
        .iter()
        .filter_map(|i| match i.kind {
            zil_ir::InstructionKind::CommitPlaceReplace { reservation, .. } => Some(reservation),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(commits, reservations);
    let report = check(&replacement);
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1);
    replacement_native(&[("main", &replacement)], b"AZB", 0);
    // Nearest actual permission error: a shared Store element cannot supply
    // the exclusive parent required to prepare this owning replacement.
    rejected(
        &replacement.replace("let v=store_at_mut(s,0)", "let v=store_at(s,0)"),
        ResultCategory::SafeReject,
        Phase::Types,
        "E-ADDRESS-PERMISSION",
    );
    {
        let body = "let v=store_at_mut(s,0);let callback=()=>v.tag;return callback()";
        let report = check(&format!("{MODEL}fn main()->i32{{{SETUP}{body}}}"));
        assert_eq!(
            report.status,
            ResultCategory::Unsupported,
            "{}",
            report.to_json()
        );
        assert_eq!(report.phase, Phase::Types, "{}", report.to_json());
        assert!(
            report
                .diagnostics
                .iter()
                .any(|d| d.message.contains("Copy or concrete callback fields")
                    || d.message.contains("capture")),
            "{}",
            report.to_json()
        );
    }
    {
        let helper = "fn escape(s:Store<Cell>)->&mut Cell{return store_at_mut(s,0)}";
        rejected(
            &format!("{MODEL}{helper}fn main(){{}}"),
            ResultCategory::Unsupported,
            Phase::Types,
            "synchronous inputs",
        );
    }
    for helper in [
        "fn escape(s:Store<Cell>)->&i32{let v=store_at_mut(s,0);return &v.tag}",
        "fn escape(v:&mut Cell)->&mut i32{return &mut v.tag}",
    ] {
        let report = check(&format!("{MODEL}{helper}fn main(){{}}"));
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1);
    }
    let report = check(&format!(
        "{MODEL}fn main()->i32{{{SETUP}let v=store_at_mut(s,0);let copied=*v;return copied.tag}}"
    ));
    assert_ne!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert!(matches!(
        report.status,
        ResultCategory::Unsupported | ResultCategory::SafeReject
    ));
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.message.contains("Copy")),
        "{}",
        report.to_json()
    );
}

#[test]
fn runtime_index_traps_before_address_even_when_capacity_has_spare_slots() {
    let text = format!(
        "{MODEL}extern \"C\" fn getchar()->i32;fn main()->i32{{{SETUP}var input=0;unsafe{{input=getchar()}};let v=store_at_mut(s,i64(input)-48);v.tag=70;return v.tag-70}}"
    );
    for optimization in [Optimization::O0, Optimization::O2] {
        let dir = directory();
        fs::write(dir.join("main.zil"), &text).unwrap();
        let executable = dir.join("program");
        let report = compile_project(
            &project(&[("main", &text)]),
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
        for (input, trap) in [
            (b"0", false),
            (b"1", false),
            (b"2", true),
            (b"7", true),
            (b"/", true),
        ] {
            let mut child = Command::new(&executable)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap();
            child.stdin.take().unwrap().write_all(input).unwrap();
            let output = child.wait_with_output().unwrap();
            fs::write(
                dir.join(format!("native-{}.txt", input[0])),
                format!("{optimization:?} {output:?}\n"),
            )
            .unwrap();
            if trap {
                assert_eq!(
                    std::os::unix::process::ExitStatusExt::signal(&output.status),
                    Some(4),
                    "{output:?}"
                );
                assert!(output.stdout.is_empty());
            } else {
                assert_eq!(output.status.code(), Some(0), "{output:?}");
                assert_eq!(output.stdout, b"AB");
            }
            assert!(output.stderr.is_empty());
        }
    }
}

#[test]
fn raw_and_wire_rebuild_mutability_index_shape_and_actual_holds() {
    use zil_ir::{InstructionKind as I, Type};
    let text =
        format!("{MODEL}fn main()->i32{{{SETUP}let v=store_at_mut(s,0);v.tag=9;return v.tag}}");
    let (module, plan) = candidate(&text);
    let main = module
        .functions
        .iter()
        .position(|f| f.name == "main")
        .unwrap();
    let (block, position, store, index, result) = module.functions[main]
        .blocks
        .iter()
        .enumerate()
        .find_map(|(b, block)| {
            block
                .instructions
                .iter()
                .enumerate()
                .find_map(|(i, inst)| match inst.kind {
                    I::OwnedStoreBorrowMut { store, index } => {
                        Some((b, i, store, index, inst.result.unwrap()))
                    }
                    _ => None,
                })
        })
        .unwrap();
    let mut bad = module.clone();
    bad.functions[main].blocks[block].instructions[position].kind =
        I::OwnedStoreBorrow { store, index };
    reject_candidate("upgrade-shared-op", bad, plan.clone(), "E-VERIFY-TYPE", "");
    let mut bad = module.clone();
    if let Type::Address { mutable, .. } = &mut bad.functions[main].values[result.0 as usize] {
        *mutable = false;
    }
    reject_candidate(
        "downgrade-exclusive-result",
        bad,
        plan.clone(),
        "E-VERIFY-TYPE",
        "",
    );
    let mut bad = module.clone();
    bad.functions[main].blocks[block].instructions[position].kind = I::OwnedStoreBorrowMut {
        store: index,
        index,
    };
    reject_candidate(
        "wrong-store",
        bad,
        plan.clone(),
        "E-VERIFY-OWNED-STORE",
        "description",
    );
    let mut bad = module.clone();
    bad.functions[main].blocks[block].instructions[position].kind = I::OwnedStoreBorrowMut {
        store,
        index: store,
    };
    reject_candidate(
        "wrong-index",
        bad,
        plan.clone(),
        "E-VERIFY-OWNED-STORE",
        "index must be i64",
    );
    let mut bad = module.clone();
    bad.functions[main].values[result.0 as usize] = Type::Address {
        mutable: true,
        inner: Box::new(Type::I32),
    };
    reject_candidate("wrong-pointee", bad, plan.clone(), "E-VERIFY-TYPE", "");
    let mut forged = plan.clone();
    forged.functions[main]
        .holds
        .iter_mut()
        .find(|h| h.value == result)
        .unwrap()
        .roots
        .clear();
    reject_candidate("erased-root", module, forged, "E-VERIFY-HOLD", "");
}

#[test]
fn raw_and_wire_retargeted_structure_write_cannot_cross_exclusive_window() {
    use zil_ir::InstructionKind as I;
    let text = format!(
        "{MODEL}{STATE}fn main()->i32{{let s=state();let v=store_at_mut(s.left,0);store_reserve(s.right,16);v.tag=9;return v.tag}}"
    );
    let (mut module, plan) = candidate(&text);
    let f = module
        .functions
        .iter_mut()
        .find(|f| f.name == "main")
        .unwrap();
    let descriptor = f
        .blocks
        .iter_mut()
        .flat_map(|b| &mut b.instructions)
        .find(|i| matches!(i.kind, I::DescribePackageField { index: 2, .. }))
        .unwrap();
    if let I::DescribePackageField { index, .. } = &mut descriptor.kind {
        *index = 1;
    }
    reject_candidate(
        "retargeted-sibling-store",
        module,
        plan,
        "E-BORROW-CONFLICT",
        "",
    );
}

#[test]
fn runtime_updates_cross_four_modules_without_moving_element_resources() {
    let files = [
        (
            "main",
            include_str!("../../../examples/mainline/store_mut180/main.zil"),
        ),
        (
            "model",
            include_str!("../../../examples/mainline/store_mut180/model.zil"),
        ),
        (
            "storage",
            include_str!("../../../examples/mainline/store_mut180/storage.zil"),
        ),
        (
            "edit",
            include_str!("../../../examples/mainline/store_mut180/edit.zil"),
        ),
    ];
    for (input, stdout, exit) in [
        (b"0".as_slice(), b"".as_slice(), 0),
        (b"1", b"aA", 0),
        (b"3", b"abcABC", 0),
        (b"4", b"abcdABCD", 0),
        (b"9", b"", 9),
    ] {
        native_input(&files, input, stdout, exit);
    }
}

#[test]
fn receiver_index_and_rhs_are_evaluated_once_and_in_source_order() {
    let text = format!(
        r#"{MODEL}
fn receiver()->Store<Cell> contract{{result(create)}}{{unsafe{{putchar(82)}};let s=empty();store_reserve(s,1);store_push(s,cell(65));return give s}}
fn index()->i64{{unsafe{{putchar(73)}};return 0}}
fn rhs()->i32{{unsafe{{putchar(86)}};return 9}}
fn main()->i32{{store_at_mut(receiver(),index()).tag=rhs();return 0}}
"#
    );
    native(&[("main", &text)], b"RIVA", 0);
}

#[test]
fn index_effect_runs_before_window_and_error_rhs_cleans_store() {
    let text = format!(
        "{MODEL}fn index(s:Store<Cell>)->i64{{store_reserve(s,32);return 0}}fn main()->i32{{{SETUP}let v=store_at_mut(s,index(s));v.tag=71;return v.tag-71}}"
    );
    native(&[("main", &text)], b"AB", 0);
    let text = format!(
        "{MODEL}enum Fault{{No}}fn fail()->Fault!i32=>Fault.No;fn edit(v:&mut Cell)->Fault!void{{v.tag=fail()?;return ()}}fn main()->Fault!i32{{{SETUP}let v=store_at_mut(s,0);edit(v)?;return 90}}"
    );
    native(&[("main", &text)], b"AB", 1);
}

#[test]
fn rhs_and_child_capture_cannot_hide_a_live_window() {
    rejected(
        &format!(
            "{MODEL}fn main()->i32{{{SETUP}store_at_mut(s,0).tag=i32(store_len(s));return 0}}"
        ),
        ResultCategory::SafeReject,
        Phase::Verify,
        "E-BORROW-CONFLICT",
    );
    rejected(
        &format!(
            "{MODEL}fn main()->i32{{{SETUP}let v=store_at_mut(s,0);let point=&mut v.tag;var callback=()=>{{*point+=1;return *point}};store_reserve(s,16);return callback()}}"
        ),
        ResultCategory::SafeReject,
        Phase::Verify,
        "E-BORROW-CONFLICT",
    );
    let text = format!(
        "{MODEL}fn main()->i32{{{SETUP}let v=store_at_mut(s,0);let point=&mut v.tag;var callback=()=>{{*point+=1;return *point}};let n=callback();store_reserve(s,16);return n-66}}"
    );
    native(&[("main", &text)], b"AB", 0);
}

#[test]
fn raw_and_wire_reject_resource_projection_and_early_owner_drop() {
    use zil_ir::{InstructionKind as I, Type};
    let text =
        format!("{MODEL}fn main()->i32{{{SETUP}let v=store_at_mut(s,0);v.tag=9;return v.tag}}");
    let (module, plan) = candidate(&text);
    let main = module
        .functions
        .iter()
        .position(|f| f.name == "main")
        .unwrap();
    let mut bad = module.clone();
    let f = &mut bad.functions[main];
    let field = f
        .blocks
        .iter_mut()
        .flat_map(|b| &mut b.instructions)
        .find(|i| matches!(i.kind, I::AddressField { .. }))
        .unwrap();
    let I::AddressField { address, index } = &mut field.kind else {
        unreachable!()
    };
    *index = 0;
    let parent = *address;
    let projected = field.result.unwrap();
    let Type::Address { mutable, inner } = &f.values[parent.0 as usize] else {
        unreachable!()
    };
    let Type::Package { fields, .. } = inner.as_ref() else {
        unreachable!()
    };
    let projected_type = Type::Address {
        mutable: *mutable,
        inner: Box::new(fields[0].1.clone()),
    };
    f.values[projected.0 as usize] = projected_type.clone();
    // Carry the actual projected type through the source's explicit reborrow.
    // The attempted owned write still reaches the independent owner check.
    for inst in f.blocks.iter().flat_map(|b| &b.instructions) {
        if let I::ReborrowAddress { address, mutable } = inst.kind
            && address == projected
        {
            let Type::Address { inner, .. } = &projected_type else {
                unreachable!()
            };
            f.values[inst.result.unwrap().0 as usize] = Type::Address {
                mutable,
                inner: inner.clone(),
            };
        }
    }
    reject_candidate(
        "managed-field-projection",
        bad,
        plan.clone(),
        "E-UNSUPPORTED",
        "",
    );
    let mut bad = module.clone();
    let f = &mut bad.functions[main];
    let owner = f
        .locals
        .iter()
        .find(|l| matches!(&l.ty,Type::Managed(t) if matches!(t.as_ref(),Type::OwnedStore{..})))
        .unwrap()
        .id;
    let (drop_block, drop_index) = f
        .blocks
        .iter()
        .enumerate()
        .find_map(|(b, block)| {
            block
                .instructions
                .iter()
                .position(|i| matches!(i.kind,I::DropManaged{local} if local==owner))
                .map(|i| (b, i))
        })
        .unwrap();
    let dropped = f.blocks[drop_block].instructions.remove(drop_index);
    let (borrow_block, borrow_index) = f
        .blocks
        .iter()
        .enumerate()
        .find_map(|(b, block)| {
            block
                .instructions
                .iter()
                .position(|i| matches!(i.kind, I::OwnedStoreBorrowMut { .. }))
                .map(|i| (b, i))
        })
        .unwrap();
    f.blocks[borrow_block]
        .instructions
        .insert(borrow_index + 1, dropped);
    reject_candidate("early-store-drop", bad, plan, "E-BORROW-CONFLICT", "");
}

#[test]
fn projected_copy_slice_keeps_then_releases_exclusive_permission() {
    let text = format!(
        "{MODEL}fn main()->i32{{{SETUP}let v=store_at_mut(s,0);let part=&mut v.samples[0..2];part[1]=7;let n=part[1];store_reserve(s,16);return n-7}}"
    );
    native(&[("main", &text)], b"AB", 0);
    rejected(
        &format!(
            "{MODEL}fn main()->i32{{{SETUP}let v=store_at_mut(s,0);let part=&mut v.samples[0..2];store_reserve(s,16);return part[1]}}"
        ),
        ResultCategory::SafeReject,
        Phase::Verify,
        "E-BORROW-CONFLICT",
    );
}

#[test]
fn index_evaluation_cannot_consume_the_previously_evaluated_store() {
    rejected(
        &format!(
            "{MODEL}fn index(give s:Store<Cell>)->i64{{return 0}}fn main()->i32{{{SETUP}let v=store_at_mut(s,index(give s));return v.tag}}"
        ),
        ResultCategory::SafeReject,
        Phase::Analyze,
        "description",
    );
}

#[test]
fn raw_and_wire_cannot_write_after_consistently_downgrading_the_window() {
    use zil_ir::{InstructionKind as I, Type};
    let text =
        format!("{MODEL}fn main()->i32{{{SETUP}let v=store_at_mut(s,0);v.tag=9;return v.tag}}");
    let (mut module, plan) = candidate(&text);
    let f = module
        .functions
        .iter_mut()
        .find(|f| f.name == "main")
        .unwrap();
    for ty in f
        .values
        .iter_mut()
        .chain(f.locals.iter_mut().map(|l| &mut l.ty))
    {
        if let Type::Address { mutable, .. } = ty {
            *mutable = false;
        }
    }
    for inst in f.blocks.iter_mut().flat_map(|b| &mut b.instructions) {
        if let I::OwnedStoreBorrowMut { store, index } = inst.kind {
            inst.kind = I::OwnedStoreBorrow { store, index };
        }
    }
    reject_candidate(
        "consistent-shared-write",
        module,
        plan,
        "E-VERIFY-ADDRESS",
        "exclusive",
    );
}

#[test]
fn owning_store_field_replacement_preserves_failure_and_actual_description_boundaries() {
    for (prepared, stdout) in [(false, b"ABX".as_slice()), (true, b"CABX".as_slice())] {
        let rhs = if prepared {
            "make(R{code:67},fail()?)"
        } else {
            "R{code:fail()?}"
        };
        let text = format!(
            "{MODEL}enum F{{Bad}}fn fail()->F!i32{{return F.Bad()}}fn make(give spare:R,n:i32)->R contract{{result(create)}}{{return R{{code:n}}}}fn run()->F!i32{{{SETUP}let v=store_at_mut(s,0);v.owner={rhs};return 0}}fn main()->i32{{match run(){{.ok(_)=>{{return 1}},.err(_)=>{{unsafe{{putchar(88)}}return 0}}}}}}"
        );
        replacement_native(&[("main", &text)], stdout, 0);
    }
    for body in [
        "let v=store_at_mut(s,0);let old=v.owner;v.owner=R{code:90};return old.code",
        "let old=store_at(s,1).owner;let v=store_at_mut(s,0);v.owner=R{code:90};return old.code",
        "let index:i64=1;let old=store_at(s,index).owner;let v=store_at_mut(s,0);v.owner=R{code:90};return old.code",
    ] {
        rejected(
            &format!("{MODEL}fn main()->i32{{{SETUP}{body}}}"),
            ResultCategory::SafeReject,
            Phase::Analyze,
            "live element resource description",
        );
    }
    // A same-type description from a physically separate Store is still valid.
    let other = format!(
        "{MODEL}fn main()->i32{{{SETUP}let t=empty();store_reserve(t,1);store_push(t,cell(67));let old=store_at(t,0).owner;let v=store_at_mut(s,0);v.owner=R{{code:90}};return old.code-67}}"
    );
    replacement_native(&[("main", &other)], b"ACZB", 0);
    let package = format!(
        "{MODEL}{STATE}fn main()->i32{{let s=state();let v=store_at_mut(s.left,0);v.owner=R{{code:68}};return 0}}"
    );
    replacement_native(&[("main", &package)], b"AZDB", 0);
    let order = format!(
        "{MODEL}fn index()->i64{{unsafe{{putchar(73)}}return 0}}fn owner()->R contract{{result(create)}}{{unsafe{{putchar(82)}}return R{{code:90}}}}fn main()->i32{{{SETUP}let v=store_at_mut(s,index());v.owner=owner();return 0}}"
    );
    replacement_native(&[("main", &order)], b"IRAZB", 0);
    let child = format!(
        "{MODEL}fn main()->i32{{{SETUP}let v=store_at_mut(s,0);let child=&v.owner.code;v.owner=R{{code:90}};return *child}}"
    );
    let report = check(&child);
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
    let grow = format!(
        "{MODEL}fn grow(s:Store<Cell>)->R contract{{result(create)}}{{store_reserve(s,32);return R{{code:90}}}}fn main()->i32{{{SETUP}let v=store_at_mut(s,0);v.owner=grow(s);return v.tag}}"
    );
    let report = check(&grow);
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
}

#[test]
fn owning_store_replacement_raw_and_wire_rebuild_description_liveness_and_parent_pin() {
    use zil_ir::InstructionKind as I;
    // The real source uses the old instance before settlement. Move only that
    // contiguous consumer into the post-Commit position, preserving definitions
    // and the honest candidate's origin claims. The stale Desc is now live.
    for old in ["let old=v.owner;", "let old=store_at(s,1).owner;"] {
        let setup = if old.contains("store_at") {
            ""
        } else {
            "let v=store_at_mut(s,0);"
        };
        let borrow = if setup.is_empty() {
            "let v=store_at_mut(s,0);"
        } else {
            ""
        };
        let text = format!(
            "{MODEL}fn main()->i32{{{SETUP}{setup}{old}let n=old.code;{borrow}v.owner=R{{code:90}};return n-65}}"
        );
        let (mut module, plan) = candidate(&text);
        let f = module
            .functions
            .iter_mut()
            .find(|f| f.name == "main")
            .unwrap();
        let block = f
            .blocks
            .iter_mut()
            .find(|b| {
                b.instructions
                    .iter()
                    .any(|i| matches!(i.kind, I::CommitPlaceReplace { .. }))
            })
            .unwrap();
        let first = block
            .instructions
            .iter()
            .position(|i| matches!(i.kind, I::ReadDesc { .. }))
            .unwrap();
        let end = first
            + block.instructions[first..]
                .iter()
                .position(|i| matches!(i.kind, I::Store { .. }))
                .unwrap()
            + 1;
        let read = block.instructions.drain(first..end).collect::<Vec<_>>();
        let commit = block
            .instructions
            .iter()
            .position(|i| matches!(i.kind, I::CommitPlaceReplace { .. }))
            .unwrap();
        block.instructions.splice(commit + 1..commit + 1, read);
        let errors = zil_analyze::analyze(&module, &Limits::default()).unwrap_err();
        assert!(
            errors
                .iter()
                .any(|d| d.message.contains("live element resource description")),
            "{errors:?}"
        );
        let (code, message) = if old.contains("store_at") {
            ("E-BORROW-CONFLICT", "")
        } else {
            ("E-VERIFY-RESOURCE", "live element resource description")
        };
        reject_candidate(
            "store-replacement-stale-field-description",
            module,
            plan,
            code,
            message,
        );
    }
    let text = format!(
        "{MODEL}fn grow(s:Store<Cell>)->R contract{{result(create)}}{{store_reserve(s,32);return R{{code:90}}}}fn main()->i32{{{SETUP}let t=empty();let v=store_at_mut(s,0);v.owner=grow(t);return v.tag-65}}"
    );
    let (mut module, plan) = candidate(&text);
    let grow = module
        .functions
        .iter()
        .find(|f| f.name == "grow")
        .unwrap()
        .id;
    let f = module
        .functions
        .iter_mut()
        .find(|f| f.name == "main")
        .unwrap();
    let parent = f
        .blocks
        .iter()
        .flat_map(|b| &b.instructions)
        .find_map(|i| match i.kind {
            I::OwnedStoreBorrowMut { store, .. } => Some(store),
            _ => None,
        })
        .unwrap();
    let call = f
        .blocks
        .iter_mut()
        .flat_map(|b| &mut b.instructions)
        .find(|i| matches!(i.kind, I::Call { function, .. } if function == grow))
        .unwrap();
    if let I::Call { arguments, .. } = &mut call.kind {
        arguments[0] = parent;
    }
    assert!(zil_analyze::analyze(&module, &Limits::default()).is_err());
    reject_candidate(
        "store-replacement-parent-reallocation",
        module,
        plan,
        "E-BORROW-CONFLICT",
        "",
    );
}

#[test]
fn owning_store_parameter_and_product_helper_invalidation_summaries() {
    for helper in [
        "fn replace(s:Store<Cell>){let v=store_at_mut(s,0);v.owner=R{code:90}}fn main()->i32{let s=empty();store_reserve(s,1);store_push(s,cell(65));replace(s);return 0}",
        "fn replace(v:&mut Cell){v.owner=R{code:90}}fn main()->i32{let s=empty();store_reserve(s,1);store_push(s,cell(65));replace(store_at_mut(s,0));return 0}",
    ] {
        let text = format!("{MODEL}{helper}");
        let report = check(&text);
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        replacement_native(&[("main", &text)], b"AZ", 0);
    }
    // A may-write to the Copy payload keeps the same resource instance. Keep
    // an unused real replacement in the module so the new Plan is active.
    let copy = format!(
        "{MODEL}fn unused(s:Store<Cell>){{let v=store_at_mut(s,0);v.owner=R{{code:88}}}}fn update(v:&mut Cell){{v.owner.code=90}}fn main()->i32{{{SETUP}let v=store_at_mut(s,0);let old=v.owner;update(v);return old.code-90}}"
    );
    replacement_native(&[("main", &copy)], b"ZB", 0);
}

#[test]
fn owning_store_replacement_has_exact_prepaid_fresh_and_warm_boundaries() {
    let text = format!(
        "{MODEL}fn main()->i32{{{SETUP}let v=store_at_mut(s,0);v.owner=R{{code:90}};return v.owner.code-90}}"
    );
    let unit = SourceUnit::new(
        FileId(0),
        ModuleId(0),
        "store-replacement-budget.zil",
        &text,
    );
    // G1.3.6: previous Analyze19754; paid retirement profile/empty-shape
    // forwarding exclusions add71 without granting a new Store operation.
    for (stage, expected_n) in [(Stage::Analyze, 19825), (Stage::Verify, 54842)] {
        let options = |work| CompileOptions {
            stop_after: stage,
            limits: Limits {
                max_analysis_steps: work,
                ..Limits::default()
            },
            ..CompileOptions::default()
        };
        let (mut low, mut high) = (1, Limits::default().max_analysis_steps);
        while low < high {
            let mid = low + (high - low) / 2;
            let report = zil_driver::compile_source(&unit, &options(mid));
            match report.status {
                ResultCategory::Success => high = mid,
                ResultCategory::ResourceLimit => low = mid + 1,
                _ => panic!("{}", report.to_json()),
            }
        }
        assert_eq!(
            low, expected_n,
            "{stage:?}: actual Store replacement work changed"
        );
        let mut warm = zil_driver::CompilationSession::default();
        for (work, expected) in [
            (low, ResultCategory::Success),
            (low - 1, ResultCategory::ResourceLimit),
            (low, ResultCategory::Success),
        ] {
            for report in [
                zil_driver::compile_source(&unit, &options(work)),
                warm.compile_source(&unit, &options(work)),
            ] {
                assert_eq!(report.status, expected, "{}", report.to_json());
                if expected == ResultCategory::ResourceLimit {
                    assert_eq!(
                        report.phase,
                        if stage == Stage::Analyze {
                            Phase::Analyze
                        } else {
                            Phase::Verify
                        },
                        "{}",
                        report.to_json()
                    );
                }
                if expected == ResultCategory::Success && stage == Stage::Verify {
                    assert_eq!(report.verification_runs, 1);
                }
            }
            assert_eq!(warm.active_query_count(), 0);
        }
        println!(
            "G1.3.1 {stage:?} full-request N={low}; N-1 ResourceLimit; fresh/warm/retry agree"
        );
    }
}

#[test]
fn owning_store_call_invalidation_reaches_relays_live_descriptions_and_failure_cleanup() {
    let replace = "fn replace(s:Store<Cell>){let v=store_at_mut(s,0);v.owner=R{code:90}}fn relay(s:Store<Cell>){replace(s)}";
    let product = "fn replace(v:&mut Cell){v.owner=R{code:90}}fn relay(v:&mut Cell){replace(v)}";
    for (helpers, body) in [
        (
            replace,
            "let old=store_at(s,0).owner;replace(s);return old.code",
        ),
        (
            replace,
            "let old=store_at(s,1).owner;let alias=s;relay(alias);return old.code",
        ),
        (
            product,
            "let v=store_at_mut(s,0);let old=v.owner;replace(v);return old.code",
        ),
        (
            product,
            "let v=store_at_mut(s,0);let old=v.owner;relay(v);return old.code",
        ),
        (
            replace,
            "let old=store_at(s,0).owner;let saved=()=>old.code;relay(s);return saved()",
        ),
    ] {
        rejected(
            &format!("{MODEL}{helpers}fn main()->i32{{{SETUP}{body}}}"),
            ResultCategory::SafeReject,
            Phase::Analyze,
            "live element resource description",
        );
    }
    let relay = format!(
        "{MODEL}{replace}fn main()->i32{{{SETUP}relay(s);return store_at(s,0).owner.code-90}}"
    );
    replacement_native(&[("main", &relay)], b"AZB", 0);
    let other = format!(
        "{MODEL}{replace}fn main()->i32{{{SETUP}let t=empty();store_reserve(t,1);store_push(t,cell(67));let old=store_at(t,0).owner;relay(s);return old.code-67}}"
    );
    replacement_native(&[("main", &other)], b"ACZB", 0);
    let saved = format!(
        "{MODEL}{replace}fn main()->i32{{{SETUP}let t=empty();store_reserve(t,1);store_push(t,cell(67));let old=store_at(t,0).owner;let saved=()=>old.code;relay(s);return saved()-67}}"
    );
    replacement_native(&[("main", &saved)], b"ACZB", 0);
    for (prepared, expected) in [(false, b"XAB".as_slice()), (true, b"CXAB".as_slice())] {
        let rhs = if prepared {
            "make(R{code:67},fail()?)"
        } else {
            "R{code:fail()?}"
        };
        let source = format!(
            "{MODEL}enum F{{Bad}}fn fail()->F!i32{{return F.Bad()}}fn make(give spare:R,n:i32)->R contract{{result(create)}}{{return R{{code:n}}}}fn replace(s:Store<Cell>)->F!i32{{let v=store_at_mut(s,0);v.owner={rhs};return 0}}fn relay(s:Store<Cell>)->F!i32{{return replace(s)}}fn main()->i32{{{SETUP}match relay(s){{.ok(_)=>{{return 1}},.err(_)=>{{unsafe{{putchar(88)}}return store_at(s,0).owner.code-65}}}}}}"
        );
        replacement_native(&[("main", &source)], expected, 0);
    }
}

#[test]
fn owning_store_capture_and_indirect_keep_their_actual_types_boundaries() {
    for (label, helper, body) in [
        (
            "capture",
            "",
            "let callback=()=>{let v=store_at_mut(s,0);v.owner=R{code:90};return 0};callback();return 0",
        ),
        (
            "capture-old",
            "",
            "let old=store_at(s,0).owner;let callback=()=>{let v=store_at_mut(s,0);v.owner=R{code:90};return 0};callback();return old.code",
        ),
        (
            "indirect-store",
            "fn replace(s:Store<Cell>){let v=store_at_mut(s,0);v.owner=R{code:90}}",
            "let callback=replace;callback(s);return 0",
        ),
        (
            "indirect-product",
            "fn replace(v:&mut Cell){v.owner=R{code:90}}",
            "let v=store_at_mut(s,0);let callback=replace;callback(v);return 0",
        ),
    ] {
        let text = format!("{MODEL}{helper}fn main()->i32{{{SETUP}{body}}}");
        let report = check(&text);
        assert_eq!(
            report.status,
            ResultCategory::Unsupported,
            "{label}: {}",
            report.to_json()
        );
        assert_eq!(report.phase, Phase::Types, "{label}: {}", report.to_json());
        assert!(
            report.diagnostics.iter().any(|d| d.message.contains(
                if label.starts_with("capture") {
                    "Store capture requires an owned carrier"
                } else {
                    "function pointer parameters and result require pure Copy values"
                }
            )),
            "{label}: {}",
            report.to_json()
        );
    }
}

#[test]
fn owning_store_call_raw_and_wire_recompute_instance_invalidation() {
    use zil_ir::InstructionKind as I;
    for (label, helpers, body, called, changed, code) in [
        (
            "store-param",
            "fn replace(s:Store<Cell>){let v=store_at_mut(s,0);v.owner=R{code:90}}",
            "let old=store_at(s,0).owner;let n=old.code;replace(s);return n-65",
            "replace",
            "main",
            "E-BORROW-CONFLICT",
        ),
        (
            "store-relay",
            "fn replace(s:Store<Cell>){let v=store_at_mut(s,0);v.owner=R{code:90}}fn relay(s:Store<Cell>){replace(s)}",
            "let old=store_at(s,0).owner;let n=old.code;relay(s);return n-65",
            "relay",
            "main",
            "E-BORROW-CONFLICT",
        ),
        (
            "product-param",
            "fn replace(v:&mut Cell){v.owner=R{code:90}}",
            "let v=store_at_mut(s,0);let old=v.owner;let n=old.code;replace(v);return n-65",
            "replace",
            "main",
            "E-VERIFY-RESOURCE",
        ),
        (
            "store-param-nonlocal-desc",
            "fn replace(s:Store<Cell>){let v=store_at_mut(s,0);v.owner=R{code:90}}fn caller(s:Store<Cell>,old:R)->i32{let n=old.code;replace(s);return n}",
            "return 0",
            "replace",
            "caller",
            "E-VERIFY-RESOURCE",
        ),
        (
            "store-relay-nonlocal-desc",
            "fn replace(s:Store<Cell>){let v=store_at_mut(s,0);v.owner=R{code:90}}fn relay(s:Store<Cell>){replace(s)}fn caller(s:Store<Cell>,old:R)->i32{let n=old.code;relay(s);return n}",
            "return 0",
            "relay",
            "caller",
            "E-VERIFY-RESOURCE",
        ),
    ] {
        let text = format!("{MODEL}{helpers}fn main()->i32{{{SETUP}{body}}}");
        let (mut module, plan) = candidate(&text);
        let callee = module
            .functions
            .iter()
            .find(|f| f.name == called)
            .unwrap()
            .id;
        let f = module
            .functions
            .iter_mut()
            .find(|f| f.name == changed)
            .unwrap();
        let block = f
            .blocks
            .iter_mut()
            .find(|b| {
                b.instructions
                    .iter()
                    .any(|i| matches!(i.kind, I::Call { function, .. } if function == callee))
            })
            .unwrap();
        let first = block
            .instructions
            .iter()
            .position(|i| matches!(i.kind, I::ReadDesc { .. }))
            .unwrap();
        let end = first
            + block.instructions[first..]
                .iter()
                .position(|i| matches!(i.kind, I::Store { .. }))
                .unwrap()
            + 1;
        let reads = block.instructions.drain(first..end).collect::<Vec<_>>();
        let call = block
            .instructions
            .iter()
            .position(|i| matches!(i.kind, I::Call { function, .. } if function == callee))
            .unwrap();
        block.instructions.splice(call + 1..call + 1, reads);
        let errors = zil_analyze::analyze(&module, &Limits::default()).unwrap_err();
        assert!(
            errors
                .iter()
                .any(|d| d.message.contains("live element resource description")),
            "{label}: {errors:?}"
        );
        reject_candidate(
            label,
            module,
            plan,
            code,
            if code == "E-VERIFY-RESOURCE" {
                "live element resource description"
            } else {
                ""
            },
        );
    }
}

#[test]
fn owning_store_descriptor_arguments_preserve_actual_entry_apart_checks() {
    for body in [
        "let v=store_at_mut(s,0);let old=v.owner;return inspect(v,old)",
        "return 0",
    ] {
        let source = format!(
            "{MODEL}fn replace(v:&mut Cell){{v.owner=R{{code:90}}}}fn inspect(v:&mut Cell,old:R)->i32{{replace(v);return old.code}}fn main()->i32{{{SETUP}{body}}}"
        );
        let report = check(&source);
        if body == "return 0" {
            assert_eq!(
                report.status,
                ResultCategory::Success,
                "{}",
                report.to_json()
            );
        } else {
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
                    .any(|d| d.message.contains("simultaneous actual head arguments")),
                "{}",
                report.to_json()
            );
        }
    }
}

#[test]
fn owning_store_helper_return_boundaries_are_not_instance_proofs() {
    for (label, helper, body) in [
        (
            "window-identity",
            "fn pass(v:&mut Cell)->&mut Cell{return v}",
            "let v=pass(store_at_mut(s,0));v.owner=R{code:90};return 0",
        ),
        (
            "description-return",
            "fn old(v:&Cell)->R{return v.owner}",
            "let v=store_at(s,0);let old=old(v);return old.code",
        ),
        (
            "closure-identity",
            "fn pass<F:Fn()->i32>(f:F)->F{return f}",
            "let old=store_at(s,0).owner;let saved=pass(()=>old.code);let n=saved();return n-65",
        ),
    ] {
        let source = format!("{MODEL}{helper}fn main()->i32{{{SETUP}{body}}}");
        let report = check(&source);
        assert_eq!(
            report.status,
            ResultCategory::Unsupported,
            "{label}: {}",
            report.to_json()
        );
        assert_eq!(report.phase, Phase::Types, "{label}: {}", report.to_json());
        let message = match label {
            "window-identity" => "Store element windows are synchronous inputs",
            "description-return" => "return type Desc",
            _ => "borrowed dependencies cannot escape",
        };
        assert!(
            report
                .diagnostics
                .iter()
                .any(|d| d.message.contains(message)),
            "{label}: {}",
            report.to_json()
        );
    }
}

#[test]
fn owning_store_call_raw_and_wire_reach_saved_closure_description_consumer() {
    use zil_ir::InstructionKind as I;
    let source = format!(
        "{MODEL}fn replace(s:Store<Cell>){{let v=store_at_mut(s,0);v.owner=R{{code:90}}}}fn relay(s:Store<Cell>){{replace(s)}}fn caller(s:Store<Cell>,old:R)->i32{{let saved=()=>old.code;let n=saved();relay(s);return n}}fn main()->i32{{return 0}}"
    );
    let (mut module, plan) = candidate(&source);
    let relay = module
        .functions
        .iter()
        .find(|f| f.name == "relay")
        .unwrap()
        .id;
    let f = module
        .functions
        .iter_mut()
        .find(|f| f.name == "caller")
        .unwrap();
    let block = f
        .blocks
        .iter_mut()
        .find(|b| {
            b.instructions
                .iter()
                .any(|i| matches!(i.kind, I::Call { function, .. } if function == relay))
        })
        .unwrap();
    let first = block
        .instructions
        .iter()
        .position(|i| {
            matches!(
                i.kind,
                I::CallClosureShared { .. }
                    | I::CallClosure { .. }
                    | I::CallClosureBorrowed { .. }
                    | I::CallClosureOnce { .. }
            )
        })
        .unwrap();
    let end = first
        + block.instructions[first..]
            .iter()
            .position(|i| matches!(i.kind, I::Store { .. }))
            .unwrap()
        + 1;
    let reads = block.instructions.drain(first..end).collect::<Vec<_>>();
    let call = block
        .instructions
        .iter()
        .position(|i| matches!(i.kind, I::Call { function, .. } if function == relay))
        .unwrap();
    block.instructions.splice(call + 1..call + 1, reads);
    let errors = zil_analyze::analyze(&module, &Limits::default()).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|d| d.message.contains("live element resource description")),
        "{errors:?}"
    );
    reject_candidate(
        "saved-closure-desc-after-relay",
        module,
        plan,
        "E-VERIFY-RESOURCE",
        "live element resource description",
    );
}

#[test]
fn owning_store_call_invalidation_has_exact_prepaid_fresh_and_warm_boundaries() {
    let text = format!(
        "{MODEL}fn replace(v:&mut Cell){{v.owner=R{{code:90}}}}fn update(s:Store<Cell>){{let v=store_at_mut(s,0);replace(v)}}fn relay(s:Store<Cell>){{update(s)}}fn main()->i32{{{SETUP}relay(s);return store_at(s,0).owner.code-90}}"
    );
    let unit = SourceUnit::new(
        FileId(0),
        ModuleId(0),
        "store-call-invalidation-budget.zil",
        &text,
    );
    // G1.3.6: previous Analyze23731; actual external Commit helper requires
    // exact descriptor origins/point-state and forwarding inventory (+450).
    for (stage, expected_n) in [(Stage::Analyze, 24181), (Stage::Verify, 63080)] {
        let options = |work| CompileOptions {
            stop_after: stage,
            limits: Limits {
                max_analysis_steps: work,
                ..Limits::default()
            },
            ..CompileOptions::default()
        };
        let (mut low, mut high) = (1, Limits::default().max_analysis_steps);
        while low < high {
            let mid = low + (high - low) / 2;
            let report = zil_driver::compile_source(&unit, &options(mid));
            match report.status {
                ResultCategory::Success => high = mid,
                ResultCategory::ResourceLimit => low = mid + 1,
                _ => panic!("{}", report.to_json()),
            }
        }
        assert_eq!(
            low, expected_n,
            "{stage:?}: actual call invalidation work changed"
        );
        let mut warm = zil_driver::CompilationSession::default();
        for (work, expected) in [
            (low, ResultCategory::Success),
            (low - 1, ResultCategory::ResourceLimit),
            (low, ResultCategory::Success),
        ] {
            for report in [
                zil_driver::compile_source(&unit, &options(work)),
                warm.compile_source(&unit, &options(work)),
            ] {
                assert_eq!(report.status, expected, "{}", report.to_json());
                if expected == ResultCategory::ResourceLimit {
                    assert_eq!(
                        report.phase,
                        if stage == Stage::Analyze {
                            Phase::Analyze
                        } else {
                            Phase::Verify
                        },
                        "{}",
                        report.to_json()
                    );
                } else if stage == Stage::Verify {
                    assert_eq!(report.verification_runs, 1);
                }
            }
            assert_eq!(warm.active_query_count(), 0);
        }
        println!(
            "G1.3.2 {stage:?} full-request N={low}; N-1 ResourceLimit; fresh/warm/retry agree"
        );
    }
}

#[test]
fn owning_store_borrowed_closure_raw_and_wire_reach_descriptor_consumer() {
    use zil_ir::InstructionKind as I;
    let source = format!(
        "{MODEL}fn replace(s:Store<Cell>){{let v=store_at_mut(s,0);v.owner=R{{code:90}}}}fn relay(s:Store<Cell>){{replace(s)}}fn caller(s:Store<Cell>,old:R)->i32{{let saved=()=>old.code;let borrowed=&saved;let n=borrowed();relay(s);return n}}fn main()->i32{{return 0}}"
    );
    let (mut module, plan) = candidate(&source);
    let relay = module
        .functions
        .iter()
        .find(|f| f.name == "relay")
        .unwrap()
        .id;
    let f = module
        .functions
        .iter_mut()
        .find(|f| f.name == "caller")
        .unwrap();
    let block = f
        .blocks
        .iter_mut()
        .find(|b| {
            b.instructions
                .iter()
                .any(|i| matches!(i.kind,I::Call{function,..}if function==relay))
        })
        .unwrap();
    let first = block
        .instructions
        .iter()
        .position(|i| matches!(i.kind, I::CallClosureBorrowed { .. }))
        .unwrap();
    let end = first
        + block.instructions[first..]
            .iter()
            .position(|i| matches!(i.kind, I::Store { .. }))
            .unwrap()
        + 1;
    let reads = block.instructions.drain(first..end).collect::<Vec<_>>();
    let call = block
        .instructions
        .iter()
        .position(|i| matches!(i.kind,I::Call{function,..}if function==relay))
        .unwrap();
    block.instructions.splice(call + 1..call + 1, reads);
    let errors = zil_analyze::analyze(&module, &Limits::default()).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|d| d.message.contains("live element resource description")),
        "{errors:?}"
    );
    reject_candidate(
        "borrowed-closure-desc-after-relay",
        module,
        plan,
        "E-VERIFY-RESOURCE",
        "live element resource description",
    );
}

#[test]
fn owning_store_borrowed_closure_carriers_preserve_last_use_and_other_store() {
    let helper = "fn replace(s:Store<Cell>){let v=store_at_mut(s,0);v.owner=R{code:90}}fn relay(s:Store<Cell>){replace(s)}";
    let before = format!(
        "{MODEL}{helper}fn main()->i32{{{SETUP}let old=store_at(s,0).owner;let saved=()=>old.code;let borrowed=&saved;let n=borrowed();relay(s);return n-65}}"
    );
    replacement_native(&[("main", &before)], b"AZB", 0);
    let other = format!(
        "{MODEL}{helper}fn main()->i32{{{SETUP}let t=empty();store_reserve(t,1);store_push(t,cell(67));let old=store_at(t,0).owner;let saved=()=>old.code;let borrowed=&saved;relay(s);return borrowed()-67}}"
    );
    replacement_native(&[("main", &other)], b"ACZB", 0);
    let after = format!(
        "{MODEL}{helper}fn main()->i32{{{SETUP}let old=store_at(s,0).owner;let saved=()=>old.code;let borrowed=&saved;relay(s);return borrowed()-65}}"
    );
    rejected(
        &after,
        ResultCategory::SafeReject,
        Phase::Analyze,
        "live element resource description",
    );
    let nested = format!(
        "{MODEL}{helper}fn caller(s:Store<Cell>,old:R)->i32{{let saved=()=>old.code;let wrapped=()=>saved();let n=wrapped();relay(s);return n}}fn main()->i32{{return 0}}"
    );
    rejected(
        &nested,
        ResultCategory::Unsupported,
        Phase::Analyze,
        "transitive carrier bridge",
    );
    let plain = format!(
        "{MODEL}{helper}fn main()->i32{{{SETUP}let saved=()=>1;let wrapped=()=>saved();relay(s);return wrapped()-1}}"
    );
    replacement_native(&[("main", &plain)], b"AZB", 0);
}
