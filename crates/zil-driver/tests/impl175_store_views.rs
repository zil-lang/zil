//! Real indexed Store windows, independently checked invalidation and native bounds.
use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    path::PathBuf,
    process::{Command, Stdio},
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
        "store-views175-{}-{}",
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
        dirs.push(dir);
    }
    dirs
}

const PACKAGE: &str = r#"
extern "C" fn putchar(x:i32)->i32;
resource R{code:i32}
impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
package P{manages r:R;n:i32}
fn make(code:i32)->P=>P{r:R{code:code},n:code}
fn empty()->Store<P> contract{result(create)}{return store_new()}
fn read(p:&P)->i32=>p.n
"#;
const SETUP: &str =
    "let s=empty();store_reserve(s,2);store_push(s,make(65));store_push(s,make(66));";

#[test]
fn shared_element_windows_cross_functions_and_keep_one_owner() {
    let source = format!(
        "{PACKAGE} fn main()->i32{{{SETUP}let a=store_at(s,0);let b=store_at(s,1);return read(a)+read(b)-131}}"
    );
    let dirs = native(&[("main", &source)], b"AB", 0);
    for directory in dirs {
        let ir = fs::read_to_string(directory.join("phases/llvm.ll")).unwrap();
        assert!(ir.contains("store.element.ok."));
        assert!(ir.contains("icmp ult i64"));
        assert!(!ir.contains(" noalias "));
    }
}

#[test]
fn direct_projection_and_last_use_allow_later_reserve_and_pop() {
    let source = format!(
        "{PACKAGE} fn main()->void!i32{{{SETUP}let first=store_at(s,0).n;let last=store_at(s,1);let value=read(last);if !store_reserve(s,4){{return ()}};let tail=store_pop(s)?;return first+value+tail.n-197}}"
    );
    native(&[("main", &source)], b"BA", 0);
}

#[test]
fn shared_views_allow_len_capacity_and_unrelated_address_return() {
    let source = format!(
        "{PACKAGE} fn id(x:&i32)->&i32{{return x}}fn main()->i32{{{SETUP}let view=store_at(s,0);let n:i32=1;let value=*id(&n);return read(view)+value+i32(store_len(s)+store_capacity(s))-70}}"
    );
    native(&[("main", &source)], b"AB", 0);
}

#[test]
fn copy_child_address_keeps_a_live_window_through_local_callback() {
    let source = format!(
        "{PACKAGE} fn main()->i32{{{SETUP}let view=store_at(s,0);let child=&view.n;let callback=()=>*child;let value=callback();store_reserve(s,4);return value-65}}"
    );
    native(&[("main", &source)], b"AB", 0);
}

fn reject_source(
    source: &str,
    category: ResultCategory,
    phase: zil_base::Phase,
    code: &str,
    message: &str,
) {
    let result = check(source);
    assert_eq!(result.status, category, "{}", result.to_json());
    assert!(
        result
            .diagnostics
            .iter()
            .any(|d| d.phase == phase && d.code == code && d.message.contains(message)),
        "{}",
        result.to_json()
    );
}

#[test]
fn every_intersecting_container_mutation_is_rejected_while_view_is_live() {
    for change in [
        "store_reserve(s,4);",
        "store_push(s,make(67));",
        "store_pop(s);",
        "consume(give s);",
    ] {
        let source = format!(
            "{PACKAGE} fn consume(give x:Store<P>){{}}fn main()->i32{{{SETUP}let view=store_at(s,0);{change}return read(view)}}"
        );
        reject_source(
            &source,
            ResultCategory::SafeReject,
            zil_base::Phase::Verify,
            "E-BORROW-CONFLICT",
            "",
        );
    }
    let source = format!(
        "{PACKAGE} fn main()->i32{{{SETUP}let view=store_at(s,0);let child=&view.n;let callback=()=>*child;store_reserve(s,4);return callback()}}"
    );
    reject_source(
        &source,
        ResultCategory::SafeReject,
        zil_base::Phase::Verify,
        "E-BORROW-CONFLICT",
        "",
    );
}

