//! 010: real source through scalar CFG verification and the native backend.
//! These checks do not claim support for general iterator values or patterns.
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use zil_base::{FileId, Limits, ModuleId, Phase, ResultCategory, SourceUnit, TargetProfile};
use zil_driver::{CompileOptions, Stage, compile_source};
use zil_ir::{CoreModule, InstructionKind, TypedModule};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "zil-range-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn source(text: &str) -> SourceUnit {
    SourceUnit::new(FileId(0), ModuleId(0), "ranges.zil", text)
}
fn core(text: &str) -> CoreModule {
    let source = source(text);
    let parsed = zil_syntax::parse(&source);
    assert!(parsed.is_ok(), "{:?}", parsed.diagnostics);
    zil_expand::expand(&source, &parsed.module).unwrap()
}
fn checked_cfg(text: &str) -> TypedModule {
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    let core = zil_ct::evaluate_source_constants(&core(text), &target, &limits).unwrap();
    let typed = zil_types::check(&core, &target, &limits).unwrap();
    let plan = zil_analyze::analyze(&typed, &limits).unwrap();
    zil_verify::verify_module(typed, plan, &target, &limits)
        .unwrap()
        .module()
        .clone()
}
fn native(text: &str, expected: i64) {
    let directory = Directory::new();
    let options = CompileOptions {
        stop_after: Stage::Build,
        entry: Some("result".into()),
        output: Some(directory.0.join("program")),
        ..CompileOptions::default()
    };
    let report = compile_source(&source(text), &options);
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    let mut child = Command::new(&report.artifacts["executable"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if child.try_wait().unwrap().is_some() {
            break;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("range executable exceeded five seconds: {text}");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success(), "{:?}", output);
    assert_eq!(
        String::from_utf8(output.stdout).unwrap().trim(),
        expected.to_string()
    );
}
fn rejected(text: &str, category: ResultCategory, code: &str, phase: Phase) {
    let report = compile_source(&source(text), &CompileOptions::default());
    assert_eq!(report.status, category, "{}", report.to_json());
    assert_eq!(report.diagnostics[0].code, code, "{}", report.to_json());
    assert_eq!(report.diagnostics[0].phase, phase, "{}", report.to_json());
}

#[test]
fn rng01_i32_half_open_positive_negative_empty_and_reversed() {
    for (bounds, expected) in [("1..5", 10), ("-4..0", -10), ("2..2", 0), ("5..2", 0)] {
        native(
            &format!(
                "fn result() -> i32 {{\nvar sum = 0\nfor n in {bounds} {{ sum = sum + n }}\nreturn sum\n}}"
            ),
            expected,
        );
    }
}

#[test]
fn rng02_i64_bounds_and_contextual_literals() {
    native(
        "fn result() -> i64 {
            let low: i64 = 2147483648
            var sum: i64 = 0
            for n in low..(low + 3) { sum = sum + n }
            return sum
        }",
        6_442_450_947,
    );
    native(
        "fn result() -> i64 {
            let high: i64 = 3
            var sum: i64 = 0
            for n in 0..(high + 1) { sum = sum + n }
            for n in -2..(-high + 2) { sum = sum + n }
            return sum
        }",
        4,
    );
    native(
        "fn upper(value: i64) -> i64 => value
        fn result() -> i64 {
            let high: i64 = 3
            var total: i64 = 0
            for n in 0..(high |> upper) { total = total + n }
            return total
        }",
        3,
    );
}

#[test]
fn rng03_integer_extremes_do_not_step_past_maximum() {
    for (ty, maximum, minimum) in [
        ("i32", i32::MAX as i64, i32::MIN as i64),
        ("i64", i64::MAX, i64::MIN),
    ] {
        native(
            &format!(
                "fn result() -> {ty} {{
            let high: {ty} = {maximum}
            var count: {ty} = 0
            for n in (high - 2)..high {{ count = count + 1 }}
            for n in high..high {{ count = count + 100 }}
            return count
        }}"
            ),
            2,
        );
        native(
            &format!(
                "fn result() -> {ty} {{
            let low: {ty} = {minimum}
            var last: {ty} = 0
            for n in low..(low + 2) {{ last = n }}
            return last
        }}"
            ),
            minimum + 1,
        );
    }
}

