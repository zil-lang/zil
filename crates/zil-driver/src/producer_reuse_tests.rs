//! Producer/version reuse is compared against the current source in a fresh
//! legacy session. Public interfaces alone are not the Types oracle.
use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};
use zil_base::{FileId, ModuleId};

static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "zil-producer-differential-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn project(&self, texts: &[(&str, &str)]) -> ProjectSources {
        let mut paths = BTreeMap::new();
        let modules = texts
            .iter()
            .enumerate()
            .map(|(i, (name, text))| {
                let path = self.0.join(format!("{name}.zil"));
                fs::write(&path, text).unwrap();
                if i != 0 {
                    paths.insert(vec![(*name).to_owned()], ModuleId(i as u32));
                }
                SourceUnit::new(
                    FileId(i as u32),
                    ModuleId(i as u32),
                    path.to_string_lossy(),
                    *text,
                )
            })
            .collect();
        ProjectSources {
            root: ModuleId(0),
            modules,
            paths,
        }
    }
    fn scalar(&self, main: &str, left: &str, right: &str) -> ProjectSources {
        self.project(&[("main", main), ("left", left), ("right", right)])
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

fn options(graph: bool) -> CompileOptions {
    CompileOptions {
        producer_reuse: true,
        reactive_queries: graph,
        ..Default::default()
    }
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
fn typed<'a>(session: &'a CompilationSession, report: &CompileReport) -> &'a TypedProgram {
    session
        .raw
        .values()
        .find_map(|snapshot| match snapshot {
            RawSnapshot::ProjectTyped(program)
                if program.module.source_fingerprint == report.input_fingerprint =>
            {
                Some(program.as_ref())
            }
            _ => None,
        })
        .expect("current complete TypedProgram")
}
fn compare(
    session: &CompilationSession,
    report: &CompileReport,
    project: &ProjectSources,
    options: &CompileOptions,
) -> CompileReport {
    let legacy = CompileOptions {
        producer_reuse: false,
        reactive_queries: false,
        semantic_cache: None,
        cancellation: Default::default(),
        ..options.clone()
    };
    let mut fresh = CompilationSession::default();
    let expected = fresh.compile_project(project, &legacy);
    assert_eq!(
        report.status,
        expected.status,
        "{}\n{}",
        report.to_json(),
        expected.to_json()
    );
    assert_eq!(report.phase, expected.phase);
    assert_eq!(report.diagnostics, expected.diagnostics);
    assert_eq!(report.input_fingerprint, expected.input_fingerprint);
    assert_eq!(report.source_files, expected.source_files);
    assert_eq!(report.scalar_interfaces, expected.scalar_interfaces);
    assert_eq!(report.verification_runs, expected.verification_runs);
    assert_eq!(session.active_query_count(), 0);
    assert_eq!(fresh.active_query_count(), 0);
    if report.status == ResultCategory::Success && options.stop_after >= Stage::Types {
        assert_eq!(typed(session, report), typed(&fresh, &expected));
    }
    expected
}

fn compare_path(
    session: &CompilationSession,
    report: &CompileReport,
    path: &Path,
    options: &CompileOptions,
) -> CompileReport {
    let legacy = CompileOptions {
        producer_reuse: false,
        reactive_queries: false,
        semantic_cache: None,
        cancellation: Default::default(),
        ..options.clone()
    };
    let mut fresh = CompilationSession::default();
    let expected = fresh.compile_path(path, &legacy);
    assert_eq!(
        report.status,
        expected.status,
        "{}\n{}",
        report.to_json(),
        expected.to_json()
    );
    assert_eq!(report.phase, expected.phase);
    assert_eq!(report.diagnostics, expected.diagnostics);
    assert_eq!(report.input_fingerprint, expected.input_fingerprint);
    assert_eq!(report.source_files, expected.source_files);
    assert_eq!(report.scalar_interfaces, expected.scalar_interfaces);
    assert_eq!(report.verification_runs, expected.verification_runs);
    assert_eq!(session.active_query_count(), 0);
    assert_eq!(fresh.active_query_count(), 0);
    if report.status == ResultCategory::Success && options.stop_after >= Stage::Types {
        assert_eq!(typed(session, report), typed(&fresh, &expected));
    }
    expected
}

