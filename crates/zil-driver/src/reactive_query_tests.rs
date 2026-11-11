//! Current-source differential controls for the opt-in semantic query graph.
//! The oracle has a fresh session and the original non-reactive Types path.
use super::*;
use crate::ProjectSources;
use std::sync::atomic::{AtomicUsize, Ordering};
use zil_base::{FileId, ModuleId};

static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "zil-reactive-source-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn project(&self, main: &str, left: &str, right: &str) -> ProjectSources {
        let modules = [("main.zil", main), ("left.zil", left), ("right.zil", right)]
            .into_iter()
            .enumerate()
            .map(|(i, (name, text))| {
                let path = self.0.join(name);
                fs::write(&path, text).unwrap();
                SourceUnit::new(
                    FileId(i as u32),
                    ModuleId(i as u32),
                    path.to_string_lossy(),
                    text,
                )
            })
            .collect();
        ProjectSources {
            root: ModuleId(0),
            modules,
            paths: BTreeMap::from([
                (vec!["left".into()], ModuleId(1)),
                (vec!["right".into()], ModuleId(2)),
            ]),
        }
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
const MAIN: &str = "import left\nimport right\nfn value(x:i32)->i32{if x<5{return left.value(x)+right.value(x);}else{return right.value(x);}}";
const LEFT: &str = "fn leaf(x:i32)->i32{if x<7{return x+1;}else{return x+2;}} pub fn value(x:i32)->i32{if x<3{return leaf(x);}else{return leaf(x+1);}}";
const RIGHT: &str = "fn leaf(x:i32)->i32{if x<7{return x+3;}else{return x+4;}} pub fn value(x:i32)->i32{if x<3{return leaf(x);}else{return leaf(x+1);}}";
fn options() -> CompileOptions {
    CompileOptions {
        reactive_queries: true,
        ..Default::default()
    }
}
fn typed<'a>(session: &'a CompilationSession, report: &CompileReport) -> &'a TypedProgram {
    session
        .raw
        .values()
        .find_map(|raw| match raw {
            RawSnapshot::ProjectTyped(program)
                if program.module.source_fingerprint == report.input_fingerprint =>
            {
                Some(program.as_ref())
            }
            _ => None,
        })
        .expect("current successful Types snapshot")
}
fn compare(
    session: &CompilationSession,
    report: &CompileReport,
    project: &ProjectSources,
    options: &CompileOptions,
) -> CompileReport {
    let mut fresh_options = options.clone();
    fresh_options.reactive_queries = false;
    fresh_options.semantic_cache = None;
    fresh_options.cancellation = Default::default();
    let mut fresh = CompilationSession::default();
    let expected = fresh.compile_project(project, &fresh_options);
    assert_eq!(
        report.status,
        expected.status,
        "{}\n{}",
        report.to_json(),
        expected.to_json()
    );
    assert_eq!(report.phase, expected.phase);
    assert_eq!(report.diagnostics, expected.diagnostics);
    assert_eq!(report.source_files, expected.source_files);
    assert_eq!(report.scalar_interfaces, expected.scalar_interfaces);
    assert_eq!(session.active_query_count(), 0);
    if report.status == ResultCategory::Success {
        assert_eq!(report.verification_runs, 1);
        assert_eq!(expected.verification_runs, 1);
        assert_eq!(typed(session, report), typed(&fresh, &expected));
    }
    expected
}
fn successful(report: &CompileReport) {
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1);
}

#[test]
fn reactive_private_and_atomic_multi_file_edits_reduce_actual_body_encoding() {
    let dir = Directory::new();
    let options = options();
    let mut session = CompilationSession::default();
    let initial = dir.project(MAIN, LEFT, RIGHT);
    let cold = session.compile_project(&initial, &options);
    successful(&cold);
    assert!(cold.function_type_work.unwrap().body_key_executions > 0);
    for (left, right) in [
        (LEFT.replace("x+1", "x+5"), RIGHT.to_owned()),
        (LEFT.replace("x+1", "x+6"), RIGHT.replace("x+3", "x+7")),
    ] {
        // The driver receives one immutable snapshot containing BOTH changes.
        let project = dir.project(MAIN, &left, &right);
        let current = session.compile_project(&project, &options);
        successful(&current);
        let full = compare(&session, &current, &project, &options);
        let (work, full_work) = (
            current.function_type_work.unwrap(),
            full.function_type_work.unwrap(),
        );
        assert!(work.body_key_reuses > 0);
        assert!(work.body_key_executions < full_work.body_key_executions);
        assert!(work.body_key_visited_nodes < full_work.body_key_visited_nodes);
        assert!(work.reused_functions > 0);
        assert!(work.executed_functions > 0);
    }
    let no_change = session.compile_project(
        &dir.project(
            MAIN,
            &LEFT.replace("x+1", "x+6"),
            &RIGHT.replace("x+3", "x+7"),
        ),
        &options,
    );
    successful(&no_change);
    assert!(
        no_change.function_type_work.is_none(),
        "whole-project hit must not report old work"
    );
}