#[test]
fn rng04_continue_uses_step_and_break_skips_it() {
    native(
        "fn result() -> i32 {
            var sum = 0
            for n in 1..8 {
                if n == 2 { continue }
                if n == 6 { break }
                sum = sum + n
            }
            return sum
        }",
        13,
    );
}

#[test]
fn rng05_nested_for_and_while_target_the_nearest_loop() {
    native(
        "fn result() -> i32 {
            var sum = 0
            for i in 0..4 {
                for j in 0..4 {
                    if j == 1 { continue }
                    if j == 3 { break }
                    sum = sum + i * 10 + j
                }
                if i == 2 { continue }
                sum = sum + 100
            }
            return sum
        }",
        428,
    );
    native(
        "fn result() -> i32 {
            var sum = 0
            for i in 0..3 {
                var j = 0
                while j < 4 {
                    j = j + 1
                    if j == 2 { continue }
                    if j == 4 { break }
                    sum = sum + i * 10 + j
                }
                if i == 1 { continue }
                sum = sum + 100
            }
            return sum
        }",
        272,
    );
    native(
        "fn result() -> i32 {
            var sum = 0
            var i = 0
            while i < 3 {
                i = i + 1
                for j in 0..4 {
                    if j == 1 { continue }
                    if j == 3 { break }
                    sum = sum + i * 10 + j
                }
            }
            return sum
        }",
        126,
    );
}

#[test]
fn rng06_bounds_use_outer_scope_and_are_captured_before_iteration() {
    native(
        "fn result() -> i32 {
            let n = 2
            var high = 5
            var sum = 0
            for n in (n..high) {
                sum = sum + n
                high = 2
            }
            return sum * 10 + n
        }",
        92,
    );
    native(
        "fn result() -> i32 {
            var sum = 0
            for n in 0..3 {
                let n = 100
                sum = sum + n
            }
            return sum
        }",
        300,
    );
}

#[test]
fn rng07_iteration_binding_is_read_only_scoped_and_not_self() {
    for (text, code) in [
        (
            "fn result() -> i32 { for n in 0..3 { n = n + 1 }\nreturn 0 }",
            "E-IMMUTABLE-ASSIGNMENT",
        ),
        (
            "fn result() -> i32 { for n in 0..3 { }\nreturn n }",
            "E-UNKNOWN-NAME",
        ),
        (
            "fn result() -> i32 { for self in 0..3 { }\nreturn 0 }",
            "E-RECEIVER-RESERVED-NAME",
        ),
    ] {
        rejected(text, ResultCategory::SafeReject, code, Phase::Types);
    }
}

#[test]
fn rng08_typed_mixed_widths_and_noninteger_bounds_are_rejected() {
    for (left, right) in [("i32", "i64"), ("i64", "i32")] {
        rejected(
            &format!(
                "fn result() -> i32 {{
            let low: {left} = 0
            let high: {right} = 3
            for n in low..high {{ }}
            return 0
        }}"
            ),
            ResultCategory::SafeReject,
            "E-TYPE-MISMATCH",
            Phase::Types,
        );
    }
    rejected(
        "fn result() -> i32 { for n in false..true { }\nreturn 0 }",
        ResultCategory::SafeReject,
        "E-OPERATOR-TYPE",
        Phase::Types,
    );
}

#[test]
fn rng09_other_iterables_and_patterns_remain_unsupported() {
    for text in [
        "fn result() -> i32 { for _ in 0..3 { }\nreturn 0 }",
        "fn result() -> i32 { for (a, b) in 0..3 { }\nreturn 0 }",
        "fn result() -> i32 { for n in 3 { }\nreturn 0 }",
    ] {
        rejected(
            text,
            ResultCategory::Unsupported,
            "E-UNSUPPORTED",
            Phase::Types,
        );
    }
}

#[test]
fn rng10_unconditional_exits_leave_no_invalid_step_uses() {
    for (body, expected) in [
        ("for n in 0..3 { break }", 8),
        ("for n in 0..3 { continue }", 8),
        ("for n in 0..3 { return n }", 0),
        ("for n in 3..0 { return n }", 8),
        ("for n in 0..3 { if n == 0 { break } else { return n } }", 8),
    ] {
        native(
            &format!("fn result() -> i32 {{\n{body}\nreturn 8\n}}"),
            expected,
        );
    }
}