#[test]
fn producer_filesystem_real_io_emit_keeps_module_reuse_and_complete_types() {
    const IO: &str = include_str!("../../../examples/mainline/resource_closures/io.zil");
    const ENTRY: &str = "import left\nimport io\nfn main()->i32{var x:i32=io.read();if x<0{x=0;}let y=left.value(x);io.emit(y&255);return y%127;}";
    for graph in [false, true] {
        let dir = Directory::new();
        let options = options(graph);
        dir.project(&[("main", ENTRY), ("left", LEFT), ("io", IO)]);
        let path = dir.0.join("main.zil");
        let mut session = CompilationSession::default();
        let mut legacy_session = CompilationSession::default();
        let legacy_cold = legacy_session.compile_path(&path, &CompileOptions::default());
        successful(&legacy_cold);
        let cold = session.compile_path(&path, &options);
        successful(&cold);
        compare_path(&session, &cold, &path, &options);
        assert_eq!(cold.source_files.len(), 3);
        let work = cold
            .producer_work
            .as_deref()
            .expect("ordinary io.emit must reach the actual producer");
        assert_eq!(work.normalize_modules, 3);
        assert_eq!(work.lower_modules, 3);
        // Actual input declarations: main has one, left two, and the original
        // IO fixture four (two extern declarations plus read and emit).
        assert_eq!(work.normalized_item_deep_clones, 7);
        assert_eq!(work.restored_module_deep_clones, 0);
        assert_eq!(work.restored_item_deep_clones, 7);
        assert_eq!(work.retained_normalized_items, 0);
        assert_eq!(work.retained_restored_modules, 0);
        let materialization = cold.materialization_work.expect("current scalar admission");
        assert_eq!(materialization.executed_modules, 3);
        assert_eq!(materialization.cloned_modules, 3);
        assert_eq!(materialization.reused_modules, 0);
        let mut previous_left = LEFT.to_owned();
        let mut previous_io = IO.to_owned();
        for (label, left, io, changed_modules) in [
            (
                "equal-width",
                LEFT.replacen("x+1", "x+5", 1),
                IO.to_owned(),
                1u64,
            ),
            // One source literal grows by one byte. This must not invalidate
            // the unchanged main/IO producers through remaining-limit guards.
            (
                "wide-private",
                LEFT.replacen("x+1", "x+10", 1),
                IO.to_owned(),
                1,
            ),
            // Both writes precede this single filesystem compile request. Left
            // grows again; IO gains a real arithmetic expression before putchar.
            (
                "wide-multi",
                LEFT.replacen("x+1", "x+100", 1),
                IO.replace("putchar(value)", "putchar(value + 1)"),
                2,
            ),
        ] {
            assert_ne!(left, previous_left, "{label} actually changes left");
            assert_eq!(
                io != previous_io,
                changed_modules == 2,
                "{label} actual IO change"
            );
            if label != "equal-width" {
                assert!(
                    left.len() > previous_left.len(),
                    "{label} must retain the wider literal"
                );
            }
            fs::write(dir.0.join("left.zil"), &left).unwrap();
            fs::write(dir.0.join("io.zil"), &io).unwrap();
            let legacy = legacy_session.compile_path(&path, &CompileOptions::default());
            let current = session.compile_path(&path, &options);
            successful(&legacy);
            successful(&current);
            compare_path(&session, &current, &path, &options);
            assert_eq!(typed(&session, &current), typed(&legacy_session, &legacy));
            let materialization = current
                .materialization_work
                .expect("current scalar admission");
            assert_eq!(
                materialization.executed_modules as u64, changed_modules,
                "{label}"
            );
            assert_eq!(
                materialization.cloned_modules as u64, changed_modules,
                "{label}"
            );
            assert_eq!(
                materialization.reused_modules as u64,
                3 - changed_modules,
                "{label}"
            );
            let work = current
                .producer_work
                .as_deref()
                .expect("current actual producer work");
            assert_eq!(
                work.normalize_modules, changed_modules,
                "{label}: normalize only changed modules"
            );
            assert_eq!(
                work.lower_modules, changed_modules,
                "{label}: lower only changed modules"
            );
            assert_eq!(
                work.reused_lower_modules,
                3 - changed_modules,
                "{label}: keep unchanged module products"
            );
            assert_eq!(work.cloned_source_map_entries, 0, "{label}: shared maps");
            // Shared item consumers copy only bindings that actually change:
            // equal-width edits change leaf alone; width shifts move value;
            // IO's last emit changes while its three preceding items stay put.
            let (changed_declarations, retained_items) = match label {
                "equal-width" => (1u64, 1u64),
                "wide-private" => (2, 0),
                "wide-multi" => (3, 3),
                _ => unreachable!("fixed source edit cases"),
            };
            assert_eq!(
                work.normalized_item_deep_clones, changed_declarations,
                "{label}: no unchanged declaration may create a mutable Normalize body"
            );
            assert_eq!(
                work.restored_module_deep_clones, 0,
                "{label}: real shared-item consumption never materializes a full Restore Module"
            );
            assert_eq!(
                work.restored_item_deep_clones, changed_declarations,
                "{label}: only current changed item bindings receive mutable Restore bodies"
            );
            assert_eq!(
                work.retained_restored_items, retained_items,
                "{label}: unchanged items inside a changed module retain their real AST owner"
            );
            assert_eq!(
                work.retained_normalized_items,
                7 - changed_declarations,
                "{label}: every unchanged declaration retains its complete normalized Arc"
            );
            assert_eq!(
                work.retained_restored_modules,
                3 - changed_modules,
                "{label}: every unchanged module retains its complete restored Arc"
            );
            let functions = current
                .function_type_work
                .as_deref()
                .expect("actual current Types work");
            let legacy_functions = legacy
                .function_type_work
                .as_deref()
                .expect("actual same-history legacy Types work");
            assert_eq!(functions.core_function_eq_comparisons, 0, "{label}");
            assert!(
                functions.producer_version_reuses > 0,
                "{label}: unchanged main retains its version"
            );
            assert!(
                functions.executed_functions <= legacy_functions.executed_functions,
                "{label}: CFG execution cannot increase over the same-history existing cache"
            );
            if !graph {
                assert_eq!(functions.graph.executed, 0);
            }
            previous_left = left;
            previous_io = io;
        }
    }
}

