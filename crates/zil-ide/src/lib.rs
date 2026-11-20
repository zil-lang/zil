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
    fn editor_invalidates_cached_parse_when_generic_bounds_change() {
        let original = r#"
fn id<T: Copy>(x: T) -> T { return x; }
fn main() -> i32 { return id(42); }
"#;
        let mut editor = EditorSession::new();
        let options = CompileOptions {
            stop_after: Stage::Verify,
            ..CompileOptions::default()
        };
        let source = SourceUnit::new(FileId(0), ModuleId(0), "lib_store_template.zil", original);
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
        let changed = r#"
resource Tok { code: i32; }
fn id<T: Copy>(x: T) -> T { return x; }
fn main() -> i32 { let t = Tok { code: 1 }; let _ = id(give t); return 0; }
"#;
        let edited = SourceUnit::new(FileId(0), ModuleId(0), "lib_store_template.zil", changed);
        let cli = zil_driver::compile_source(&edited, &options);
        let ide = editor.analyze(&edited, &options);
        assert_ne!(cli.status, ResultCategory::Success);
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
