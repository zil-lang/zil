//! Independent review of root177's template admission and composed loan wiring.
//! This does not independently re-approve reviewer-authored174 Store semantics.
use std::{
    collections::BTreeMap,
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_base::{FileId, Limits, ModuleId, Phase, ResultCategory, SourceUnit, TargetProfile};
use zil_driver::{CompileOptions, Optimization, ProjectSources, Stage, compile_project};

static NEXT: AtomicUsize = AtomicUsize::new(0);

fn directory(label: &str) -> PathBuf {
    let root = std::env::var_os("ZIL_TEST_REPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let path = root.join(format!(
        "review177-{label}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&path).unwrap();
    path
}

fn project(files: &[(&str, &str)]) -> ProjectSources {
    ProjectSources {
        root: ModuleId(0),
        modules: files
            .iter()
            .enumerate()
            .map(|(i, (name, source))| {
                SourceUnit::new(
                    FileId(i as u32),
                    ModuleId(i as u32),
                    format!("{name}.zil"),
                    *source,
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

fn reject(
    files: &[(&str, &str)],
    label: &str,
    category: ResultCategory,
    phase: Phase,
    code: &str,
    message: &str,
) {
    let dir = directory(label);
    for (name, source) in files {
        fs::write(dir.join(format!("{name}.zil")), source).unwrap();
    }
    let report = compile_project(
        &project(files),
        &CompileOptions {
            emit_dir: Some(dir.join("phases")),
            ..CompileOptions::default()
        },
    );
    fs::write(dir.join("compile.json"), report.to_json()).unwrap();
    assert_eq!(report.status, category, "{label}: {}", report.to_json());
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.phase == phase && d.code == code && d.message.contains(message)),
        "{label}: {}",
        report.to_json()
    );
}

fn statistic(text: &str, key: &str) -> usize {
    text.split_once(&format!("\"{key}\":"))
        .unwrap()
        .1
        .split([',', '}'])
        .next()
        .unwrap()
        .trim()
        .parse()
        .unwrap()
}

fn accept(source: &str, label: &str) {
    let dir = directory(label);
    fs::write(dir.join("main.zil"), source).unwrap();
    let report = compile_project(&project(&[("main", source)]), &CompileOptions::default());
    fs::write(dir.join("compile.json"), report.to_json()).unwrap();
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1);
}

fn native(source: &str, label: &str, expected: &[u8], allocations: Option<usize>) {
    for opt in [Optimization::O0, Optimization::O2] {
        let dir = directory(label);
        fs::write(dir.join("main.zil"), source).unwrap();
        let executable = dir.join("program");
        let report = compile_project(
            &project(&[("main", source)]),
            &CompileOptions {
                stop_after: Stage::Build,
                optimization: opt,
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
        let run = Command::new(&executable).output().unwrap();
        fs::write(dir.join("stdout.log"), &run.stdout).unwrap();
        fs::write(dir.join("stderr.log"), &run.stderr).unwrap();
        assert_eq!(run.status.code(), Some(0), "{run:?}");
        assert_eq!(run.stdout, expected);
        assert!(run.stderr.is_empty());
        if let Some(allocations) = allocations {
            let probe = dir.join("allocation-probe");
            let shim = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../tools/language/173/allocator_probe.c");
            let linked = Command::new("clang")
                .arg(match opt {
                    Optimization::O0 => "-O0",
                    _ => "-O2",
                })
                .arg(dir.join("phases/llvm.ll"))
                .arg(shim)
                .args(["-Wl,--wrap=realloc", "-Wl,--wrap=free", "-o"])
                .arg(&probe)
                .output()
                .unwrap();
            fs::write(dir.join("link.stdout.log"), &linked.stdout).unwrap();
            fs::write(dir.join("link.stderr.log"), &linked.stderr).unwrap();
            assert!(linked.status.success(), "{linked:?}");
            let run = Command::new(probe)
                .env("ZIL_TEST_FAIL_AT", "0")
                .output()
                .unwrap();
            fs::write(dir.join("allocation.stdout.log"), &run.stdout).unwrap();
            fs::write(dir.join("allocation.stderr.log"), &run.stderr).unwrap();
            assert_eq!(run.status.code(), Some(0), "{run:?}");
            assert_eq!(run.stdout, expected);
            let report = std::str::from_utf8(&run.stderr).unwrap();
            assert_eq!(statistic(report, "allocated"), allocations);
            assert_eq!(statistic(report, "frees"), allocations);
            assert_eq!(statistic(report, "live"), 0);
            assert_eq!(statistic(report, "failures"), 0);
        }
    }
}

const MODEL: &str = r#"
extern "C" fn putchar(n:i32)->i32;
resource Tag { code:i32 }
impl Drop for Tag { fn drop(self:Self) { unsafe { putchar(.code) } } }
package Cell { manages tag:Tag; value:i32 }
package Batch { manages list:Store<Cell>; count:i32 }
package Panel<C> { manages tag:Tag; manages list:Store<Cell>; callback:C; hits:i32 }
fn batch(code:i32)->Batch {
    let list:Store<Cell>=store_new();
    if store_reserve(list,1) { store_push(list,Cell{tag:Tag{code:code},value:code}) }
    let count=i32(store_len(list));
    return Batch{list:give list,count:count}
}
fn count(list:Store<Cell>)->i32 { return i32(store_len(list)) }
fn consume(batch:Batch)->i32 { return batch.count }
fn handler()->impl FnMut()->i32 {
    let mark=Tag{code:87}; var calls=0;
    return move()=>{calls+=1;return calls+mark.code-87}
}
fn panel<C:FnMut()->i32>(callback:C)->Panel<C> {
    let prepared=batch(65);
    return Panel::<C>{tag:Tag{code:88},list:give prepared.list,callback:callback,hits:0}
}
"#;

#[test]
fn unused_template_keeps_field_identity_and_rejects_conditional_partial_publication() {
    let extra = "package Pair<C>{manages tag:Tag;manages left:Store<Cell>;manages right:Store<Cell>;callback:C}";
    for (label, function, message) in [
        (
            "wrong-field-refill",
            "fn unused<C:FnMut()->i32>(input:Pair<C>)->Pair<C>{var next=input;let left=give next.left;let prepared=batch(66);next.right=give prepared.list;return next}",
            "partial callable State must be refilled",
        ),
        (
            "conditional-missing",
            "fn unused<C:FnMut()->i32>(input:Panel<C>,take:bool)->Panel<C>{if take{let removed=give input.list}return input}",
            "partial callable State must be refilled",
        ),
        (
            "aliased-concrete-owner",
            "fn unused<C:FnMut()->i32>(input:Panel<C>)->Panel<C>{let original=batch(66);let alias=original;let taken=give alias.list;consume(alias);return input}",
            "partial callable State must be refilled",
        ),
        (
            "moved-managed-read",
            "fn unused<C:FnMut()->i32>(input:Panel<C>)->i32{let original=batch(66);let taken=give original.list;return count(original.list)}",
            "managed Package field used after give",
        ),
    ] {
        let source = format!("{MODEL}{extra}{function}fn main()->i32=>0");
        reject(
            &[("main", &source)],
            label,
            ResultCategory::SafeReject,
            Phase::Expand,
            "E-GENERIC-ARGUMENT",
            message,
        );
    }
}

#[test]
fn borrowed_and_nonfield_values_do_not_get_the_new_give_capability() {
    for (label, function, message) in [
        (
            "borrowed-parent",
            "fn unused<C:FnMut()->i32>(input:Panel<C>,b:&Batch)->i32{let result=give b.list;return 0}",
            "local owning Package",
        ),
        (
            "borrowed-deref-alias",
            "fn unused<C:FnMut()->i32>(input:Panel<C>,b:&Batch)->i32{let alias=*b;let result=give alias.list;return 0}",
            "requires a declared bound",
        ),
        (
            "direct-store-descriptor",
            "fn unused<C:FnMut()->i32>(input:Panel<C>,list:Store<Cell>)->i32{let result=give list;return 0}",
            "descriptors do not own",
        ),
        (
            "arbitrary-generic-give",
            "fn unused<T>(value:T)->T{return give value}",
            "owning State profile",
        ),
    ] {
        let source = format!("{MODEL}{function}fn main()->i32=>0");
        reject(
            &[("main", &source)],
            label,
            if label == "direct-store-descriptor" {
                ResultCategory::SafeReject
            } else {
                ResultCategory::Unsupported
            },
            Phase::Expand,
            if label == "direct-store-descriptor" {
                "E-GENERIC-ARGUMENT"
            } else {
                "E-UNSUPPORTED"
            },
            message,
        );
    }
    let library = "pub resource Token{n:i32}pub package Bag{manages token:Token}";
    let source = format!(
        "{MODEL}import hidden;fn unused<C:FnMut()->i32>(input:Panel<C>,bag:hidden.Bag)->i32{{let stolen=give bag.token;return 0}}fn main()->i32=>0"
    );
    reject(
        &[("main", &source), ("hidden", library)],
        "private-manages",
        ResultCategory::SafeReject,
        Phase::Expand,
        "E-FIELD-PRIVATE",
        "private",
    );
}

#[test]
fn concrete_resource_give_refill_keeps_actual_callback_state() {
    let source = format!(
        "{MODEL}fn replace<C:FnMut()->i32>(input:Panel<C>)->Panel<C>{{var next=input;let old=give next.tag;next.tag=Tag{{code:89}};let callback=&mut next.callback;let first=callback();next.hits=callback();return next}}fn main()->i32{{let current=replace(panel(handler()));return current.hits-2}}"
    );
    native(&source, "resource-leaf", b"XYAW", None);
}

#[test]
fn concrete_byte_buffer_give_refill_releases_both_actual_allocations() {
    let source = format!(
        r#"{MODEL}
package Bytes {{ manages data:ByteBuffer }}
package BytePanel<C> {{ manages tag:Tag; manages data:ByteBuffer; callback:C; hits:i32 }}
fn bytes()->Bytes {{ let data=bytes_new();bytes_reserve(data,1);bytes_push(data,9u8);return Bytes{{data:give data}} }}
fn build<C:FnMut()->i32>(callback:C)->BytePanel<C> {{ let b=bytes();return BytePanel::<C>{{tag:Tag{{code:88}},data:give b.data,callback:callback,hits:0}} }}
fn replace<C:FnMut()->i32>(input:BytePanel<C>)->BytePanel<C> {{ var next=input;let old=give next.data;let b=bytes();next.data=give b.data;let callback=&mut next.callback;next.hits=callback();return next }}
fn main()->i32 {{ var current=replace(build(handler()));let callback=&mut current.callback;if callback()!=2{{return 1}}return i32(bytes_len(current.data))-1 }}
"#
    );
    native(&source, "byte-buffer-leaf", b"XW", Some(2));
}

#[test]
fn returning_partial_branch_does_not_poison_the_complete_branch() {
    let source = format!(
        "{MODEL}fn finish<C:FnMut()->i32>(input:Panel<C>,take:bool)->i32{{if take{{let removed=give input.list;return 0}}return count(input.list)-1}}fn main()->i32{{let a=finish(panel(handler()),true);let b=finish(panel(handler()),false);return a+b}}"
    );
    native(&source, "returning-branch", b"AXWXAW", None);
}

fn last_read_source() -> String {
    format!(
        "{MODEL}fn main()->i32{{var current=panel(handler());let view=store_at(current.list,0);let value=view.value;let callback=&mut current.callback;let first=callback();let second=callback();return value-65+first-1+second-2}}"
    )
}

#[test]
fn last_store_read_can_end_before_mutable_callback_qualification() {
    native(&last_read_source(), "last-read", b"XAW", None);
}

#[test]
fn independent_callback_field_is_allowed_while_whole_parent_move_stays_closed() {
    for (label, mutation) in [
        (
            "callback-parent-root",
            "let callback=&mut current.callback;let called=callback();",
        ),
        ("whole-parent-move", "let next=current;"),
    ] {
        let source = format!(
            "{MODEL}fn main()->i32{{var current=panel(handler());let view=store_at(current.list,0);{mutation}return view.value}}"
        );
        if label == "callback-parent-root" {
            accept(&source, label);
            continue;
        }
        reject(
            &[("main", &source)],
            label,
            ResultCategory::SafeReject,
            Phase::Verify,
            "E-BORROW-CONFLICT",
            "",
        );
    }
    let source = format!(
        "{MODEL}fn main()->i32{{var current=panel(handler());let callback=&mut current.callback;let moved=current;return callback()}}"
    );
    reject(
        &[("main", &source)],
        "callback-window-parent-move",
        ResultCategory::SafeReject,
        Phase::Verify,
        "E-BORROW-CONFLICT",
        "",
    );
}

#[test]
fn callback_field_qualification_allows_sibling_copy_write_before_its_last_use() {
    // This is the original independent03 positive fixture, retained verbatim.
    // 179 proves this Copy field disjoint from the real callback field.
    let source = format!(
        "{MODEL}fn replace<C:FnMut()->i32>(input:Panel<C>)->Panel<C>{{var next=input;let old=give next.tag;next.tag=Tag{{code:89}};let callback=&mut next.callback;next.hits=callback();next.hits=callback();return next}}fn main()->i32{{let current=replace(panel(handler()));return current.hits-2}}"
    );
    native(&source, "callback-live-parent-copy-write", b"XYAW", None);
}

#[test]
fn independent_raw_and_wire_rebuild_both_composed_loan_kinds() {
    let source = last_read_source();
    let unit = SourceUnit::new(FileId(0), ModuleId(0), "review177-raw.zil", source.clone());
    let syntax = zil_syntax::parse(&unit).into_result().unwrap();
    let core = zil_expand::expand(&unit, &syntax).unwrap();
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    let mut module = zil_types::check(&core, &target, &limits).unwrap();
    let mut plan = zil_analyze::analyze(&module, &limits).unwrap();
    zil_verify::verify_module(module.clone(), plan.clone(), &target, &limits).unwrap();
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
                .any(|i| matches!(i.kind, zil_ir::InstructionKind::BorrowPackageField { .. }))
        })
        .unwrap();
    let callback = block
        .instructions
        .iter()
        .position(|i| matches!(i.kind, zil_ir::InstructionKind::BorrowPackageField { .. }))
        .unwrap();
    let operation = block.instructions.remove(callback);
    let element = block
        .instructions
        .iter()
        .position(|i| matches!(i.kind, zil_ir::InstructionKind::OwnedStoreBorrow { .. }))
        .unwrap();
    block.instructions.insert(element + 1, operation);
    plan.module_fingerprint = module.fingerprint();
    let bytes = zil_ir::wire::encode_module_candidate(&module, &plan, &limits).unwrap();
    let dir = directory("raw-composed-loans");
    fs::write(dir.join("main.zil"), source).unwrap();
    fs::write(dir.join("candidate.bin"), &bytes).unwrap();
    // The exact original instruction reorder now proves a valid pair of
    // sibling-field loans. Hold/root claims still undergo independent checks.
    for (route, result) in [
        (
            "raw",
            zil_verify::verify_module(module.clone(), plan.clone(), &target, &limits),
        ),
        (
            "wire",
            zil_verify::verify_module_bytes(&bytes, &target, &limits),
        ),
    ] {
        let verified = result.unwrap();
        fs::write(
            dir.join(format!("{route}-accepted.txt")),
            format!("Success {:?}\n", verified.fingerprint()),
        )
        .unwrap();
    }
    // Moving cleanup of their common parent before the last uses must still
    // fail. This is the nearest invalid operation to the valid field reorder.
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
                .any(|i| matches!(i.kind, zil_ir::InstructionKind::BorrowPackageField { .. }))
        })
        .unwrap();
    let callback = block
        .instructions
        .iter()
        .position(|i| matches!(i.kind, zil_ir::InstructionKind::BorrowPackageField { .. }))
        .unwrap();
    let zil_ir::InstructionKind::BorrowPackageField { local, .. } =
        block.instructions[callback].kind
    else {
        unreachable!()
    };
    let cleanup = block.instructions.iter().position(|i|
        matches!(i.kind, zil_ir::InstructionKind::DropPackage { local: owner } if owner == local)).unwrap();
    let operation = block.instructions.remove(cleanup);
    block.instructions.insert(callback + 1, operation);
    plan.module_fingerprint = module.fingerprint();
    let bytes = zil_ir::wire::encode_module_candidate(&module, &plan, &limits).unwrap();
    fs::write(dir.join("early-parent-cleanup.bin"), &bytes).unwrap();
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
        assert!(
            errors.iter().any(|d| d.phase == Phase::Verify
                && d.category == ResultCategory::SafeReject
                && d.code == "E-BORROW-CONFLICT"),
            "{errors:?}"
        );
    }
}