#[test]
fn producer_revisited_parse_allocation_keeps_materialization_sparse() {
    const IO: &str = include_str!("../../../examples/mainline/resource_closures/io.zil");
    const ENTRY: &str = "import left\nimport io\nfn main()->i32{let x=left.value(io.read());io.emit(x);return x%127;}";
    for graph in [false, true] {
        let dir = Directory::new();
        dir.project(&[("main", ENTRY), ("left", LEFT), ("io", IO)]);
        let path = dir.0.join("main.zil");
        let options = options(graph);
        let mut session = CompilationSession::default();
        let cold = session.compile_path(&path, &options);
        successful(&cold);
        let edited_left = LEFT.replacen("x+1", "x+10", 1);
        fs::write(dir.0.join("left.zil"), &edited_left).unwrap();
        let private = session.compile_path(&path, &options);
        successful(&private);
        compare_path(&session, &private, &path, &options);
        // Revisit a previously cached project before a batch that repeats the
        // private bytes. Discovery constructs a new sealed parser owner, while
        // the query database may still return an older equivalent Parse Arc.
        fs::write(dir.0.join("left.zil"), LEFT).unwrap();
        let restored = session.compile_path(&path, &options);
        successful(&restored);
        assert!(
            restored.producer_work.is_none(),
            "named parent is a project hit"
        );
        fs::write(dir.0.join("left.zil"), &edited_left).unwrap();
        fs::write(
            dir.0.join("io.zil"),
            IO.replace("putchar(value)", "putchar(value + 1)"),
        )
        .unwrap();
        let batch = session.compile_path(&path, &options);
        successful(&batch);
        compare_path(&session, &batch, &path, &options);
        let work = batch
            .materialization_work
            .expect("current sealed materialization");
        assert_eq!(work.executed_modules, 2);
        assert_eq!(work.cloned_modules, 2);
        assert_eq!(work.reused_modules, 1);
    }
}

#[test]
fn producer_borrowed_bodies_reenter_full_restore_on_insufficient_budget() {
    let directory = Directory::new();
    let target = directory.scalar(MAIN, LEFT, RIGHT);
    // Keep the token quota identical so right really retains its parser
    // revision. Extra module separators add no placeholder body or statement.
    let cheaper_left = format!("{};;", LEFT.replacen("return x+1;", "return x;", 1));
    let cheaper = directory.scalar(MAIN, &cheaper_left, RIGHT);
    let full_parse =
        zil_syntax::parse_with_limits(&target.modules[1], &CompileOptions::default().limits);
    let cheap_parse =
        zil_syntax::parse_with_limits(&cheaper.modules[1], &CompileOptions::default().limits);
    assert!(full_parse.diagnostics.is_empty());
    assert!(cheap_parse.diagnostics.is_empty());
    assert_eq!(full_parse.tokens.len(), cheap_parse.tokens.len());
    for nodes in [false, true] {
        let n = budget_boundary(&target, Stage::Core, nodes);
        for graph in [false, true] {
            let mut limited = CompileOptions {
                stop_after: Stage::Core,
                ..options(graph)
            };
            set_budget(&mut limited, nodes, n - 1);
            let mut session = CompilationSession::default();
            let warm = session.compile_project(&cheaper, &limited);
            assert_eq!(warm.status, ResultCategory::Success, "{}", warm.to_json());
            compare(&session, &warm, &cheaper, &limited);
            let current = session.compile_project(&target, &limited);
            compare(&session, &current, &target, &limited);
            assert_eq!(current.status, ResultCategory::ResourceLimit);
            assert!(!current.diagnostics.is_empty());
            let work = current
                .producer_work
                .as_deref()
                .expect("actual failed producer work");
            assert!(
                work.retained_normalized_items > 0,
                "the rejected request must actually borrow complete normalized bodies"
            );
            assert!(
                work.retained_restored_modules > 0,
                "a complete unchanged module must be retained before the failing stage"
            );
            assert!(
                work.restored_item_deep_clones > 1,
                "beyond the changed left item, insufficient receipts must visit a complete retained item's normalized body"
            );
            assert_eq!(work.restored_module_deep_clones, 0);
            assert_eq!(current.verification_runs, 0);
            assert_eq!(session.active_query_count(), 0);
            let restored = session.compile_project(&cheaper, &limited);
            assert_eq!(
                restored.status,
                ResultCategory::Success,
                "{}",
                restored.to_json()
            );
            compare(&session, &restored, &cheaper, &limited);
        }
    }
}

#[test]
fn producer_filesystem_ordinary_materializer_identifier_candidates_match_fresh() {
    // These identifier spellings hit the old materializer's conservative token
    // gate. Their declarations/calls are ordinary scalar syntax, not CT syntax.
    for name in ["emit", "quote", "splice", "Type", "Syntax", "Ident"] {
        for graph in [false, true] {
            let dir = Directory::new();
            let options = options(graph);
            let main = MAIN.replace("left.value", &format!("left.{name}"));
            let left = LEFT.replace("pub fn value", &format!("pub fn {name}"));
            dir.scalar(&main, &left, RIGHT);
            let path = dir.0.join("main.zil");
            let mut session = CompilationSession::default();
            let cold = session.compile_path(&path, &options);
            successful(&cold);
            compare_path(&session, &cold, &path, &options);
            let work = cold
                .producer_work
                .as_deref()
                .expect("ordinary identifier must not suppress the sealed scalar producer");
            assert_eq!(work.lower_modules, 3, "ordinary {name}");
            fs::write(dir.0.join("left.zil"), left.replace("x+1", "x+5")).unwrap();
            let current = session.compile_path(&path, &options);
            successful(&current);
            compare_path(&session, &current, &path, &options);
            let work = current
                .producer_work
                .as_deref()
                .expect("actual producer after ordinary-name edit");
            assert_eq!(work.lower_modules, 1, "ordinary {name}");
            assert_eq!(work.reused_lower_modules, 2, "ordinary {name}");
            let functions = current
                .function_type_work
                .as_deref()
                .expect("actual Types work");
            assert_eq!(functions.core_function_eq_comparisons, 0);
            assert!(functions.producer_version_reuses > 0);
        }
    }
}