#[test]
fn reactive_current_signatures_absence_visibility_and_source_id_rebinding() {
    let dir = Directory::new();
    let options = options();
    let mut session = CompilationSession::default();
    successful(&session.compile_project(&dir.project(MAIN, LEFT, RIGHT), &options));
    let cases = [
        (
            MAIN.to_owned(),
            LEFT.replace("leaf(x:i32)", "leaf(x:bool)"),
            RIGHT.to_owned(),
        ),
        (
            MAIN.to_owned(),
            LEFT.replace("pub fn value", "fn value"),
            RIGHT.to_owned(),
        ),
        (
            MAIN.replace("left.value", "left.missing"),
            LEFT.to_owned(),
            RIGHT.to_owned(),
        ),
        (
            MAIN.replace("left.value", "left.missing"),
            LEFT.replace("pub fn value", "pub fn missing"),
            RIGHT.to_owned(),
        ),
        (
            MAIN.to_owned(),
            format!("fn inserted()->i32=>8\n{LEFT}"),
            RIGHT.to_owned(),
        ),
        (
            format!("// shifted root\n\n{MAIN}"),
            format!("// shifted left\n{LEFT}"),
            format!("// shifted right\n{RIGHT}"),
        ),
        (
            MAIN.replace("import left\nimport right", "import right\nimport left"),
            LEFT.to_owned(),
            RIGHT.to_owned(),
        ),
        (MAIN.to_owned(), LEFT.to_owned(), RIGHT.to_owned()),
    ];
    for (main, left, right) in cases {
        let project = dir.project(&main, &left, &right);
        let current = session.compile_project(&project, &options);
        compare(&session, &current, &project, &options);
    }
    // Preserve logical module paths while assigning entirely different compact
    // ModuleId/FileId values. Old typed references and locations cannot survive.
    let mut remapped = dir.project(MAIN, LEFT, RIGHT);
    remapped.root = ModuleId(30);
    for (i, source) in remapped.modules.iter_mut().enumerate() {
        source.module_id = ModuleId(30 + i as u32);
        source.file_id = FileId(90 + i as u32);
    }
    remapped.paths = BTreeMap::from([
        (vec!["left".into()], ModuleId(31)),
        (vec!["right".into()], ModuleId(32)),
    ]);
    let current = session.compile_project(&remapped, &options);
    successful(&current);
    compare(&session, &current, &remapped, &options);
}

#[test]
fn reactive_dynamic_call_bonds_and_legal_mutual_recursion_match_full_types() {
    let dir = Directory::new();
    let options = options();
    let left = "pub fn one(x:i32)->i32{if x<1{return 1;}else{return two(x-1);}} pub fn two(x:i32)->i32{if x<1{return 2;}else{return one(x-1);}} pub fn value(x:i32)->i32{if x<3{return one(x);}else{return two(x);}}";
    let mut session = CompilationSession::default();
    successful(&session.compile_project(&dir.project(MAIN, left, RIGHT), &options));
    for i in 0..12 {
        let main = if i % 2 == 0 {
            MAIN.replace("left.value(x)", "left.one(x)")
        } else {
            MAIN.replace("left.value(x)", "right.value(x)")
        };
        let left = left.replace("return 1;", &format!("return {};", 3 + i));
        let project = dir.project(&main, &left, RIGHT);
        let current = session.compile_project(&project, &options);
        successful(&current);
        compare(&session, &current, &project, &options);
    }
}

#[test]
fn reactive_budget_boundary_cancel_retry_and_eviction_preserve_current_checks() {
    let dir = Directory::new();
    let initial = dir.project(MAIN, LEFT, RIGHT);
    let options = options();
    let mut session = CompilationSession::with_capacity(8);
    let initial_report = session.compile_project(&initial, &options);
    successful(&initial_report);
    let instructions = typed(&session, &initial_report)
        .module
        .functions
        .iter()
        .flat_map(|f| &f.blocks)
        .map(|b| b.instructions.len())
        .sum::<usize>();
    assert!(instructions > 0);
    for n in [instructions, instructions - 1, instructions] {
        let mut limited = options.clone();
        limited.limits.max_instructions = n;
        let current = session.compile_project(&initial, &limited);
        assert_eq!(current.status == ResultCategory::Success, n == instructions);
        compare(&session, &current, &initial, &limited);
    }
    for phase in [Phase::Types, Phase::Analyze, Phase::Verify] {
        let current_options = CompileOptions {
            reactive_queries: true,
            ..Default::default()
        };
        let project = dir.project(MAIN, &LEFT.replace("x+1", "x+9"), RIGHT);
        let cancelled =
            session.compile_project_with_observer(&project, &current_options, &mut |event| {
                if event.phase == phase {
                    current_options.cancellation.cancel();
                }
            });
        assert_eq!(cancelled.status, ResultCategory::Cancelled);
        assert_eq!(session.active_query_count(), 0);
        let retry = session.compile_project(&project, &options);
        successful(&retry);
        compare(&session, &retry, &project, &options);
    }
    for i in 0..80 {
        let project = dir.project(MAIN, &LEFT.replace("x+1", &format!("x+{}", i + 10)), RIGHT);
        let current = session.compile_project(&project, &options);
        successful(&current);
        if i % 11 == 0 {
            compare(&session, &current, &project, &options);
        }
        assert!(session.cached_query_count() <= 8);
    }
    let restored = session.compile_project(&initial, &options);
    successful(&restored);
    compare(&session, &restored, &initial, &options);
}

