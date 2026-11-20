//! End-to-end Body IR compilation pipeline:
//! Source AST -> `zil-typeck` (with `zil-interp` CT) -> `zil-solve` ->
//! `zil-check` (fidelity + certificate) -> `zil-backend-llvm` (LLVM 23 O0/O2).

use std::path::PathBuf;
use zil_backend_llvm::{BodyBuildOptions, Optimization, build_verified_body};
use zil_base::{Diagnostic, FileId, ModuleId, Phase, SourceUnit, Span};
use zil_check::{CheckBudget, VerifiedModule, check_fidelity, check_module};
use zil_interp::{EvalBudget, Value, evaluate};
use zil_ir::body::{FnId, Module};
use zil_solve::{Budget as SolveBudget, solve_module};
use zil_typeck::{ConstEvaluator, CtValue, Limits as TypeckLimits, ModuleInput, lower_program};

#[derive(Clone, Debug)]
pub struct BodyCompileError {
    pub phase: Phase,
    pub message: String,
    pub diagnostics: Vec<Diagnostic>,
}

struct InterpCtEvaluator;

impl ConstEvaluator for InterpCtEvaluator {
    fn eval(&mut self, module: Module, entry: FnId, span: Span) -> Result<CtValue, Diagnostic> {
        let cert = solve_module(&module, SolveBudget { steps: 1_000_000 }).map_err(|e| {
            Diagnostic::error(
                Phase::Ct,
                "E-CT-VERIFY",
                Some(span),
                format!("CT solver rejected: {e:?}"),
            )
        })?;
        let verified =
            check_module(module, &cert, CheckBudget { steps: 1_000_000 }).map_err(|e| {
                Diagnostic::error(
                    Phase::Ct,
                    "E-CT-VERIFY",
                    Some(span),
                    format!("CT checker rejected: {e:?}"),
                )
            })?;
        let val = evaluate(
            &verified,
            entry,
            Vec::new(),
            EvalBudget {
                steps: 100_000,
                call_depth: 64,
            },
        )
        .map_err(|e| {
            Diagnostic::error(
                Phase::Ct,
                "E-CT-EVAL",
                Some(span),
                format!("CT evaluation failed: {e:?}"),
            )
        })?;
        value_to_ct(val, span)
    }
}

fn value_to_ct(val: Value, span: Span) -> Result<CtValue, Diagnostic> {
    match val {
        Value::Unit => Ok(CtValue::Unit),
        Value::Bool(b) => Ok(CtValue::Bool(b)),
        Value::Int { ty, bits } => Ok(CtValue::Int { ty, bits }),
        Value::Float { ty, bits } => Ok(CtValue::Float { ty, bits }),
        Value::Aggregate {
            variant, fields, ..
        } => {
            let mut out = Vec::with_capacity(fields.len());
            for f in fields {
                out.push(value_to_ct(f, span)?);
            }
            Ok(CtValue::Aggregate {
                variant,
                fields: out,
            })
        }
        Value::ErrorUnion { ok, payload, .. } => Ok(CtValue::ErrorUnion {
            ok,
            payload: Box::new(value_to_ct(*payload, span)?),
        }),
        Value::FnRef(_) => Err(Diagnostic::error(
            Phase::Ct,
            "E-CT-RESULT",
            Some(span),
            "CT expression evaluated to non-materializable function pointer",
        )),
    }
}