#[test]
fn producer_filesystem_ordinary_emit_preserves_materializer_budget_n_n_minus_one() {
    let dir = Directory::new();
    dir.project(&[
        (
            "main",
            "import left\nimport io\nfn main()->i32{let x=left.value(1);io.emit(x);return x;}",
        ),
        (
            "left",
            "pub fn value(x:i32)->i32{if x<3{return x+1;}return x+2;}",
        ),
        (
            "io",
            include_str!("../../../examples/mainline/resource_closures/io.zil"),
        ),
    ]);
    let path = dir.0.join("main.zil");
    // Core includes the existing conservative materializer's classification
    // and Expand costs. The no-op producer bridge must not erase either bill.
    for nodes in [false, true] {
        let mut legacy = CompileOptions {
            stop_after: Stage::Core,
            ..Default::default()
        };
        let mut low = 0;
        let mut high = if nodes {
            legacy.limits.max_nodes
        } else {
            legacy.limits.max_analysis_steps
        };
        let full = CompilationSession::default().compile_path(&path, &legacy);
        assert_eq!(full.status, ResultCategory::Success, "{}", full.to_json());
        while high - low > 1 {
            let middle = low + (high - low) / 2;
            set_budget(&mut legacy, nodes, middle);
            let current = CompilationSession::default().compile_path(&path, &legacy);
            if current.status == ResultCategory::Success {
                high = middle;
            } else {
                assert_eq!(
                    current.status,
                    ResultCategory::ResourceLimit,
                    "{}",
                    current.to_json()
                );
                low = middle;
            }
        }
        assert!(high > 0);
        for graph in [false, true] {
            for limit in [high, high - 1, high] {
                let mut limited = CompileOptions {
                    stop_after: Stage::Core,
                    ..options(graph)
                };
                set_budget(&mut limited, nodes, limit);
                // A fresh producer session at each boundary tests the cold
                // routing cost, without hiding it behind a previous receipt.
                let mut session = CompilationSession::default();
                let current = session.compile_path(&path, &limited);
                let expected = compare_path(&session, &current, &path, &limited);
                assert_eq!(current.status == ResultCategory::Success, limit == high);
                assert_eq!(
                    current.phase, expected.phase,
                    "retain the original first failing phase"
                );
                assert_eq!(current.verification_runs, 0);
                if limit != high {
                    assert_eq!(current.status, ResultCategory::ResourceLimit);
                    assert!(!current.diagnostics.is_empty());
                }
            }
        }
    }
}

#[test]
fn producer_ordinary_emit_does_not_restore_unreachable_materializer_pruned_bodies() {
    for graph in [false, true] {
        let directory = Directory::new();
        let options = options(graph);
        let mut session = CompilationSession::default();
        for (argument, missing) in [(1, "absent_name"), (2, "still_absent")] {
            let main = format!(
                "fn emit(x:i32)->i32{{if x<7{{return x+1;}}return x+2;}}fn main()->i32{{return emit({argument});}}"
            );
            let orphan = format!("fn orphan()->i32{{return {missing};}}");
            // No import reaches orphan. The existing materializer deliberately
            // prunes its invalid body; original sealed syntax is not a no-op.
            let project = directory.project(&[("main", &main), ("orphan", &orphan)]);
            let report = session.compile_project(&project, &options);
            successful(&report);
            compare(&session, &report, &project, &options);
            assert!(
                report.producer_work.is_none(),
                "materialized pruning must fall back instead of reviving the invalid orphan"
            );
        }
    }
}

#[test]
fn producer_private_atomic_multi_and_restore_match_complete_fresh_types() {
    for graph in [false, true] {
        let dir = Directory::new();
        let options = options(graph);
        let mut session = CompilationSession::default();
        let initial = dir.scalar(MAIN, LEFT, RIGHT);
        let cold = session.compile_project(&initial, &options);
        successful(&cold);
        compare(&session, &cold, &initial, &options);
        for (left, right) in [
            (LEFT.replace("x+1", "x+5"), RIGHT.to_owned()),
            (LEFT.replace("x+1", "x+6"), RIGHT.replace("x+3", "x+8")),
            (LEFT.replace("x+1", "x+10"), RIGHT.to_owned()),
            (LEFT.to_owned(), RIGHT.to_owned()),
        ] {
            let project = dir.scalar(MAIN, &left, &right);
            let current = session.compile_project(&project, &options);
            successful(&current);
            compare(&session, &current, &project, &options);
        }
        let unchanged = session.compile_project(&initial, &options);
        successful(&unchanged);
        compare(&session, &unchanged, &initial, &options);
        assert!(
            unchanged.function_type_work.is_none(),
            "whole-project Types hit cannot publish stale work"
        );
    }
}

