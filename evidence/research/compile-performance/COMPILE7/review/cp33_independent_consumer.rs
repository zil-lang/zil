//! Non-author public API controls for CP33, no performance scoring.
use std::{fs, path::PathBuf};
use zil_base::{FileId, Limits, ModuleId, Phase, ResultCategory, SourceUnit};
use zil_driver::{CompilationSession, CompileOptions, CompileReport, QueryDisposition, Stage};

fn equivalent(a: &CompileReport, b: &CompileReport) {
    assert_eq!(a.status, b.status, "{}\n{}", a.to_json(), b.to_json());
    assert_eq!(a.phase, b.phase);
    assert_eq!(a.diagnostics, b.diagnostics);
    assert_eq!(a.source_files, b.source_files);
    assert_eq!(a.input_fingerprint, b.input_fingerprint);
    assert_eq!(a.scalar_interfaces, b.scalar_interfaces);
    assert_eq!(a.verification_runs, b.verification_runs);
}
fn parse_hits(r: &CompileReport) -> (usize, usize) {
    let count = |kind| r.query_events.iter().filter(|e| e.phase == Phase::Parse && e.disposition == kind).count();
    (count(QueryDisposition::Executed), count(QueryDisposition::Reused))
}
fn source(text: &str) -> SourceUnit {
    SourceUnit::new(FileId(10), ModuleId(20), "independent-receipt.zil", text)
}

#[test]
fn parse_success_receipt_never_neutralizes_later_phase_node_identity() {
    let candidates = [
        "fn f(x:i32)->i32 { let a=x+1; let b=a+2; let c=b*3; return c+4; }",
        "fn f(x:i32)->i32 { var y=x; while y<10 { y=y+1; } return y; }",
        "fn f(x:i32)->i32 { if x==1 { return x+2; } else { return x+3; } }",
    ];
    let options = CompileOptions { stop_after: Stage::Verify, ..CompileOptions::default() };
    let mut found = false;
    for text in candidates {
        let src = source(text);
        let parsed = zil_syntax::parse(&src);
        assert!(parsed.is_ok());
        let mut session = CompilationSession::with_capacity(512);
        let high = session.compile_source(&src, &options);
        assert_eq!(high.status, ResultCategory::Success, "{}", high.to_json());
        assert_eq!(high.verification_runs, 1);
        for n in parsed.usage.nodes_required..=parsed.usage.nodes_required + 128 {
            let mut low = options.clone(); low.limits.max_nodes = n;
            assert!(parsed.fits_limits(&low.limits));
            let fresh = CompilationSession::default().compile_source(&src, &low);
            if fresh.status != ResultCategory::ResourceLimit || fresh.phase == Phase::Parse { continue; }
            let warm = session.compile_source(&src, &low);
            equivalent(&warm, &fresh);
            assert_eq!(parse_hits(&warm), (0, 1));
            assert!(!warm.query_events.iter().any(|e| e.phase != Phase::Parse && e.disposition == QueryDisposition::Reused));
            assert_eq!(session.active_query_count(), 0);
            let restored = session.compile_source(&src, &options);
            equivalent(&restored, &CompilationSession::default().compile_source(&src, &options));
            assert_eq!(restored.verification_runs, 1);
            println!("later_phase_budget nodes={n} parse_required={} first_error={:?}", parsed.usage.nodes_required, fresh.diagnostics[0]);
            found = true; break;
        }
        if found { break; }
    }
    assert!(found, "need a real Parse-success/later-node-failure boundary");
}

