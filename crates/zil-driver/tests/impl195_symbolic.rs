//! Unused templates must preserve every loop exit's ownership facts. A later
//! Copy instantiation cannot repair a bad definition or hide a continue edge.
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_base::{FileId, Limits, ModuleId, Phase, ResultCategory, SourceUnit};
use zil_driver::{CompileOptions, CompileReport, Stage, compile_source};

static NEXT: AtomicUsize = AtomicUsize::new(0);
const INVARIANT: &str = "generic loop must preserve outer carrier and non-Copy consumption state";
const MOVED: &str =
    "generic value used after move; Copy cannot be inferred from a concrete instance";

fn directory(label: &str) -> PathBuf {
    let root = std::env::var_os("ZIL_TEST_REPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let path = root.join(format!(
        "symbolic195-{label}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&path).unwrap();
    path
}
fn check(label: &str, source: &str) -> CompileReport {
    let dir = directory(label);
    fs::write(dir.join("main.zil"), source).unwrap();
    let report = compile_source(
        &SourceUnit::new(FileId(0), ModuleId(0), "symbolic195.zil", source),
        &CompileOptions {
            stop_after: Stage::Verify,
            emit_dir: Some(dir.join("phases")),
            ..CompileOptions::default()
        },
    );
    fs::write(dir.join("compile.json"), report.to_json()).unwrap();
    report
}
fn accepted(label: &str, source: &str) {
    let report = check(label, source);
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{label}: {}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1);
}
fn rejected(label: &str, source: &str, category: ResultCategory, message: &str) {
    let report = check(label, source);
    assert_eq!(report.status, category, "{label}: {}", report.to_json());
    assert_eq!(report.phase, Phase::Expand, "{label}: {}", report.to_json());
    assert_eq!(report.verification_runs, 0);
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.message.contains(message)),
        "{label}: {}",
        report.to_json()
    );
    assert!(!report.artifacts.contains_key("executable"));
}

#[test]
fn loop_control_outside_a_loop_is_rejected_even_in_unused_templates() {
    for control in ["break", "continue"] {
        let source = format!("fn bad<T>(value:T)->i32{{{control}}}fn main(){{}}");
        let report = check(control, &source);
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{}",
            report.to_json()
        );
        assert_eq!(report.phase, Phase::Expand);
        assert_eq!(report.verification_runs, 0);
        assert!(
            report
                .diagnostics
                .iter()
                .any(|d| d.code == "E-LOOP-CONTROL")
        );
    }
}

#[test]
fn every_continue_path_preserves_outer_noncopy_consumption() {
    for body in [
        "for index in 0..2{sink(value);continue}",
        "while flag{sink(value);continue}",
        "for index in 0..2{if flag{sink(value);continue}else{return}}",
        "for index in 0..2{match flag{true=>{sink(value);continue},false=>{break}}}",
        "for outer in 0..2{for inner in 0..2{sink(value);break};continue}",
        "for index in 0..2{let taken=value;let value=0;continue}",
    ] {
        rejected(
            "continue-move",
            &format!("fn sink<T>(value:T){{}}fn bad<T>(value:T,flag:bool){{{body}}}fn main(){{}}"),
            ResultCategory::Unsupported,
            INVARIANT,
        );
    }
}

#[test]
fn every_break_state_participates_in_post_loop_may_move_join() {
    for body in [
        "for index in 0..2{sink(value);break}",
        "while flag{sink(value);break}",
        "for index in 0..2{if flag{sink(value);break}else{continue}}",
        "for index in 0..2{match flag{true=>{sink(value);break},false=>{break}}}",
        "for index in 0..2{sink(value);let value=0;break}",
    ] {
        rejected(
            "break-move",
            &format!(
                "fn sink<T>(value:T){{}}fn bad<T>(value:T,flag:bool)->T{{{body};return value}}fn main(){{}}"
            ),
            ResultCategory::SafeReject,
            MOVED,
        );
    }
}

#[test]
fn returning_paths_do_not_pollute_surviving_break_and_continue_states() {
    for definition in [
        "fn good<T>(value:T,flag:bool)->T{for index in 0..2{if flag{return value}else{break}}return value}",
        "fn good<T>(value:T,flag:bool)->T{for index in 0..2{match flag{true=>{return value},false=>{continue}}}return value}",
        "fn good<T>(value:T)->T{var current=value;for index in 0..2{let temporary=current;current=temporary;continue}return current}",
        "fn good<T>(value:T)->T{for index in 0..2{let value=0;if index==0{continue};break}return value}",
        "fn good<T>(value:T){for index in 0..2{let consumed=value;break}}",
    ] {
        accepted("return-excluded", &format!("{definition}fn main(){{}}"));
    }
}

#[test]
fn store_carriers_and_managed_fields_cannot_be_consumed_on_continue_edges() {
    rejected(
        "store-continue",
        "fn bad<T:StoreElement>(give store:Store<T>){for index in 0..2{let taken=give store;continue}}fn main(){}",
        ResultCategory::Unsupported,
        INVARIANT,
    );
    rejected(
        "field-continue",
        "package Batch<T:StoreElement>{manages items:Store<T>}fn bad<T:StoreElement>(batch:Batch<T>){for index in 0..2{let taken=give batch.items;continue}}fn main(){}",
        ResultCategory::Unsupported,
        "generic loop cannot move or replace an outer managed field",
    );
}