#[test]
fn indirect_alias_contract_is_instantiated_from_real_store_roots() {
    let helpers = "fn change(x:Store<P>){store_reserve(x,4)}fn use_pair(a:Store<P>,b:Store<P>)->i32{let view=store_at(a,0);change(b);return read(view)}";
    let source = format!("{PACKAGE}{helpers}fn main()->i32{{{SETUP}return use_pair(s,s)}}");
    reject_source(
        &source,
        ResultCategory::SafeReject,
        zil_base::Phase::Verify,
        "E-BORROW-CONFLICT",
        "alias",
    );
    let source = format!(
        "{PACKAGE}{helpers}fn main()->i32{{{SETUP}let other=empty();return use_pair(s,other)-65}}"
    );
    native(&[("main", &source)], b"AB", 0);
}

#[test]
fn view_and_descriptor_input_aliasing_cannot_hide_a_write_inside_helper() {
    let helper = "fn use_pair(view:&P,s:Store<P>)->i32{store_reserve(s,4);return view.n}";
    let source = format!(
        "{PACKAGE}{helper}fn main()->i32{{{SETUP}let view=store_at(s,0);return use_pair(view,s)}}"
    );
    reject_source(
        &source,
        ResultCategory::SafeReject,
        zil_base::Phase::Verify,
        "E-BORROW-CONFLICT",
        "alias",
    );
    let source = format!(
        "{PACKAGE}{helper}fn main()->i32{{{SETUP}let other=empty();let view=store_at(s,0);return use_pair(view,other)-65}}"
    );
    native(&[("main", &source)], b"AB", 0);
}

#[test]
fn element_shape_mutable_owned_field_return_and_capture_guards_are_explicit() {
    use zil_base::Phase::Types;
    for tail in [
        "fn bad(x:&P)->&P{return x}fn main(){}",
        "fn bad(x:&P)->&i32{return &x.n}fn main(){}",
        "fn bad(s:Store<P>)->&i32{let x=store_at(s,0);return &x.n}fn main(){}",
    ] {
        reject_source(
            &format!("{PACKAGE}{tail}"),
            ResultCategory::Unsupported,
            Types,
            "E-UNSUPPORTED",
            "Store element windows are synchronous",
        );
    }
    reject_source(
        &format!(
            "{PACKAGE} fn main()->i32{{{SETUP}let view=store_at(s,0);let callback=()=>view.n;return callback()}}"
        ),
        ResultCategory::Unsupported,
        Types,
        "E-UNSUPPORTED",
        "Store element address capture",
    );
    reject_source(
        &format!("{PACKAGE} fn main()->i32{{{SETUP}let view=store_at(s,0);return view.r.code}}"),
        ResultCategory::Unsupported,
        Types,
        "E-UNSUPPORTED",
        "only project finite Copy fields",
    );
    reject_source(
        &format!(
            "{PACKAGE} fn main()->i32{{{SETUP}let view=store_at(s,0);view.n=9;return view.n}}"
        ),
        ResultCategory::SafeReject,
        Types,
        "E-ADDRESS-PERMISSION",
        "exclusive",
    );
    reject_source(
        &format!("{PACKAGE} fn bad(x:&mut P){{}}fn main(){{}}"),
        ResultCategory::Unsupported,
        Types,
        "E-UNSUPPORTED",
        "head addresses require",
    );
    reject_source(
        &format!("{PACKAGE} fn main()->i32{{let value=make(65);return read(&value)}}"),
        ResultCategory::Unsupported,
        Types,
        "E-UNSUPPORTED",
        "",
    );
}

