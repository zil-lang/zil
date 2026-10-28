//! Real source controls for finite owned leaf-Store captures.
use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_base::{FileId, ModuleId, Phase, ResultCategory, SourceUnit};
use zil_driver::{
    CompileOptions, CompileReport, Optimization, ProjectSources, Stage, compile_project,
};

static NEXT: AtomicUsize = AtomicUsize::new(0);
const ORIGINALS: [&str; 4] = [
    include_str!("impl204_fixtures/00_package_owned_control.zil"),
    include_str!("impl204_fixtures/01_direct_store_capture.zil"),
    include_str!("impl204_fixtures/02_original201_package_store_capture.zil"),
    include_str!("impl204_fixtures/03_mutating_store_capture.zil"),
];

fn compile(label: &str, source: &str, stage: Stage) -> (PathBuf, CompileReport) {
    compile_optimized(label, source, stage, Optimization::O0)
}

fn compile_optimized(
    label: &str,
    source: &str,
    stage: Stage,
    optimization: Optimization,
) -> (PathBuf, CompileReport) {
    let root = std::env::var_os("ZIL_TEST_REPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let directory = root.join(format!(
        "capture204-{label}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&directory).unwrap();
    fs::write(directory.join("source.zil"), source).unwrap();
    let report = compile_project(
        &ProjectSources {
            root: ModuleId(0),
            modules: vec![SourceUnit::new(FileId(0), ModuleId(0), "main.zil", source)],
            paths: Default::default(),
        },
        &CompileOptions {
            stop_after: stage,
            optimization,
            output: (stage == Stage::Build).then(|| directory.join("program")),
            emit_dir: Some(directory.join("phases")),
            ..CompileOptions::default()
        },
    );
    fs::write(directory.join("compile.json"), report.to_json()).unwrap();
    (directory, report)
}

fn native(label: &str, source: &str, stdout: &str, exit: i32) {
    for optimization in [Optimization::O0, Optimization::O2] {
        let (directory, report) = compile_optimized(label, source, Stage::Build, optimization);
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1);
        let output = Command::new(&report.artifacts["executable"])
            .output()
            .unwrap();
        fs::write(directory.join("stdout.log"), &output.stdout).unwrap();
        fs::write(directory.join("stderr.log"), &output.stderr).unwrap();
        fs::write(
            directory.join("native.txt"),
            format!(
                "optimization={optimization:?}\nexit={:?}\nexpected_exit={exit}\nexpected_stdout={stdout:?}\n",
                output.status.code()
            ),
        )
        .unwrap();
        assert_eq!(output.status.code(), Some(exit), "{label}: {output:?}");
        assert_eq!(output.stdout, stdout.as_bytes(), "{label}: {output:?}");
        assert!(output.stderr.is_empty(), "{label}: {output:?}");
    }
}

fn reject_types(label: &str, source: &str, message: &str) {
    let (_, report) = compile(label, source, Stage::Verify);
    assert_eq!(
        report.status,
        ResultCategory::Unsupported,
        "{}",
        report.to_json()
    );
    assert_eq!(report.phase, Phase::Types, "{}", report.to_json());
    assert_eq!(report.verification_runs, 0);
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.message.contains(message)),
        "{}",
        report.to_json()
    );
}

fn through_types(label: &str, source: &str) {
    let (_, report) = compile(label, source, Stage::Types);
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.phase, Phase::Types);
    assert_eq!(report.verification_runs, 0);
}

#[test]
fn original_complete_package_control_reaches_types() {
    through_types("original-control", ORIGINALS[0]);
}

#[test]
fn original_direct_store_capture_reaches_types() {
    through_types("original-direct", ORIGINALS[1]);
}

#[test]
fn original_package_store_capture_reaches_types() {
    through_types("original-package", ORIGINALS[2]);
}

#[test]
fn original_mutating_store_capture_reaches_types() {
    through_types("original-mutating", ORIGINALS[3]);
}

const LEAF: &str = r#"
extern "C" fn putchar(n: i32) -> i32;
resource Ticket { code: i32 }
impl Drop for Ticket { fn drop(self: Self) { unsafe { putchar(.code) } } }
package Item { manages ticket: Ticket; value: i32 }
package Bag { manages items: Store<Item>; manages side: Ticket }
"#;
const PREPARE: &str = r#"
    let items: Store<Item> = store_new()
    store_reserve(items, 1)
    store_push(items, Item { ticket: Ticket { code: 65 }, value: 42 })
"#;
const MAIN_ONCE: &str = r#"
fn main() -> i32 {
    let callback = make()
    let value = callback()
    unsafe { putchar(90) }
    return value - 1
}
"#;

fn direct_once(move_capture: bool) -> String {
    format!(
        "{LEAF}fn make() -> impl FnOnce() -> i32 {{ {PREPARE}
            return {} () => {{ let local = give items; return i32(store_len(local)) }}
        }}{MAIN_ONCE}",
        if move_capture { "move" } else { "" }
    )
}

fn package_once(move_capture: bool, whole: bool) -> String {
    let body = if whole {
        "let local = bag; return i32(store_len(local.items))"
    } else {
        "let local = give bag.items; return i32(store_len(local))"
    };
    format!(
        "{LEAF}fn make() -> impl FnOnce() -> i32 {{ {PREPARE}
            let bag = Bag {{ items: give items, side: Ticket {{ code: 66 }} }}
            return {} () => {{ {body} }}
        }}{MAIN_ONCE}",
        if move_capture { "move" } else { "" }
    )
}