#[test]
fn producer_equal_width_edit_skips_real_module_work_and_uses_versions_without_graph() {
    for graph in [false, true] {
        let dir = Directory::new();
        let options = options(graph);
        let mut session = CompilationSession::default();
        let initial = dir.scalar(MAIN, LEFT, RIGHT);
        let cold = session.compile_project(&initial, &options);
        successful(&cold);
        let cold_work = cold
            .producer_work
            .as_deref()
            .expect("executed producer counters");
        assert_eq!(cold_work.normalize_modules, 3);
        assert_eq!(cold_work.lower_modules, 3);
        let changed = dir.scalar(MAIN, &LEFT.replace("x+1", "x+5"), RIGHT);
        let current = session.compile_project(&changed, &options);
        successful(&current);
        let fresh = compare(&session, &current, &changed, &options);
        let work = current
            .producer_work
            .as_deref()
            .expect("current producer counters");
        assert_eq!(work.normalize_modules, 1, "only changed left is normalized");
        assert_eq!(work.lower_modules, 1, "only changed left is lowered");
        assert_eq!(work.reused_lower_modules, 2);
        assert!(work.receipt_hits > 0);
        assert!(work.visited_nodes < cold_work.visited_nodes);
        assert!(
            work.cloned_source_map_entries == 0
                && work.retained_lower_cores > 0
                && work.retained_lower_source_maps > 0,
            "producer assembly retains unchanged Core and source maps"
        );
        let functions = current
            .function_type_work
            .as_deref()
            .expect("actual current Types work");
        let full = fresh
            .function_type_work
            .as_deref()
            .expect("actual fresh Types work");
        assert!(functions.producer_version_reuses > 0);
        assert!(functions.body_key_executions < full.body_key_executions);
        assert_eq!(functions.core_function_eq_comparisons, 0);
        if !graph {
            assert_eq!(
                functions.graph.executed, 0,
                "B uses direct version memoization"
            );
        }
    }
}

#[test]
fn producer_declarations_imports_absence_visibility_and_compact_ids_match_fresh() {
    for graph in [false, true] {
        let dir = Directory::new();
        let options = options(graph);
        let mut session = CompilationSession::default();
        successful(&session.compile_project(&dir.scalar(MAIN, LEFT, RIGHT), &options));
        for (main, left, right) in [
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
                MAIN.to_owned(),
                LEFT.replace(
                    "fn leaf(x:i32)->i32{if x<7{return x+1;}else{return x+2;}} ",
                    "",
                ),
                RIGHT.to_owned(),
            ),
            (
                MAIN.replace("import left\nimport right", "import right\nimport left"),
                LEFT.to_owned(),
                RIGHT.to_owned(),
            ),
            (
                MAIN.replace("left.value(x)", "right.value(x)")
                    .replace("import left\n", ""),
                LEFT.to_owned(),
                RIGHT.to_owned(),
            ),
            (
                format!("// shifted root\n\n{MAIN}"),
                format!("// shifted left\n\n{LEFT}"),
                format!("// shifted right\n\n{RIGHT}"),
            ),
            (MAIN.to_owned(), LEFT.to_owned(), RIGHT.to_owned()),
        ] {
            let project = dir.scalar(&main, &left, &right);
            let current = session.compile_project(&project, &options);
            compare(&session, &current, &project, &options);
        }
        // Add/remove a module, rename logical import and source path together.
        for project in [
            dir.project(&[
                ("main", MAIN),
                ("left", LEFT),
                ("right", RIGHT),
                ("added", "pub fn extra()->i32=>9"),
            ]),
            dir.project(&[
                ("main", &MAIN.replace("left", "renamed")),
                ("renamed", LEFT),
                ("right", RIGHT),
            ]),
            dir.scalar(MAIN, LEFT, RIGHT),
        ] {
            let current = session.compile_project(&project, &options);
            successful(&current);
            compare(&session, &current, &project, &options);
        }
        let mut rebound = dir.scalar(MAIN, LEFT, RIGHT);
        rebound.root = ModuleId(30);
        for (i, source) in rebound.modules.iter_mut().enumerate() {
            source.module_id = ModuleId(30 + i as u32);
            source.file_id = FileId(90 + i as u32);
        }
        rebound.paths = BTreeMap::from([
            (vec!["left".into()], ModuleId(31)),
            (vec!["right".into()], ModuleId(32)),
        ]);
        let current = session.compile_project(&rebound, &options);
        successful(&current);
        compare(&session, &current, &rebound, &options);
    }
}

#[test]
fn producer_errors_repair_and_shifted_locations_retain_exact_diagnostics() {
    for graph in [false, true] {
        let dir = Directory::new();
        let options = options(graph);
        let mut session = CompilationSession::default();
        successful(&session.compile_project(&dir.scalar(MAIN, LEFT, RIGHT), &options));
        for left in [
            LEFT.replace("return x+1;", "return absent_name;"),
            format!(
                "// shifted\n\n{}",
                LEFT.replace("return x+1;", "return absent_name;")
            ),
            LEFT.replace("return x+1;", "return x+8;"),
            LEFT.replace("return x+1;", "return true;"),
            LEFT.replace("return x+1;", "return x+;"),
            format!("// all repaired\n\n{LEFT}"),
            LEFT.to_owned(),
        ] {
            let project = dir.scalar(MAIN, &left, RIGHT);
            let current = session.compile_project(&project, &options);
            compare(&session, &current, &project, &options);
            if left.contains("absent_name") {
                assert_eq!(current.status, ResultCategory::SafeReject);
                let diagnostic = current
                    .diagnostics
                    .iter()
                    .find(|d| d.code == "E-UNKNOWN-NAME")
                    .expect("current absent name rejection");
                let span = diagnostic.span.expect("exact failing source span");
                assert_eq!(span.file, project.modules[1].file_id);
                assert_eq!(&left[span.start..span.end], "absent_name");
                assert_eq!(current.source_files[&span.file], project.modules[1].path);
            }
        }
    }
}