#[test]
fn element_read_respects_actual_package_field_visibility() {
    let files = [
        (
            "main",
            "import model; fn read(x:&model.P)->i32=>x.secret; fn main()->i32=>0",
        ),
        (
            "model",
            "pub resource R{} pub package P{manages r:R;secret:i32}",
        ),
    ];
    let result = compile_project(&project(&files), &CompileOptions::default());
    assert_eq!(
        result.status,
        ResultCategory::SafeReject,
        "{}",
        result.to_json()
    );
    assert!(
        result
            .diagnostics
            .iter()
            .any(|d| d.phase == zil_base::Phase::Types && d.code == "E-FIELD-PRIVATE"),
        "{}",
        result.to_json()
    );
}

#[test]
fn runtime_negative_empty_and_end_indices_trap_before_element_access() {
    for (setup, index) in [("let s=empty();", 0), (SETUP, -1), (SETUP, 2)] {
        let source = format!("{PACKAGE} fn main()->i32{{{setup}return store_at(s,{index}).n}}");
        for optimization in [Optimization::O0, Optimization::O2] {
            let dir = directory();
            fs::write(dir.join("main.zil"), &source).unwrap();
            let executable = dir.join("program");
            let report = compile_project(
                &project(&[("main", &source)]),
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
            let result = Command::new(&executable).output().unwrap();
            fs::write(dir.join("native.txt"), format!("{result:?}\n")).unwrap();
            assert!(!result.status.success());
            #[cfg(unix)]
            assert_eq!(
                std::os::unix::process::ExitStatusExt::signal(&result.status),
                Some(4),
                "{result:?}"
            );
            assert!(result.stdout.is_empty());
            assert!(result.stderr.is_empty());
        }
    }
}

fn candidate(source: &str) -> (zil_ir::TypedModule, zil_ir::CandidatePlan) {
    let source = SourceUnit::new(FileId(0), ModuleId(0), "raw175.zil", source);
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
                .any(|d| d.code == code && d.message.contains(message)),
            "{label}/{route}: {errors:?}"
        );
    }
}

