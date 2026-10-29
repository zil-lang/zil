#![forbid(unsafe_code)]
//! Editor requests use the same driver, entry contracts, target and resource
//! limits as command-line checking. The editor has no second semantic checker.

use std::path::Path;
use zil_base::{Diagnostic, SourceUnit};
use zil_driver::{CompilationSession, CompileOptions, CompileReport, QueryEvent, Stage};

#[derive(Default)]
pub struct EditorSession {
    compiler: CompilationSession,
}
impl EditorSession {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn clear(&mut self) {
        self.compiler.clear();
    }
    pub fn analyze(&mut self, source: &SourceUnit, options: &CompileOptions) -> CompileReport {
        self.analyze_with_observer(source, options, &mut |_| {})
    }
    pub fn analyze_with_observer(
        &mut self,
        source: &SourceUnit,
        options: &CompileOptions,
        observer: &mut dyn FnMut(&QueryEvent),
    ) -> CompileReport {
        let options = CompileOptions {
            stop_after: Stage::Verify,
            output: None,
            ..options.clone()
        };
        self.compiler
            .compile_source_with_observer(source, &options, observer)
    }
    pub fn analyze_file(&mut self, path: &Path, options: &CompileOptions) -> CompileReport {
        let options = CompileOptions {
            stop_after: Stage::Verify,
            output: None,
            ..options.clone()
        };
        self.compiler.compile_path(path, &options)
    }
}
pub fn diagnostics(source: &SourceUnit) -> Vec<Diagnostic> {
    EditorSession::default()
        .analyze(source, &CompileOptions::default())
        .diagnostics
}
pub fn file_diagnostics(path: &Path) -> Vec<Diagnostic> {
    EditorSession::default()
        .analyze_file(path, &CompileOptions::default())
        .diagnostics
}

#[cfg(test)]
mod tests {
    use super::*;
    use zil_base::{FileId, ModuleId, ResultCategory};
    use zil_driver::{CancellationToken, QueryDisposition};
    #[test]
    fn editor_and_command_line_driver_return_identical_semantic_diagnostics() {
        let cases = [
            "fn main() -> i32 { return 0 }",
            "fn main() -> bool => true",
            "fn main() -> i32 { return true }",
            "fn main() -> i32 { return missing }",
            "fn main() -> i32 { return 0 }\nfn main() -> i32 { return 1 }",
            "let x = 1",
            "fn check() -> i64 { return #(1 / 0) }",
        ];
        let mut editor = EditorSession::default();
        for text in cases {
            let source = SourceUnit::new(FileId(2), ModuleId(3), "editor.zil", text);
            let options = CompileOptions::default();
            let cli = zil_driver::compile_source(&source, &options);
            let ide = editor.analyze(&source, &options);
            assert_eq!(ide.status, cli.status);
            assert_eq!(ide.phase, cli.phase);
            assert_eq!(ide.diagnostics, cli.diagnostics);
        }
    }
    #[test]
    fn editor_session_reuses_raw_queries_and_rechecks_verification() {
        let source = SourceUnit::new(FileId(0), ModuleId(0), "editor.zil", "fn main() {}");
        let mut editor = EditorSession::new();
        let options = CompileOptions::default();
        assert_eq!(
            editor.analyze(&source, &options).status,
            ResultCategory::Success
        );
        let warm = editor.analyze(&source, &options);
        assert!(
            warm.query_events
                .iter()
                .any(|event| event.disposition == QueryDisposition::Reused)
        );
        assert_eq!(warm.verification_runs, 1);
    }
    #[test]
    fn editor_preserves_target_limits_and_cancellation() {
        let source = SourceUnit::new(FileId(0), ModuleId(0), "editor.zil", "fn main() {}");
        let mut editor = EditorSession::default();
        let mut options = CompileOptions::default();
        options.limits.max_tokens = 1;
        let cli = zil_driver::compile_source(&source, &options);
        let ide = editor.analyze(&source, &options);
        assert_eq!(ide.status, ResultCategory::ResourceLimit);
        assert_eq!(ide.diagnostics, cli.diagnostics);
        let options = CompileOptions {
            cancellation: CancellationToken::default(),
            ..CompileOptions::default()
        };
        options.cancellation.cancel();
        assert_eq!(
            editor.analyze(&source, &options).status,
            ResultCategory::Cancelled
        );
    }

