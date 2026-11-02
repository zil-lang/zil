use std::{
    collections::BTreeMap,
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_base::{FileId, Fingerprint, ModuleId, Phase, ResultCategory, SourceUnit};
use zil_driver::{
    CompilationSession, CompileOptions, CompileReport, ProjectSources, QueryDisposition,
};

static NEXT: AtomicUsize = AtomicUsize::new(0);

#[derive(Clone, Copy, Debug)]
enum Input {
    Project,
    Filesystem,
}

struct Case {
    dir: PathBuf,
    project: ProjectSources,
}

impl Case {
    fn new(modules: usize) -> Self {
        let dir = std::env::var_os("ZIL_TEST_REPORT_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir)
            .join(format!(
                "zil-project-syntax-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
        fs::create_dir_all(&dir).unwrap();
        let mut root = (1..modules)
            .map(|i| format!("import m{i}\n"))
            .collect::<String>();
        root += "fn value()->i32{var total:i32=0;\n";
        for i in 1..modules {
            root += &format!("total=total+m{i}.value();\n");
        }
        root += "return total;}\nextern \"C\" fn putchar(v:i32)->i32;\n";
        root += "fn main()->i32{let result=value();unsafe{";
        for shift in [0, 8, 16, 24] {
            root += &format!("putchar((result>>{shift})&255);");
        }
        root += "}return 0;}\n";
        let mut paths = BTreeMap::new();
        let sources = (0..modules)
            .map(|i| {
                let name = if i == 0 {
                    "main".to_owned()
                } else {
                    format!("m{i}")
                };
                paths.insert(vec![name.clone()], ModuleId(i as u32));
                let path = dir.join(format!("{name}.zil"));
                let text = if i == 0 { root.clone() } else { leaf(i) };
                fs::write(&path, &text).unwrap();
                SourceUnit::new(
                    FileId(i as u32),
                    ModuleId(i as u32),
                    path.to_string_lossy(),
                    text,
                )
            })
            .collect();
        Self {
            dir,
            project: ProjectSources {
                root: ModuleId(0),
                modules: sources,
                paths,
            },
        }
    }

    fn put(&mut self, index: usize, text: String) {
        fs::write(&self.project.modules[index].path, &text).unwrap();
        self.project.modules[index].text = text;
    }

    fn compile(
        &self,
        session: &mut CompilationSession,
        options: &CompileOptions,
        input: Input,
    ) -> CompileReport {
        match input {
            Input::Project => session.compile_project(&self.project, options),
            Input::Filesystem => session.compile_path(&self.dir.join("main.zil"), options),
        }
    }

    fn fresh(&self, options: &CompileOptions, input: Input) -> CompileReport {
        self.compile(&mut CompilationSession::default(), options, input)
    }
}

impl Drop for Case {
    fn drop(&mut self) {
        if std::thread::panicking() {
            eprintln!("project syntax fixture retained: {}", self.dir.display());
        } else {
            let _ = fs::remove_dir_all(&self.dir);
        }
    }
}

fn leaf(value: usize) -> String {
    format!(
        "pub fn helper(x:i32)->i32{{return x+{value};}}\npub fn value()->i32{{return helper(0);}}\n"
    )
}

fn success(report: &CompileReport) {
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1, "{}", report.to_json());
}

fn equivalent(warm: &CompileReport, fresh: &CompileReport) {
    assert_eq!(
        warm.status,
        fresh.status,
        "{}\n{}",
        warm.to_json(),
        fresh.to_json()
    );
    assert_eq!(warm.phase, fresh.phase);
    assert_eq!(warm.diagnostics, fresh.diagnostics);
    assert_eq!(warm.source_files, fresh.source_files);
    assert_eq!(warm.input_fingerprint, fresh.input_fingerprint);
    assert_eq!(warm.scalar_interfaces, fresh.scalar_interfaces);
    assert_eq!(warm.verification_runs, fresh.verification_runs);
}

fn parses(report: &CompileReport) -> (usize, usize) {
    let mut counts = (0, 0);
    for event in &report.query_events {
        if event.phase == Phase::Parse {
            match event.disposition {
                QueryDisposition::Executed => counts.0 += 1,
                QueryDisposition::Reused => counts.1 += 1,
            }
        }
    }
    counts
}

fn bounded(session: &CompilationSession, capacity: usize) {
    assert!(session.cached_query_count() <= capacity);
    assert_eq!(session.active_query_count(), 0);
}

#[test]
fn changed_modules_reuse_syntax_beyond_raw_capacity_in_both_entry_paths() {
    // This includes the actual 1024 imported-module failure shape, without
    // multiplying every size/capacity/input combination into a stress matrix.
    for (modules, capacity, input) in [
        (40, 8, Input::Project),
        (128, 32, Input::Filesystem),
        (1025, 32, Input::Project),
        (1025, 8, Input::Filesystem),
    ] {
        let mut case = Case::new(modules);
        let options = CompileOptions::default();
        let mut session = CompilationSession::with_capacity(capacity);
        let cold = case.compile(&mut session, &options, input);
        success(&cold);
        assert_eq!(parses(&cold), (modules, 0));
        let original_root = case.project.modules[0].text.clone();
        let edits = [
            (
                0,
                format!("// changes the remaining front-end quota\n{original_root}"),
            ),
            (1, leaf(1001)),
            (
                1,
                "pub fn helper(x:i32,extra:i32)->i32{return x+extra+1;}\n\
                 pub fn value()->i32{return helper(0,0);}\n"
                    .into(),
            ),
            (modules - 1, leaf(2001)),
        ];
        for (index, text) in edits {
            case.put(index, text);
            let changed = case.compile(&mut session, &options, input);
            success(&changed);
            assert_eq!(parses(&changed), (1, modules - 1), "{input:?} {modules}");
            equivalent(&changed, &case.fresh(&options, input));
            bounded(&session, capacity);
        }
        case.put(1, leaf(1));
        let restored = case.compile(&mut session, &options, input);
        success(&restored);
        equivalent(&restored, &case.fresh(&options, input));
        assert!(parses(&restored).0 <= 1);
        bounded(&session, capacity);
        session.clear();
        assert_eq!(session.cached_query_count(), 0);
        let reset = case.compile(&mut session, &options, input);
        success(&reset);
        assert_eq!(parses(&reset), (modules, 0));
    }
}

#[test]
fn retained_syntax_checks_current_frontend_quota_and_first_error() {
    for input in [Input::Project, Input::Filesystem] {
        let mut case = Case::new(40);
        // Use a fully accepted project to retain its Expand owner. Attribute
        // parser work is covered separately by cp33_parse_budget; unexecuted
        // #[doc] attributes cannot seed a successful project syntax epoch.
        case.put(39, leaf(39).trim_end().to_owned());
        let base = CompileOptions::default();
        let tokens: usize = case
            .project
            .modules
            .iter()
            .map(|source| zil_syntax::parse(source).tokens.len())
            .sum();
        for node_budget in [false, true] {
            for quota in [tokens - 1, tokens, tokens + 1] {
                let mut session = CompilationSession::with_capacity(8);
                success(&case.compile(&mut session, &base, input));
                let mut limited = base.clone();
                if node_budget {
                    limited.limits.max_nodes = quota;
                } else {
                    limited.limits.max_tokens = quota;
                }
                let warm = case.compile(&mut session, &limited, input);
                equivalent(&warm, &case.fresh(&limited, input));
                if quota < tokens {
                    assert_eq!(warm.status, ResultCategory::ResourceLimit);
                    assert_eq!(warm.phase, Phase::Parse);
                    assert_eq!(warm.verification_runs, 0);
                } else {
                    assert_eq!(parses(&warm), (0, 40));
                    if node_budget {
                        // N bounds Parse's project token charge. Expand uses
                        // the same max_nodes value for its own work and may
                        // still reject; full acceptance must match fresh.
                        assert_ne!(warm.phase, Phase::Parse);
                    } else {
                        success(&warm);
                    }
                }
                bounded(&session, 8);
            }
        }
        // A preceding edit increases actual token consumption. A retained last
        // module cannot hide the now-insufficient remaining quota.
        let mut session = CompilationSession::with_capacity(8);
        success(&case.compile(&mut session, &base, input));
        let root = case.project.modules[0].text.clone();
        case.put(0, format!("// one more token before all modules\n{root}"));
        let mut limited = base.clone();
        limited.limits.max_tokens = tokens;
        let warm = case.compile(&mut session, &limited, input);
        equivalent(&warm, &case.fresh(&limited, input));
        assert_eq!(warm.status, ResultCategory::ResourceLimit);
        bounded(&session, 8);
    }
}

#[test]
fn retained_syntax_keeps_file_module_path_and_rule_identity() {
    let case = Case::new(40);
    let base = CompileOptions::default();
    for axis in 0..6 {
        let mut session = CompilationSession::with_capacity(8);
        success(&session.compile_project(&case.project, &base));
        let mut changed = case.project.clone();
        let mut options = base.clone();
        match axis {
            0 => changed.modules[1].file_id = FileId(1001),
            1 => {
                changed.modules[1].module_id = ModuleId(1001);
                changed.paths.insert(vec!["m1".into()], ModuleId(1001));
            }
            2 => changed.modules[1].path.push_str(".renamed"),
            3 => changed.modules[1].text = leaf(101),
            4 => options.build_inputs = Fingerprint::of(b"syntax-context-changed"),
            _ => options.limits.max_call_depth += 1,
        }
        let warm = session.compile_project(&changed, &options);
        success(&warm);
        equivalent(
            &warm,
            &CompilationSession::default().compile_project(&changed, &options),
        );
        assert_eq!(parses(&warm), if axis < 4 { (1, 39) } else { (40, 0) });
        bounded(&session, 8);
    }
}

#[test]
fn failed_edits_and_observer_cancellation_match_fresh_and_recover() {
    let mut case = Case::new(40);
    let options = CompileOptions::default();
    for input in [Input::Project, Input::Filesystem] {
        let mut session = CompilationSession::with_capacity(8);
        case.put(1, leaf(1));
        success(&case.compile(&mut session, &options, input));
        for bad in ["pub fn helper( ->", "pub fn value()->i32{return true;}"] {
            case.put(1, bad.into());
            let rejected = case.compile(&mut session, &options, input);
            assert_ne!(rejected.status, ResultCategory::Success);
            equivalent(&rejected, &case.fresh(&options, input));
            bounded(&session, 8);
            case.put(1, leaf(1));
            let restored = case.compile(&mut session, &options, input);
            success(&restored);
            equivalent(&restored, &case.fresh(&options, input));
        }
    }
    for phase in [
        Phase::Parse,
        Phase::Expand,
        Phase::Ct,
        Phase::Types,
        Phase::Analyze,
        Phase::Verify,
    ] {
        let mut session = CompilationSession::with_capacity(8);
        case.put(1, leaf(1));
        success(&session.compile_project(&case.project, &options));
        case.put(1, leaf(101));
        let cancelled = |session: &mut CompilationSession| {
            let current = CompileOptions::default();
            let mut seen = 0;
            let mut observed = Vec::new();
            let report =
                session.compile_project_with_observer(&case.project, &current, &mut |event| {
                    observed.push(event.clone());
                    if event.phase == phase {
                        seen += 1;
                        if seen == if phase == Phase::Parse { 17 } else { 1 } {
                            current.cancellation.cancel();
                        }
                    }
                });
            assert_eq!(observed, report.query_events);
            report
        };
        let stopped = cancelled(&mut session);
        assert_eq!(stopped.status, ResultCategory::Cancelled);
        equivalent(&stopped, &cancelled(&mut CompilationSession::default()));
        bounded(&session, 8);
        let recovered = session.compile_project(&case.project, &options);
        success(&recovered);
        equivalent(&recovered, &case.fresh(&options, Input::Project));
    }
}

#[test]
fn current_artifacts_source_cache_and_filesystem_resolution_are_preserved() {
    for input in [Input::Project, Input::Filesystem] {
        let mut case = Case::new(40);
        let base = CompileOptions::default();
        for source_cache in [false, true] {
            let mut session = CompilationSession::with_capacity(8);
            success(&case.compile(&mut session, &base, input));
            case.put(1, leaf(if source_cache { 102 } else { 101 }));
            let mut options = base.clone();
            if source_cache {
                options.cache_dir = Some(case.dir.join("sources"));
            } else {
                options.emit_dir = Some(case.dir.join("artifacts"));
            }
            let warm = case.compile(&mut session, &options, input);
            success(&warm);
            assert_eq!(parses(&warm), (1, 39));
            let artifacts = warm
                .artifacts
                .iter()
                .map(|(name, path)| (name.clone(), fs::read(path).unwrap()))
                .collect::<BTreeMap<_, _>>();
            let fresh = case.fresh(&options, input);
            equivalent(&warm, &fresh);
            for (name, path) in fresh.artifacts {
                assert_eq!(artifacts[&name], fs::read(path).unwrap(), "{name}");
            }
            bounded(&session, 8);
        }
    }
    let case = Case::new(40);
    let options = CompileOptions::default();
    let mut session = CompilationSession::with_capacity(8);
    success(&case.compile(&mut session, &options, Input::Filesystem));
    fs::create_dir(case.dir.join("m7")).unwrap();
    fs::write(case.dir.join("m7/mod.zil"), leaf(7)).unwrap();
    let ambiguous = case.compile(&mut session, &options, Input::Filesystem);
    equivalent(&ambiguous, &case.fresh(&options, Input::Filesystem));
    assert_eq!(ambiguous.diagnostics[0].code, "E-MODULE-AMBIGUOUS");
    fs::remove_file(case.dir.join("m7/mod.zil")).unwrap();
    fs::remove_file(case.dir.join("m7.zil")).unwrap();
    let missing = case.compile(&mut session, &options, Input::Filesystem);
    equivalent(&missing, &case.fresh(&options, Input::Filesystem));
    assert_eq!(missing.diagnostics[0].code, "E-MODULE-NOT-FOUND");
    bounded(&session, 8);
}

#[test]
#[cfg(feature = "llvm-api")]
fn retained_syntax_builds_full_changed_native_output_at_o0_and_o2() {
    use zil_driver::{NativeBackend, Optimization, Stage};
    for input in [Input::Project, Input::Filesystem] {
        for optimization in [Optimization::O0, Optimization::O2] {
            let mut case = Case::new(40);
            let options = CompileOptions {
                backend: NativeBackend::LlvmApi,
                stop_after: Stage::Build,
                optimization,
                output: Some(case.dir.join("program")),
                object_cache: Some(case.dir.join("objects")),
                ..CompileOptions::default()
            };
            let mut session = CompilationSession::with_capacity(8);
            for (value, expected, expected_parses) in [(1, 780_u32, 40), (1001, 1780, 1)] {
                case.put(1, leaf(value));
                let report = case.compile(&mut session, &options, input);
                success(&report);
                assert_eq!(parses(&report), (expected_parses, 40 - expected_parses));
                let output = std::process::Command::new(case.dir.join("program"))
                    .output()
                    .unwrap();
                assert_eq!(output.status.code(), Some(0));
                assert_eq!(output.stdout, expected.to_le_bytes());
                assert!(output.stderr.is_empty());
                bounded(&session, 8);
            }
            let fresh = case.fresh(&options, input);
            success(&fresh);
            let output = std::process::Command::new(case.dir.join("program"))
                .output()
                .unwrap();
            assert_eq!(output.status.code(), Some(0));
            assert_eq!(output.stdout, 1780_u32.to_le_bytes());
            assert!(output.stderr.is_empty());
        }
    }
}