#[test]
fn simultaneous_token_node_boundaries_preserve_full_parse_errors_and_success_values() {
    let corpus = [
        "",
        "// 注释\r\n/* one /* two */ */\nfn f()->i32=>1+2*3",
        "#[doc=\"one\"]\n#[doc=\"two\"]\n#[doc=\"three\"]\nfn f(){}",
        "#[cfg(false)]\n#[unknown]\nfn removed(){}\n#[cfg(feature)]\n#[unknown]\nfn pending(){}",
    ];
    let base = CompileOptions { stop_after: Stage::Parse, ..CompileOptions::default() };
    let mut checks = 0;
    for text in corpus {
        let src = source(text); let full = zil_syntax::parse(&src); assert!(full.is_ok());
        let mut session = CompilationSession::with_capacity(512);
        assert_eq!(session.compile_source(&src, &base).status, ResultCategory::Success);
        let mut node_values = vec![0, 1, full.usage.nodes_required.saturating_sub(1), full.usage.nodes_required, full.usage.nodes_required + 1];
        node_values.sort_unstable(); node_values.dedup();
        let mut token_values = vec![0, 1, full.tokens.len().saturating_sub(1), full.tokens.len(), full.tokens.len() + 1];
        token_values.sort_unstable(); token_values.dedup();
        for nodes in node_values {
            for tokens in &token_values {
                let mut options = base.clone();
                options.limits = Limits { max_nodes: nodes, max_tokens: *tokens, ..Limits::default() };
                let fresh_parse = zil_syntax::parse_with_limits(&src, &options.limits);
                if full.fits_limits(&options.limits) {
                    assert!(fresh_parse.is_ok());
                    assert_eq!(full.module, fresh_parse.module);
                    assert_eq!(full.tokens, fresh_parse.tokens);
                    assert_eq!(full.deferred_attributes, fresh_parse.deferred_attributes);
                    assert_eq!(full.cfg_removed, fresh_parse.cfg_removed);
                    assert_eq!(full.usage, fresh_parse.usage);
                }
                let warm = session.compile_source(&src, &options);
                equivalent(&warm, &CompilationSession::default().compile_source(&src, &options));
                assert_eq!(session.active_query_count(), 0);
                checks += 1;
            }
        }
    }
    println!("simultaneous_token_node_boundary_comparisons={checks}");
    assert!(checks >= 60);
}

struct Case(PathBuf);
impl Drop for Case {
    fn drop(&mut self) { if !std::thread::panicking() { fs::remove_dir_all(&self.0).unwrap(); } }
}

#[test]
fn independently_parsed_sources_can_feed_current_filesystem_discovery_with_new_remaining_budgets() {
    let case = Case(std::env::temp_dir().join(format!("cp33-independent-discovery-{}", std::process::id())));
    fs::create_dir(&case.0).unwrap();
    let files = [
        ("main", "import a\nimport b\nfn main()->i32=>a.value()+b.value()"),
        ("a", "pub fn value()->i32=>3"),
        ("b", "import c\npub fn value()->i32=>c.value()"),
        ("c", "pub fn value()->i32=>4"),
    ];
    let parse_options = CompileOptions { stop_after: Stage::Parse, ..CompileOptions::default() };
    let mut session = CompilationSession::with_capacity(128);
    for (i, (name, text)) in files.iter().enumerate() {
        let path = case.0.join(format!("{name}.zil")); fs::write(&path, text).unwrap();
        let src = SourceUnit::new(FileId(i as u32), ModuleId(i as u32), fs::canonicalize(&path).unwrap().to_string_lossy(), *text);
        let report = session.compile_source(&src, &parse_options);
        assert_eq!(report.status, ResultCategory::Success, "{}", report.to_json());
        assert_eq!(parse_hits(&report), (1, 0));
    }
    let path = case.0.join("main.zil");
    let verify = CompileOptions { stop_after: Stage::Verify, ..CompileOptions::default() };
    let report = session.compile_path(&path, &verify);
    equivalent(&report, &CompilationSession::default().compile_path(&path, &verify));
    assert_eq!(report.status, ResultCategory::Success, "{}", report.to_json());
    assert_eq!(report.verification_runs, 1);
    assert_eq!(parse_hits(&report), (0, 4));
    // Resolution is still performed now, even for sources warmed separately.
    fs::create_dir(case.0.join("c")).unwrap();
    fs::write(case.0.join("c/mod.zil"), files[3].1).unwrap();
    let ambiguous = session.compile_path(&path, &verify);
    equivalent(&ambiguous, &CompilationSession::default().compile_path(&path, &verify));
    assert!(ambiguous.diagnostics.iter().any(|d| d.code == "E-MODULE-AMBIGUOUS"));
    assert_eq!(ambiguous.verification_runs, 0);
    assert_eq!(session.active_query_count(), 0);
    println!("prewarmed_current_discovery parsed_queries_reused=4; current_ambiguity_rejected");
}