#[test]
fn reactive_persistent_content_cache_and_corruption_remain_raw_candidates() {
    let dir = Directory::new();
    let options = CompileOptions {
        semantic_cache: Some(dir.0.join("cache")),
        ..options()
    };
    let initial = dir.project(MAIN, LEFT, RIGHT);
    let mut session = CompilationSession::default();
    successful(&session.compile_project(&initial, &options));
    let changed = dir.project(MAIN, &LEFT.replace("x+1", "x+8"), RIGHT);
    let mut restarted = CompilationSession::default();
    let current = restarted.compile_project(&changed, &options);
    successful(&current);
    compare(&restarted, &current, &changed, &options);
    let directory = dir.0.join("cache/function-types-v1");
    let files = fs::read_dir(directory)
        .unwrap()
        .map(|file| file.unwrap().path())
        .collect::<Vec<_>>();
    assert!(!files.is_empty());
    for file in files {
        fs::write(file, b"invalid untrusted transport").unwrap();
    }
    let changed = dir.project(MAIN, &LEFT.replace("x+1", "x+9"), RIGHT);
    let mut restarted = CompilationSession::default();
    let current = restarted.compile_project(&changed, &options);
    successful(&current);
    compare(&restarted, &current, &changed, &options);
}

// These mixed programs deliberately leave the scalar graph's eligibility
// envelope. They control whole-program fallback; passing them does not claim
// that trait solving, specialization, CT selection or owned closures are queries.
#[test]
fn fallback_trait_candidate_add_remove_and_generic_instance_sets_match_fresh() {
    let dir = Directory::new();
    let options = options();
    let mut session = CompilationSession::default();
    const DECLARATIONS: &str = "trait Read{fn read(self:&Self)->i32;}struct Point{value:i32}struct Other{value:i32}fn read<T:Read>(value:&T)->i32{return value.read()}fn identity<T:Copy>(value:T)->T{return value}";
    const POINT_IMPL: &str = "impl Read for Point{fn read()->i32=>.value}";
    const OTHER_IMPL: &str = "impl Read for Other{fn read()->i32=>.value}";
    const POINT: &str = "fn main()->i32{let p=Point{value:7};return identity::<i32>(read(&p))}";
    const BOTH: &str = "fn main()->i32{let p=Point{value:7};let q=Other{value:9};let flag=identity::<bool>(true);if flag{return read(&p)+read(&q)}return identity::<i32>(0)}";
    const WIDE: &str =
        "fn main()->i32{let p=Point{value:7};return read(&p)+i32(identity::<i64>(7))}";
    for (point_impl, other_impl, entry, rejected) in [
        (POINT_IMPL, "", POINT, false),
        // Add an actual impl candidate and both read<Other>/identity<bool>.
        (POINT_IMPL, OTHER_IMPL, BOTH, false),
        // Withdraw an impl still required by this exact current call graph.
        ("", OTHER_IMPL, BOTH, true),
        (POINT_IMPL, OTHER_IMPL, BOTH, false),
        // Remove the other candidate and its instances, then change i32->i64.
        (POINT_IMPL, "", POINT, false),
        (POINT_IMPL, "", WIDE, false),
        (POINT_IMPL, "", POINT, false),
    ] {
        let main = format!("{DECLARATIONS}\n{point_impl}\n{other_impl}\n{entry}");
        let project = dir.project(&main, LEFT, RIGHT);
        let current = session.compile_project(&project, &options);
        let full = compare(&session, &current, &project, &options);
        assert_eq!(current.verification_runs, full.verification_runs);
        assert!(
            current
                .diagnostics
                .iter()
                .all(|diagnostic| diagnostic.category != ResultCategory::Unsupported)
        );
        if rejected {
            assert_eq!(
                current.status,
                ResultCategory::SafeReject,
                "{}",
                current.to_json()
            );
            let diagnostic = current
                .diagnostics
                .iter()
                .find(|diagnostic| diagnostic.code == "E-GENERIC-ARGUMENT")
                .expect("missing current trait obligation must be a named rejection");
            assert_eq!(diagnostic.phase, Phase::Expand);
            assert!(
                diagnostic
                    .message
                    .contains("does not satisfy nominal trait bound")
            );
            let span = diagnostic
                .span
                .expect("trait obligation retains the actual source location");
            assert_eq!(span.file, project.modules[0].file_id);
            assert_eq!(current.source_files[&span.file], project.modules[0].path);
            assert_eq!(current.verification_runs, 0);
        } else {
            successful(&current);
            if let Some(work) = current.function_type_work {
                assert_eq!(
                    work.graph.executed, 0,
                    "mixed nominal program must not claim scalar graph work"
                );
                assert_eq!(work.body_key_reuses, 0);
            } else {
                // Restoring exact prior source may reuse the whole Types query;
                // the current Verify and the full TypedProgram oracle still run.
                assert!(
                    current
                        .query_events
                        .iter()
                        .any(|event| event.phase == Phase::Types
                            && event.disposition == QueryDisposition::Reused)
                );
                assert_eq!(current.verification_runs, 1);
            }
        }
    }
}