fn set_budget(options: &mut CompileOptions, nodes: bool, value: usize) {
    if nodes {
        options.limits.max_nodes = value;
    } else {
        options.limits.max_analysis_steps = value;
    }
}
fn budget_boundary(project: &ProjectSources, stage: Stage, nodes: bool) -> usize {
    let mut options = CompileOptions {
        stop_after: stage,
        ..Default::default()
    };
    let mut low = 0;
    let mut high = if nodes {
        options.limits.max_nodes
    } else {
        options.limits.max_analysis_steps
    };
    let full = CompilationSession::default().compile_project(project, &options);
    assert_eq!(full.status, ResultCategory::Success, "{}", full.to_json());
    while high - low > 1 {
        let middle = low + (high - low) / 2;
        set_budget(&mut options, nodes, middle);
        let report = CompilationSession::default().compile_project(project, &options);
        if report.status == ResultCategory::Success {
            high = middle;
        } else {
            assert_eq!(
                report.status,
                ResultCategory::ResourceLimit,
                "{}",
                report.to_json()
            );
            low = middle;
        }
    }
    assert!(high > 0);
    high
}

#[test]
fn producer_expand_and_complete_budget_n_n_minus_one_match_first_fresh_error() {
    let dir = Directory::new();
    let initial = dir.scalar(MAIN, LEFT, RIGHT);
    let changed = dir.scalar(MAIN, &LEFT.replace("x+1", "x+9"), RIGHT);
    let cheaper = dir.scalar(
        MAIN,
        "fn leaf(x:i32)->i32{return x;} pub fn value(x:i32)->i32{if x<3{return leaf(x);}else{return leaf(x+1);}}",
        RIGHT,
    );
    // Core isolates Expand's cumulative budget; Verify also checks downstream
    // consumers. Per-segment Normalize/Restore/Lower receipts are tested in Expand.
    for stage in [Stage::Core, Stage::Verify] {
        for nodes in [false, true] {
            let n = budget_boundary(&changed, stage, nodes);
            for graph in [false, true] {
                for limit in [n, n - 1, n] {
                    // Limits participate in the producer environment. Warm
                    // with THIS limit so the edit exercises actual receipts.
                    let mut limited = CompileOptions {
                        stop_after: stage,
                        ..options(graph)
                    };
                    set_budget(&mut limited, nodes, limit);
                    let parent = if limit == n { &initial } else { &cheaper };
                    let mut session = CompilationSession::default();
                    let warm = session.compile_project(parent, &limited);
                    assert_eq!(
                        warm.status,
                        ResultCategory::Success,
                        "same-limit parent must fit: {}",
                        warm.to_json()
                    );
                    compare(&session, &warm, parent, &limited);
                    let current = session.compile_project(&changed, &limited);
                    compare(&session, &current, &changed, &limited);
                    assert_eq!(current.status == ResultCategory::Success, limit == n);
                    if limit == n {
                        assert!(
                            current
                                .producer_work
                                .as_deref()
                                .expect("current executed producer")
                                .receipt_hits
                                > 0,
                            "N must reuse actual same-limit receipts"
                        );
                    } else {
                        assert!(!current.diagnostics.is_empty());
                        assert_ne!(
                            current.input_fingerprint, warm.input_fingerprint,
                            "failed edit retains its own source identity"
                        );
                        let rejected_fingerprint = current.input_fingerprint;
                        let mut retry_options = limited.clone();
                        set_budget(&mut retry_options, nodes, n);
                        let retry = session.compile_project(&changed, &retry_options);
                        assert_eq!(retry.status, ResultCategory::Success, "{}", retry.to_json());
                        assert_eq!(retry.input_fingerprint, rejected_fingerprint);
                        compare(&session, &retry, &changed, &retry_options);
                    }
                    let mut fresh = CompilationSession::default();
                    let current = fresh.compile_project(&changed, &limited);
                    compare(&fresh, &current, &changed, &limited);
                }
            }
        }
    }
}

#[test]
fn producer_cancel_retry_clear_and_eviction_do_not_publish_partial_products() {
    for graph in [false, true] {
        let dir = Directory::new();
        let options = options(graph);
        let mut session = CompilationSession::with_capacity(8);
        let initial = dir.scalar(MAIN, LEFT, RIGHT);
        successful(&session.compile_project(&initial, &options));
        for phase in [Phase::Expand, Phase::Types, Phase::Analyze, Phase::Verify] {
            let project = dir.scalar(
                MAIN,
                &LEFT.replace("x+1", &format!("x+{}", phase as usize + 10)),
                RIGHT,
            );
            let cancelled = CompileOptions {
                cancellation: Default::default(),
                ..options.clone()
            };
            let report =
                session.compile_project_with_observer(&project, &cancelled, &mut |event| {
                    if event.phase == phase {
                        cancelled.cancellation.cancel();
                    }
                });
            assert_eq!(
                report.status,
                ResultCategory::Cancelled,
                "{}",
                report.to_json()
            );
            assert_eq!(session.active_query_count(), 0);
            let retry = session.compile_project(&project, &options);
            successful(&retry);
            compare(&session, &retry, &project, &options);
        }
        for i in 0..20 {
            let project = dir.scalar(MAIN, &LEFT.replace("x+1", &format!("x+{}", i + 30)), RIGHT);
            let current = session.compile_project(&project, &options);
            successful(&current);
            compare(&session, &current, &project, &options);
            assert!(session.cached_query_count() <= 8);
        }
        session.clear();
        assert_eq!(session.cached_query_count(), 0);
        let restored = session.compile_project(&initial, &options);
        successful(&restored);
        compare(&session, &restored, &initial, &options);
    }
}