#[test]
fn raw_and_wire_rebuild_element_type_index_and_hold_origins() {
    use zil_ir::{InstructionKind as I, Type};
    let source =
        format!("{PACKAGE} fn main()->i32{{{SETUP}let view=store_at(s,0);return read(view)}}");
    let (module, plan) = candidate(&source);
    let f = module
        .functions
        .iter()
        .position(|f| f.name == "main")
        .unwrap();
    let (block, position, store, index, output) = module.functions[f]
        .blocks
        .iter()
        .enumerate()
        .find_map(|(b, block)| {
            block
                .instructions
                .iter()
                .enumerate()
                .find_map(|(i, instruction)| match instruction.kind {
                    I::OwnedStoreBorrow { store, index } => {
                        Some((b, i, store, index, instruction.result.unwrap()))
                    }
                    _ => None,
                })
        })
        .unwrap();
    let mut bad = module.clone();
    bad.functions[f].blocks[block].instructions[position].kind = I::OwnedStoreBorrow {
        store: index,
        index,
    };
    reject_candidate(
        "wrong-carrier",
        bad,
        plan.clone(),
        "E-VERIFY-OWNED-STORE",
        "description",
    );
    let mut bad = module.clone();
    bad.functions[f].blocks[block].instructions[position].kind = I::OwnedStoreBorrow {
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
    bad.functions[f].values[output.0 as usize] = Type::Address {
        mutable: false,
        inner: Box::new(Type::I32),
    };
    reject_candidate(
        "wrong-result-pointee",
        bad,
        plan.clone(),
        "E-VERIFY-TYPE",
        "",
    );
    let mut forged = plan.clone();
    forged.functions[f]
        .holds
        .iter_mut()
        .find(|hold| hold.value == output)
        .unwrap()
        .roots
        .clear();
    reject_candidate("erased-hold", module.clone(), forged, "E-VERIFY-HOLD", "");
    let mut bad = module.clone();
    let field = bad
        .functions
        .iter_mut()
        .find_map(|f| {
            f.blocks.iter_mut().find_map(|b| {
                b.instructions
                    .iter_mut()
                    .find(|i| matches!(i.kind, I::AddressField { .. }))
            })
        })
        .unwrap();
    let I::AddressField { address, .. } = field.kind else {
        unreachable!()
    };
    field.kind = I::ReadAddress { address };
    reject_candidate(
        "copy-entire-package",
        bad,
        plan,
        "E-VERIFY-ADDRESS",
        "finite Copy",
    );
}

#[test]
fn raw_and_wire_reject_write_retargeted_behind_an_active_window() {
    use zil_ir::InstructionKind as I;
    let source = format!(
        "{PACKAGE} fn main()->i32{{{SETUP}let other=empty();let view=store_at(s,0);store_reserve(other,4);return read(view)}}"
    );
    let (mut module, plan) = candidate(&source);
    let f = module
        .functions
        .iter_mut()
        .find(|f| f.name == "main")
        .unwrap();
    let (block, position, store) = f
        .blocks
        .iter()
        .enumerate()
        .find_map(|(b, block)| {
            block
                .instructions
                .iter()
                .enumerate()
                .find_map(|(i, instruction)| match instruction.kind {
                    I::OwnedStoreBorrow { store, .. } => Some((b, i, store)),
                    _ => None,
                })
        })
        .unwrap();
    let call = f.blocks[block].instructions[position + 1..]
        .iter_mut()
        .find(|i| matches!(i.kind, I::OwnedStoreReserve { .. }))
        .unwrap();
    let I::OwnedStoreReserve { capacity, .. } = call.kind else {
        unreachable!()
    };
    call.kind = I::OwnedStoreReserve { store, capacity };
    reject_candidate("retargeted-write", module, plan, "E-BORROW-CONFLICT", "");
}

#[test]
fn verify_window_budget_has_an_exact_n_and_n_minus_one_boundary() {
    let source =
        format!("{PACKAGE} fn main()->i32{{{SETUP}let view=store_at(s,0);return read(view)}}");
    let (module, plan) = candidate(&source);
    let target = TargetProfile::host_llvm();
    let probe = |n| {
        let limits = Limits {
            max_analysis_steps: n,
            ..Limits::default()
        };
        zil_verify::verify_module(module.clone(), plan.clone(), &target, &limits)
    };
    let (mut lo, mut hi) = (0, Limits::default().max_analysis_steps);
    assert!(probe(hi).is_ok());
    while lo + 1 < hi {
        let mid = lo + (hi - lo) / 2;
        if probe(mid).is_ok() {
            hi = mid
        } else {
            lo = mid
        }
    }
    assert!(probe(hi).is_ok());
    let errors = probe(hi - 1).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|d| d.category == ResultCategory::ResourceLimit)
    );
    let dir = directory();
    fs::write(dir.join("boundary.json"),format!("{{\"pass_n\":{hi},\"resource_limit_n_minus_1\":{},\"default_limits_changed\":false}}\n",hi-1)).unwrap();
    fs::write(
        dir.join("candidate.bin"),
        zil_ir::wire::encode_module_candidate(&module, &plan, &Limits::default()).unwrap(),
    )
    .unwrap();
}

const MODEL: &str = include_str!("../../../examples/mainline/impl175/model.zil");
const STORAGE: &str = include_str!("../../../examples/mainline/impl175/storage.zil");
const INSPECTION: &str = include_str!("../../../examples/mainline/impl175/inspection.zil");
const MAIN: &str = include_str!("../../../examples/mainline/impl175/main.zil");

#[test]
fn runtime_index_reads_real_metadata_through_four_source_modules() {
    let files = [
        ("main", MAIN),
        ("model", MODEL),
        ("storage", STORAGE),
        ("inspection", INSPECTION),
    ];
    for (input, output, exit) in [
        (b"00".as_slice(), b"-".as_slice(), 0),
        (b"10", b"aA", 0),
        (b"30", b"aABC", 0),
        (b"32", b"cABC", 0),
        (b"41", b"bABCD", 0),
        (b"44", b"", 9),
    ] {
        native_input(&files, input, output, exit);
    }
}