#[test]
fn actual_direct_give_infers_creation_time_ownership() {
    through_types("direct-auto-give", &direct_once(false));
    through_types("direct-explicit-give", &direct_once(true));
}

#[test]
fn actual_package_field_give_and_whole_value_infer_ownership() {
    for whole in [false, true] {
        for moved in [false, true] {
            through_types(
                &format!("package-{whole}-{moved}"),
                &package_once(moved, whole),
            );
        }
    }
    let side = package_once(false, false).replace(
        "let local = give bag.items; return i32(store_len(local))",
        "let local = give bag.side; local.code; return i32(store_len(bag.items))",
    );
    through_types("package-give-other-managed-field", &side);
}

#[test]
fn ordinary_description_and_readonly_package_do_not_gain_ownership() {
    for index in [1, 2] {
        let source = ORIGINALS[index].replace("return move ()", "return ()");
        reject_types(&format!("nonowning-{index}"), &source, "Store");
    }
}

#[test]
fn direct_store_lending_result_does_not_escape_the_new_profile() {
    let source = format!(
        "{LEAF}fn make() -> impl Fn() -> &[u8] {{ {PREPARE}
            let bytes = bytes_new()
            return move () => {{ store_len(items); return bytes_view(bytes) }}
        }}fn main() {{ let callback = make(); }}"
    );
    reject_types("new-lending-result", &source, "Copy results");
    let old = source.replace("store_len(items); ", "");
    through_types("old-lending-control", &old);
}

#[test]
fn direct_child_environment_transmits_only_the_new_result_restriction() {
    let factory = &ORIGINALS[1][..ORIGINALS[1].find("fn main").unwrap()];
    let source = format!(
        "{factory}fn wrapped() -> impl Fn() -> &[u8] {{
            let child = make()
            let bytes = bytes_new()
            return move () => {{ child(); return bytes_view(bytes) }}
        }}fn main() {{ let callback = wrapped(); }}"
    );
    reject_types("child-new-lending-result", &source, "Copy results");
    let copy = source
        .replace("impl Fn() -> &[u8]", "impl Fn() -> i32")
        .replace("child(); return bytes_view(bytes)", "return child()");
    through_types("child-copy-result", &copy);
}

#[test]
fn nested_body_give_preserves_the_actual_package_transfer() {
    let source = package_once(false, false).replace(
        "return  () => { let local = give bag.items; return i32(store_len(local)) }",
        "return () => { let inner = () => { let local = give bag.items; return i32(store_len(local)) }; return inner() }",
    );
    assert!(source.contains("let inner"));
    through_types("nested-auto-package", &source);
}

#[test]
fn leaf_callback_exclusion_does_not_reclose_plain_stores_or_copy_companions() {
    for (label, definitions, value) in [
        (
            "direct-code",
            "package CodeItem { manages bytes: ByteBuffer; code: fn() -> i32 }",
            "CodeItem { bytes: bytes_new(), code: answer }",
        ),
        (
            "copy-record-code",
            "struct Code { value: fn() -> i32 } package CodeItem { manages bytes: ByteBuffer; code: Code }",
            "CodeItem { bytes: bytes_new(), code: Code { value: answer } }",
        ),
        (
            "managed-record-code",
            "resource Code { value: fn() -> i32 } package CodeItem { manages code: Code; value: i32 }",
            "CodeItem { code: Code { value: answer }, value: 7 }",
        ),
    ] {
        let prepare = format!(
            "let items: Store<CodeItem> = store_new(); store_reserve(items, 1); store_push(items, {value});"
        );
        let source = format!(
            "{definitions} fn answer() -> i32 {{ return 7 }}
            fn main() -> i32 {{ {prepare} return i32(store_len(items)) - 1 }}"
        );
        through_types(&format!("plain-{label}"), &source);
        let captured = format!(
            "{definitions} fn answer() -> i32 {{ return 7 }}
            fn make() -> impl Fn() -> i32 {{ {prepare} return move () => i32(store_len(items)) }}
            fn main() -> i32 {{ let callback = make(); return callback() - 1 }}"
        );
        reject_types(
            &format!("inner-leaf-{label}"),
            &captured,
            "leaf Package elements",
        );
    }
    let source = format!(
        "{LEAF} package Companion {{ manages items: Store<Item>; code: fn() -> i32 }}
        fn answer() -> i32 {{ return 7 }}
        fn make() -> impl Fn() -> i32 {{ {PREPARE}
            let bag = Companion {{ items: give items, code: answer }}
            return move () => i32(store_len(bag.items))
        }}{MAIN_ONCE}"
    );
    through_types("copy-companion-code", &source);
}

#[test]
fn original_sources_complete_verify_and_native() {
    for (index, source) in ORIGINALS.iter().enumerate() {
        native(&format!("original-{index}-native"), source, "", 0);
    }
}

#[test]
fn once_take_and_package_sibling_cleanup_execute() {
    native("direct-auto-native", &direct_once(false), "AZ", 0);
    native("direct-move-native", &direct_once(true), "AZ", 0);
    native(
        "package-auto-field-native",
        &package_once(false, false),
        "ABZ",
        0,
    );
    native(
        "package-auto-whole-native",
        &package_once(false, true),
        "ABZ",
        0,
    );
    let side = package_once(false, false).replace(
        "let local = give bag.items; return i32(store_len(local))",
        "let local = give bag.side; local.code; return i32(store_len(bag.items))",
    );
    native("package-give-side-native", &side, "BAZ", 0);
    let unused = package_once(false, false).replace("let value = callback()", "let value = 1");
    native("package-auto-unused-native", &unused, "ZAB", 0);
}