#[test]
fn producer_rich_fallback_ct_traits_generics_and_owned_once_match_fresh() {
    // These are fallback controls, never evidence that producer reuse supports
    // CT selection, nominal solving, resource closure state or instance graphs.
    const TRAIT: &str = "trait Read{fn read(self:&Self)->i32;}struct Point{value:i32}impl Read for Point{fn read()->i32=>.value}fn read<T:Read>(value:&T)->i32{return value.read()}fn main()->i32{let p=Point{value:7};return read(&p)}";
    const OWNED: &str = "import left\nimport right\nresource Token{n:i32}fn consume(give token:Token)->i32{return token.n}fn make(n:i32)->impl FnOnce()->i32{let token=Token{n};return move ()=>consume(give token)}#if(left.enabled()){fn selected()->i32{return left.value(1)}}else{fn selected()->i32{return right.value(2)}}fn main()->i32{let f=make(5);return f()+selected()}";
    for graph in [false, true] {
        let dir = Directory::new();
        let options = options(graph);
        let mut session = CompilationSession::default();
        for main in [
            TRAIT.to_owned(),
            TRAIT.replace("value:7", "value:8"),
            TRAIT.replace("impl Read for Point", "impl Missing for Point"),
            TRAIT.to_owned(),
        ] {
            let project = dir.scalar(&main, LEFT, RIGHT);
            let current = session.compile_project(&project, &options);
            compare(&session, &current, &project, &options);
            if main.contains("impl Missing") {
                assert_eq!(current.status, ResultCategory::SafeReject);
            } else {
                successful(&current);
            }
        }
        for (enabled, twice) in [(true, false), (false, false), (false, true), (true, false)] {
            let main = if twice {
                OWNED.replace("return f()+selected()", "return f()+f()+selected()")
            } else {
                OWNED.to_owned()
            };
            let left = format!("{LEFT}\npub fn enabled()->bool{{return {enabled};}}");
            let project = dir.scalar(&main, &left, RIGHT);
            let current = session.compile_project(&project, &options);
            compare(&session, &current, &project, &options);
            if twice {
                assert_eq!(current.status, ResultCategory::SafeReject);
                let diagnostic = current
                    .diagnostics
                    .iter()
                    .find(|d| d.phase == Phase::Analyze && d.code == "E-UNINITIALIZED")
                    .expect("second FnOnce call rejection");
                let start = main.find("f()+f()").unwrap() + 4;
                assert_eq!(
                    diagnostic.span,
                    Some(zil_base::Span::new(
                        project.modules[0].file_id,
                        start,
                        start + 3
                    ))
                );
            } else {
                successful(&current);
            }
            if let Some(work) = current.function_type_work.as_deref() {
                assert_eq!(
                    work.graph.executed, 0,
                    "rich fallback must not claim graph execution"
                );
                assert_eq!(work.body_key_reuses, 0);
            }
            if let Some(work) = &current.producer_work {
                assert_eq!(
                    work.reused_lower_modules, 0,
                    "rich fallback cannot claim module reuse"
                );
                assert_eq!(
                    work.normalize_modules, 0,
                    "rich fallback executes the legacy expander"
                );
            }
        }
    }
}

#[test]
fn producer_dynamic_calls_and_mutual_recursion_preserve_actual_targets() {
    const RECURSIVE: &str = "pub fn one(x:i32)->i32{if x<1{return 1;}else{return two(x-1);}} pub fn two(x:i32)->i32{if x<1{return 2;}else{return one(x-1);}} pub fn value(x:i32)->i32{if x<3{return one(x);}else{return two(x);}}";
    for graph in [false, true] {
        let dir = Directory::new();
        let options = options(graph);
        let mut session = CompilationSession::default();
        successful(&session.compile_project(&dir.scalar(MAIN, RECURSIVE, RIGHT), &options));
        for i in 0..6 {
            let main = MAIN.replace(
                "left.value(x)",
                if i % 2 == 0 {
                    "left.one(x)"
                } else {
                    "right.value(x)"
                },
            );
            let left = RECURSIVE.replace("return 1;", &format!("return {};", i + 3));
            let project = dir.scalar(&main, &left, RIGHT);
            let current = session.compile_project(&project, &options);
            successful(&current);
            compare(&session, &current, &project, &options);
        }
    }
}