#[test]
fn rng11_closed_ct_uses_the_iteration_type_without_running_the_loop() {
    native(
        "fn result() -> i64 {
            let upper: i64 = 3
            var total: i64 = 0
            for n in #(0)..upper {
                let term = n + #(2147483648)
                total = total + term
            }
            return total
        }",
        6_442_450_947,
    );
    let mut usages = Vec::new();
    for high in [3, i32::MAX] {
        let core = core(&format!(
            "fn result() -> i32 {{
            var total = 0
            for n in 0..{high} {{ total = total + #(2 + 3) }}
            return total
        }}"
        ));
        let (_, usage) = zil_ct::evaluate_source_constants_with_usage(
            &core,
            &TargetProfile::host_llvm(),
            &Limits::default(),
        )
        .unwrap();
        usages.push((usage.nodes, usage.steps));
    }
    assert_eq!(usages[0], usages[1]);
    assert_eq!(usages[0].1, 3);
    eprintln!("RNG11 CT node/step usage for upper 3 and i32::MAX: {usages:?}");
}

#[test]
fn rng12_runtime_iteration_variable_cannot_enter_closed_ct_even_when_empty() {
    for high in [0, 3] {
        rejected(
            &format!(
                "fn result() -> i32 {{ for n in 0..{high} {{ let value = #(n + 1) }}\nreturn 0 }}"
            ),
            ResultCategory::SafeReject,
            "E-CT-RUNTIME",
            Phase::Ct,
        );
    }
}

#[test]
fn rng13_endpoint_calls_occur_once_in_source_order_before_the_loop() {
    for (low, high, expected) in [(2, 5, 9), (5, 2, 0), (2, 2, 0)] {
        let text = format!(
            "fn lower() -> i64 => {low}
            fn upper() -> i64 => {high}
            fn result() -> i64 {{
                var total: i64 = 0
                for n in lower()..upper() {{ total = total + n }}
                return total
            }}"
        );
        let typed = checked_cfg(&text);
        let lower = typed
            .functions
            .iter()
            .find(|f| f.name == "lower")
            .unwrap()
            .id;
        let upper = typed
            .functions
            .iter()
            .find(|f| f.name == "upper")
            .unwrap()
            .id;
        let result = typed.functions.iter().find(|f| f.name == "result").unwrap();
        let calls: Vec<_> = result
            .blocks
            .iter()
            .flat_map(|block| {
                block.instructions.iter().filter_map(move |instruction| {
                    if let InstructionKind::Call { function, .. } = instruction.kind {
                        Some((block.id, function))
                    } else {
                        None
                    }
                })
            })
            .collect();
        assert_eq!(calls, [(result.entry, lower), (result.entry, upper)]);
        eprintln!(
            "RNG13 lower={low}, upper={high}; verified CFG calls={calls:?}; fingerprint={}",
            typed.fingerprint()
        );
        native(&text, expected);
    }
}

#[test]
fn rng14_range_cfg_obeys_the_existing_block_budget() {
    let text = "fn result() -> i32 { for n in 0..3 { }\nreturn 7 }";
    let mut options = CompileOptions::default();
    options.limits.max_blocks = 4;
    let rejected = compile_source(&source(text), &options);
    assert_eq!(
        rejected.status,
        ResultCategory::ResourceLimit,
        "{}",
        rejected.to_json()
    );
    assert_eq!(rejected.diagnostics[0].phase, Phase::Types);
    options.limits.max_blocks = 5;
    let accepted = compile_source(&source(text), &options);
    assert_eq!(
        accepted.status,
        ResultCategory::Success,
        "{}",
        accepted.to_json()
    );
}

#[test]
fn rng15_ct_hint_shadowing_matches_runtime_iteration_scope() {
    native(
        "fn result() -> i32 {
            let n: i64 = 100
            var total = 0
            for n in -2..-1 {
                let term = n + #(1)
                total = total + term
            }
            return total
        }",
        -1,
    );
}