#[test]
fn break_keeps_invalidated_store_observations_and_unproven_create_origins() {
    for body in [
        "let store:Store<T>=store_new();let old=store;for index in 0..2{let taken=give store;break};return store_len(old)",
        "let store:Store<T>=store_new();let old=store;for index in 0..2{if flag{let taken=give store;break}else{continue}}return store_len(old)",
    ] {
        rejected(
            "store-break-observation",
            &format!("fn bad<T:StoreElement>(flag:bool)->i64{{{body}}}fn main(){{}}"),
            ResultCategory::SafeReject,
            "Store descriptor depends on a carrier or field that was moved",
        );
    }
    rejected(
        "field-break-observation",
        "package Batch<T:StoreElement>{manages items:Store<T>}fn bad<T:StoreElement>(batch:Batch<T>)->i64{var local=batch;let old=local.items;for index in 0..2{local.items=store_new();break}return store_len(old)}fn main(){}",
        ResultCategory::SafeReject,
        "Store descriptor depends on a carrier or field that was moved",
    );
    rejected(
        "break-not-fresh",
        "fn bad<T:StoreElement>(give incoming:Store<T>,flag:bool)->Store<T>contract{result(create)}{var next:Store<T>=store_new();for index in 0..2{if flag{next=give incoming;break}else{continue}}return give next}fn main(){}",
        ResultCategory::SafeReject,
        "generic Store create result requires a fresh source on every returning path",
    );
}

#[test]
fn loop_exits_do_not_supply_function_return_or_allow_unreachable_uses() {
    for control in ["break", "continue"] {
        rejected(
            "no-function-return",
            &format!("fn bad<T>(value:T)->T{{for index in 0..2{{{control}}}}}fn main(){{}}"),
            ResultCategory::SafeReject,
            "generic body types must agree before concrete instantiation",
        );
        rejected(
            "unreachable-control",
            &format!(
                "fn bad<T>(value:T){{for index in 0..2{{{control};let lost=value}}}}fn main(){{}}"
            ),
            ResultCategory::Unsupported,
            "unreachable-statement type checking",
        );
    }
}

#[test]
fn continuation_does_not_make_a_once_callback_repeatable() {
    for control in ["continue", "if flag{continue}else{break}"] {
        rejected(
            "once-consumed",
            &format!(
                "fn bad<F:FnOnce()->i32>(f:F,flag:bool)->i32{{var total=0;for index in 0..2{{total+=f();{control}}}return total}}fn main(){{}}"
            ),
            ResultCategory::Unsupported,
            INVARIANT,
        );
    }
    let source = "fn run<F:FnOnce()->i32>(f:F,n:i32)->i32{for index in 0..n{continue}return 0}fn main()->i32{let f=move()=>1;return run(f,2)}";
    let report = check("once-zero-event-cycle", source);
    assert_eq!(
        report.status,
        ResultCategory::Unsupported,
        "{}",
        report.to_json()
    );
    assert_eq!(report.phase, Phase::Types);
    assert_eq!(report.verification_runs, 0);
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.message.contains("FnOnce"))
    );
}

#[test]
fn loop_control_keeps_index_mutability_and_scope_checks() {
    rejected(
        "index-write",
        "fn bad<T>(value:T){for index in 0..2{index+=1;continue}}fn main(){}",
        ResultCategory::SafeReject,
        "assignment requires a writable place",
    );
    rejected(
        "index-escape",
        "fn bad<T>(value:T)->i32{for index in 0..2{break}return index}fn main(){}",
        ResultCategory::SafeReject,
        "unknown value index in generic body",
    );
}

#[test]
fn loop_exit_state_collections_have_exact_existing_budget_frontiers() {
    let mut body = String::new();
    for n in 0..24 {
        body.push_str(&format!(
            "if mode=={n}{{{}}}",
            if n % 2 == 0 { "continue" } else { "break" }
        ));
    }
    let source = format!(
        "fn keep<T>(value:T,mode:i32)->T{{for index in 0..4{{{body}}}return value}}fn main(){{}}"
    );
    let unit = SourceUnit::new(FileId(0), ModuleId(0), "budget195.zil", &source);
    let ast = zil_syntax::parse(&unit).into_result().unwrap();
    for metric in ["nodes", "work"] {
        let dir = directory(metric);
        fs::write(dir.join("main.zil"), &source).unwrap();
        let mut observations = Vec::new();
        let mut attempt =
            |limit| {
                let mut limits = Limits::default();
                if metric == "nodes" {
                    limits.max_nodes = limit;
                } else {
                    limits.max_analysis_steps = limit;
                }
                let result = zil_expand::expand_with_map_limits(&unit, &ast, &limits);
                let diagnostics = match &result {
                    Ok(_) => String::from("[]"),
                    Err(d) => format!(
                        "[{}]",
                        d.iter().map(|d| d.to_json()).collect::<Vec<_>>().join(",")
                    ),
                };
                observations.push(format!(
                    "{{\"limit\":{limit},\"success\":{},\"diagnostics\":{diagnostics}}}",
                    result.is_ok()
                ));
                if let Err(diagnostics) = &result {
                    assert!(
                        diagnostics.iter().any(|d| d.phase == Phase::Expand
                            && d.category == ResultCategory::ResourceLimit),
                        "{diagnostics:?}"
                    );
                }
                result.is_ok()
            };
        let full = if metric == "nodes" {
            Limits::default().max_nodes
        } else {
            Limits::default().max_analysis_steps
        };
        assert!(attempt(full));
        assert!(!attempt(0));
        let (mut low, mut high) = (0usize, full);
        while low + 1 < high {
            let mid = low + (high - low) / 2;
            if attempt(mid) {
                high = mid;
            } else {
                low = mid;
            }
        }
        assert!(!attempt(high - 1));
        assert!(attempt(high));
        assert!(attempt(full));
        fs::write(dir.join("frontier.json"), format!("{{\"metric\":\"{metric}\",\"minimum\":{high},\"n_minus_one_rejects\":true,\"fresh_retry\":true,\"observations\":[{}]}}\n", observations.join(","))).unwrap();
    }
}