#[test]
fn producer_restart_and_corrupt_disk_cache_still_require_current_types_and_verify() {
    for graph in [false, true] {
        let dir = Directory::new();
        let options = CompileOptions {
            semantic_cache: Some(dir.0.join("cache")),
            ..options(graph)
        };
        let initial = dir.scalar(MAIN, LEFT, RIGHT);
        let mut original = CompilationSession::default();
        successful(&original.compile_project(&initial, &options));
        for corrupt in [false, true] {
            if corrupt {
                let files: Vec<_> = fs::read_dir(dir.0.join("cache/function-types-v2"))
                    .unwrap()
                    .map(|entry| entry.unwrap().path())
                    .filter(|path| path.is_file())
                    .collect();
                assert!(!files.is_empty());
                for path in files {
                    fs::write(path, b"invalid untrusted transport").unwrap();
                }
            }
            let left = LEFT.replace("x+1", if corrupt { "x+9" } else { "x+8" });
            let project = dir.scalar(MAIN, &left, RIGHT);
            let mut restarted = CompilationSession::default();
            let current = restarted.compile_project(&project, &options);
            successful(&current);
            compare(&restarted, &current, &project, &options);
        }
    }
}

#[test]
fn actual_function_products_reach_analysis_and_independent_verification() {
    let dir = Directory::new();
    let options = options(true);
    let mut session = CompilationSession::default();
    let initial = dir.scalar(MAIN, LEFT, RIGHT);
    let cold = session.compile_project(&initial, &options);
    successful(&cold);
    assert_eq!(
        cold.initialization_work
            .unwrap()
            .candidate_executed_functions,
        5
    );
    assert_eq!(cold.verification_work.unwrap().function_proofs_checked, 5);
    let warm = session.compile_project(&initial, &options);
    successful(&warm);
    assert_eq!(warm.verification_work.unwrap().full_receipts_reused, 1);

    // A wider private edit shifts the sibling function's diagnostic positions.
    // Its raw source binding must change without forcing its semantic analyses.
    let changed = dir.scalar(MAIN, &LEFT.replacen("x+1", "x+11", 1), RIGHT);
    let current = session.compile_project(&changed, &options);
    successful(&current);
    compare(&session, &current, &changed, &options);
    let cfg = current.function_type_work.as_ref().unwrap();
    assert_eq!((cfg.executed_functions, cfg.reused_functions), (1, 4));
    assert_eq!((cfg.body_key_executions, cfg.body_key_reuses), (1, 4));
    assert!(cfg.cfg_relocated_functions > 0);
    let analysis = current.initialization_work.unwrap();
    assert_eq!(
        (
            analysis.candidate_executed_functions,
            analysis.candidate_reused_functions
        ),
        (1, 4)
    );
    let verification = current.verification_work.unwrap();
    assert_eq!(
        (
            verification.function_proofs_checked,
            verification.function_proofs_reused
        ),
        (1, 4)
    );
    assert_eq!(verification.full_receipts_reused, 0);

    let broken = dir.scalar(MAIN, &LEFT.replacen("x+1", "missing(x)", 1), RIGHT);
    let failed = session.compile_project(&broken, &options);
    assert_ne!(failed.status, ResultCategory::Success);
    let recovered = session.compile_project(&changed, &options);
    successful(&recovered);
    compare(&session, &recovered, &changed, &options);
    assert_eq!(recovered.verification_work.unwrap().full_receipts_reused, 1);
}

#[test]
fn relative_scalar_versions_never_admit_constants_ct_generics_or_capture_rewrites() {
    let families = [
        (
            "const N:i32=7;fn main()->i32{return N}",
            "const N:i32=8;fn main()->i32{return N}",
            false,
        ),
        (
            "#if(true){fn selected()->i32=>7;}else{fn selected()->i32=>9;}fn main()->i32{return selected()}",
            "#if(false){fn selected()->i32=>7;}else{fn selected()->i32=>9;}fn main()->i32{return selected()}",
            true,
        ),
        (
            "fn identity<T>(x:T)->T=>x;fn main()->i32{return identity::<i32>(7)}",
            "fn identity<T>(x:T)->T=>x;fn main()->i32{return identity::<i32>(8)}",
            true,
        ),
        (
            "fn main()->i32{let x=7;let f=()=>x;return f();}",
            "fn main()->i32{let x=8;let f=()=>x;return f();}",
            true,
        ),
    ];
    for graph in [false, true] {
        for (first, changed, supported) in families {
            let dir = Directory::new();
            let options = options(graph);
            let mut session = CompilationSession::default();
            successful(&session.compile_project(&dir.scalar(MAIN, LEFT, RIGHT), &options));
            for main in [first, changed] {
                let current_sources = dir.scalar(main, LEFT, RIGHT);
                let report = session.compile_project(&current_sources, &options);
                compare(&session, &report, &current_sources, &options);
                if supported {
                    successful(&report);
                } else {
                    assert_eq!(report.status, ResultCategory::Unsupported);
                    assert_eq!(report.phase, Phase::Types);
                    assert_eq!(report.verification_runs, 0);
                }
                if let Some(work) = &report.function_type_work {
                    assert_eq!(work.producer_version_reuses, 0);
                    if work.body_key_reuses > 0 {
                        // Ordinary graph fallback may still validate unchanged
                        // complete raw Core after CT/instance materialization.
                        // It must not claim a sealed scalar input-version hit.
                        assert!(work.core_function_eq_comparisons > 0);
                    }
                }
                if let Some(work) = &report.producer_work {
                    assert_eq!(work.function_input_reuses, 0);
                    assert_eq!(work.reused_lower_modules, 0);
                }
            }
        }
    }
}

#[cfg(all(feature = "llvm-api", target_os = "linux"))]
#[path = "mixed_machine_reuse_tests.rs"]
mod mixed_machine_reuse_tests;