/// Parses, typechecks (with CT evaluation), solves, and independently verifies
/// (both AST-backed fidelity and certificate soundness) a multi-module program.
pub fn verify_body_program(sources: &[(&str, &str)]) -> Result<VerifiedModule, BodyCompileError> {
    let mut parsed = Vec::with_capacity(sources.len());
    for (i, (path, text)) in sources.iter().enumerate() {
        let unit = SourceUnit {
            file_id: FileId(i as u32),
            module_id: ModuleId(i as u32),
            path: format!("{path}.zil"),
            text: (*text).to_string(),
        };
        let ast = zil_syntax::parse(&unit)
            .into_result()
            .map_err(|diags| BodyCompileError {
                phase: Phase::Parse,
                message: format!("parse failed with {} diagnostic(s)", diags.len()),
                diagnostics: diags,
            })?;
        parsed.push((*path, ast));
    }
    let inputs: Vec<ModuleInput<'_>> = parsed
        .iter()
        .map(|(path, ast)| ModuleInput {
            path: (*path).to_string(),
            ast,
        })
        .collect();
    let ast_refs: Vec<&zil_syntax::ast::Module> = parsed.iter().map(|(_, ast)| ast).collect();

    let mut ct = InterpCtEvaluator;
    let lowered =
        lower_program(&inputs, TypeckLimits::default(), Some(&mut ct)).map_err(|diags| {
            BodyCompileError {
                phase: Phase::Types,
                message: format!("typeck failed with {} diagnostic(s)", diags.len()),
                diagnostics: diags,
            }
        })?;

    check_fidelity(&lowered.module, &lowered.source_map, &ast_refs).map_err(|e| {
        BodyCompileError {
            phase: Phase::Verify,
            message: format!("fidelity check failed in {:?}: {}", e.function, e.reason),
            diagnostics: Vec::new(),
        }
    })?;

    let cert = solve_module(&lowered.module, SolveBudget { steps: 1_000_000 }).map_err(|e| {
        BodyCompileError {
            phase: Phase::Analyze,
            message: format!("solver rejected: {e:?}"),
            diagnostics: Vec::new(),
        }
    })?;

    check_module(lowered.module, &cert, CheckBudget { steps: 1_000_000 }).map_err(|e| {
        BodyCompileError {
            phase: Phase::Verify,
            message: format!("checker rejected: {e:?}"),
            diagnostics: Vec::new(),
        }
    })
}

/// Verifies a single-file ZIL program and compiles it to a native executable
/// using LLVM 23 clang at the requested optimization level (`O0` or `O2`).
pub fn build_body_source_native(
    path: &str,
    source: &str,
    output: PathBuf,
    optimization: Optimization,
    extra_c_sources: Vec<PathBuf>,
) -> Result<String, BodyCompileError> {
    let verified = verify_body_program(&[(path, source)])?;
    build_verified_body(
        &verified,
        &BodyBuildOptions {
            output,
            optimization,
            clang: None,
            extra_c_sources,
            emit_c_main: true,
        },
    )
    .map_err(|e| BodyCompileError {
        phase: Phase::Codegen,
        message: e.message,
        diagnostics: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    #[test]
    fn native_o0_and_o2_execute_scalar_closure_and_resource_drop_cleanup() {
        let dir = std::env::temp_dir().join(format!("zil-body-native-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("create temp dir");

        // C observer tracking resource drop count on both normal and early-error exits.
        let c_path = dir.join("observer.c");
        std::fs::write(
            &c_path,
            r#"
#include <stdint.h>
static int32_t g_drops = 0;
void obs_record_drop(int32_t tag) { g_drops += tag; }
int32_t obs_get_drops(void) { return g_drops; }
"#,
        )
        .expect("write observer.c");

        let zil_src = r#"
extern "C" fn obs_record_drop(tag: i32);
extern "C" fn obs_get_drops() -> i32;

resource Guard {
    tag: i32;
}

impl Drop for Guard {
    fn drop(self: Self) {
        unsafe { obs_record_drop(self.tag) }
    }
}

enum ErrKind {
    Fail;
}

fn run_with_early_error(fail: bool) -> ErrKind!i32 {
    let g = Guard { tag: 10 }
    if fail {
        return ErrKind.Fail
    }
    let _ = g.tag
    return 5
}

fn main() -> i32 {
    let f = (x: i32) -> i32 => { return x + 7 }
    let base = f(5)
    var a: i32 = 0
    match run_with_early_error(true) {
        .ok(v) => { a = v }
        .err(_) => { a = 0 }
    }
    var b: i32 = 0
    match run_with_early_error(false) {
        .ok(v) => { b = v }
        .err(_) => { b = 0 }
    }
    var drops: i32 = 0
    unsafe { drops = obs_get_drops() }
    // base=12, a=0, b=5, drops=20 (10 from early-error cleanup + 10 from normal cleanup) => 37
    return base + a + b + drops
}
"#;

        for (label, opt) in [("o0", Optimization::O0), ("o2", Optimization::O2)] {
            let bin = dir.join(format!("app_{label}"));
            build_body_source_native(
                "native_cleanup",
                zil_src,
                bin.clone(),
                opt,
                vec![c_path.clone()],
            )
            .unwrap_or_else(|e| panic!("native build ({label}) failed: {e:?}"));

            let status = Command::new(&bin)
                .status()
                .unwrap_or_else(|e| panic!("run ({label}) failed: {e}"));
            assert_eq!(
                status.code(),
                Some(37),
                "expected exit code 37 (base=12 + b=5 + drops=20) at {label}"
            );
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