#[test]
fn fallback_ct_branch_redirects_actual_calls_with_owned_closure_state() {
    let dir = Directory::new();
    let options = options();
    let mut session = CompilationSession::default();
    // The resource factory/once consumer follows the existing retained-factory
    // source contract; CT chooses a different imported runtime callee.
    const MAIN: &str = "import left\nimport right\nresource Token{n:i32}fn consume(give token:Token)->i32{return token.n}fn make(n:i32)->impl FnOnce()->i32{let token=Token{n};return move ()=>consume(give token)}#if(left.enabled()){fn selected()->i32{return left.value(1)}}else{fn selected()->i32{return right.value(2)}}fn main()->i32{let f=make(5);return f()+selected()}";
    for (enabled, repeat_once, right) in [
        (true, false, RIGHT.to_owned()),
        (false, false, RIGHT.to_owned()),
        (false, false, RIGHT.replace("x+3", "x+8")),
        (true, true, RIGHT.to_owned()),
        (false, false, RIGHT.to_owned()),
        (true, false, RIGHT.to_owned()),
    ] {
        let main = if repeat_once {
            MAIN.replace("return f()+selected()", "return f()+f()+selected()")
        } else {
            MAIN.to_owned()
        };
        let left = format!("{LEFT}\npub fn enabled()->bool{{return {enabled};}}");
        let project = dir.project(&main, &left, &right);
        let current = session.compile_project(&project, &options);
        let full = compare(&session, &current, &project, &options);
        assert_eq!(current.verification_runs, full.verification_runs);
        assert!(
            current
                .diagnostics
                .iter()
                .all(|diagnostic| diagnostic.category != ResultCategory::Unsupported)
        );
        if repeat_once {
            assert_eq!(
                current.status,
                ResultCategory::SafeReject,
                "{}",
                current.to_json()
            );
            assert!(
                current
                    .diagnostics
                    .iter()
                    .any(|diagnostic| diagnostic.phase == Phase::Analyze
                        && diagnostic.code == "E-ANALYZE-IR")
            );
            assert_eq!(current.verification_runs, 0);
        } else {
            successful(&current);
            if let Some(work) = current.function_type_work {
                assert_eq!(work.graph.executed, 0);
                assert_eq!(work.body_key_reuses, 0);
            } else {
                assert!(
                    current
                        .query_events
                        .iter()
                        .any(|event| event.phase == Phase::Types
                            && event.disposition == QueryDisposition::Reused)
                );
                assert_eq!(current.verification_runs, 1);
            }
            let program = typed(&session, &current);
            assert!(
                !program.module.closures.is_empty(),
                "the real resource closure must survive Types"
            );
            let selected = program
                .module
                .functions
                .iter()
                .find(|function| function.name == "selected")
                .expect("CT emitted the selected ordinary function");
            let chosen_module = if enabled { ModuleId(1) } else { ModuleId(2) };
            let target = current
                .scalar_interfaces
                .iter()
                .find(|interface| interface.module == chosen_module)
                .and_then(|interface| {
                    interface
                        .exports
                        .iter()
                        .find(|export| export.name == "value")
                })
                .expect("current selected import has an exported value function")
                .function;
            assert!(selected.blocks.iter().flat_map(|block| &block.instructions).any(|instruction| matches!(instruction.kind, zil_ir::InstructionKind::Call { function, .. } if function == target)), "CT flip must redirect the actual typed call, not just change scheduling events");
        }
    }
}