    #[test]
    fn editor202_full_store_types_match_driver_and_invalidate_changed_bounds() {
        let cases = [
            include_str!("../../zil-driver/tests/impl202_fixtures/01_original_push.zil"),
            include_str!("../../zil-driver/tests/impl202_fixtures/02_original_factory.zil"),
        ];
        let mut editor = EditorSession::new();
        let options = CompileOptions {
            stop_after: Stage::Verify,
            ..CompileOptions::default()
        };
        for original in cases {
            // Reuse the same file/module/path across edits: stale template or
            // concrete-type bindings must not survive the source fingerprint.
            let source = SourceUnit::new(FileId(0), ModuleId(0), "store-template202.zil", original);
            let cli = zil_driver::compile_source(&source, &options);
            assert_eq!(cli.status, ResultCategory::Success, "{}", cli.to_json());
            assert_eq!(cli.verification_runs, 1);
            let cold = editor.analyze(&source, &options);
            assert_eq!(cold.status, cli.status);
            assert_eq!(cold.phase, cli.phase);
            assert_eq!(cold.diagnostics, cli.diagnostics);
            let warm = editor.analyze(&source, &options);
            assert_eq!(warm.status, ResultCategory::Success);
            assert_eq!(warm.verification_runs, 1);
            assert!(
                warm.query_events
                    .iter()
                    .any(|event| event.disposition == QueryDisposition::Reused)
            );
            let changed = original.replacen("fn push<T: StoreElement>", "fn push<T: Copy>", 1);
            assert_ne!(original, changed);
            let edited = SourceUnit::new(FileId(0), ModuleId(0), "store-template202.zil", changed);
            let cli = zil_driver::compile_source(&edited, &options);
            let ide = editor.analyze(&edited, &options);
            assert_ne!(cli.status, ResultCategory::Success);
            assert_eq!(cli.phase, zil_base::Phase::Expand);
            assert_eq!(ide.status, cli.status);
            assert_eq!(ide.phase, cli.phase);
            assert_eq!(ide.diagnostics, cli.diagnostics);
            assert_eq!(ide.verification_runs, 0);
            let restored = editor.analyze(&source, &options);
            assert_eq!(
                restored.status,
                ResultCategory::Success,
                "{}",
                restored.to_json()
            );
            assert_eq!(restored.verification_runs, 1);
        }
    }

    #[test]
    fn editor203_state_declaration_edits_invalidate_even_when_actual_closure_types_stay_equal() {
        let cases = [
            include_str!(
                "../../zil-driver/tests/impl203_fixtures/01_same_factory_distinct_modes.zil"
            ),
            include_str!("../../zil-driver/tests/impl203_fixtures/04_mode_cleanup_markers.zil"),
        ];
        let options = CompileOptions {
            stop_after: Stage::Verify,
            ..CompileOptions::default()
        };
        let mut editor = EditorSession::new();
        for original in cases {
            let source = SourceUnit::new(FileId(0), ModuleId(0), "state203-edit.zil", original);
            let cli = zil_driver::compile_source(&source, &options);
            assert_eq!(cli.status, ResultCategory::Success, "{}", cli.to_json());
            let cold = editor.analyze(&source, &options);
            assert_eq!(cold.diagnostics, cli.diagnostics);
            assert_eq!(cold.status, cli.status);
            assert_eq!(cold.phase, cli.phase);
            assert_eq!(cold.verification_runs, 1);
            let warm = editor.analyze(&source, &options);
            assert_eq!(warm.status, ResultCategory::Success, "{}", warm.to_json());
            assert_eq!(warm.verification_runs, 1);
            assert!(
                warm.query_events
                    .iter()
                    .any(|e| e.disposition == QueryDisposition::Reused)
            );

            // The factory and both actual ClosureIds are unchanged. Only the
            // current generic declaration weakens F to a consuming capability;
            // its second call must be rejected before independent verification.
            let head = "fn run<T: Copy, F: Fn() -> i32";
            assert_eq!(original.matches(head).count(), 1);
            let changed = original.replacen(head, "fn run<T: Copy, F: FnOnce() -> i32", 1);
            let changed = SourceUnit::new(FileId(0), ModuleId(0), "state203-edit.zil", changed);
            let cli = zil_driver::compile_source(&changed, &options);
            assert_eq!(cli.status, ResultCategory::SafeReject, "{}", cli.to_json());
            let ide = editor.analyze(&changed, &options);
            assert_eq!(ide.status, cli.status);
            assert_eq!(ide.phase, cli.phase);
            assert_eq!(ide.diagnostics, cli.diagnostics);
            assert_eq!(ide.verification_runs, 0);
            let restored = editor.analyze(&source, &options);
            assert_eq!(
                restored.status,
                ResultCategory::Success,
                "{}",
                restored.to_json()
            );
            assert_eq!(restored.verification_runs, 1);
        }
    }
}
