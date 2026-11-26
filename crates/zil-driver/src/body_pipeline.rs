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

pub(crate) struct InterpCtEvaluator;

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

    // Independent universal CT original-definition verification (checks unused
    // CT declarations and every branch directly against parsed AST).
    let by_logical: std::collections::HashMap<&str, ModuleId> = parsed
        .iter()
        .enumerate()
        .map(|(i, (path, _))| (*path, ModuleId(i as u32)))
        .collect();
    let ct_import_owned: Vec<Vec<(String, zil_check::ct_original::CtOriginalImportTarget)>> =
        parsed
            .iter()
            .map(|(_, ast)| {
                ast.items
                    .iter()
                    .filter_map(|item| {
                        let zil_syntax::ast::ItemKind::Import(imp) = &item.kind else {
                            return None;
                        };
                        let alias = imp
                            .alias
                            .clone()
                            .or_else(|| imp.path.last().cloned())
                            .unwrap_or_default();
                        let target =
                            if imp.path.len() == 2 && imp.path[0] == "std" && imp.path[1] == "meta"
                            {
                                zil_check::ct_original::CtOriginalImportTarget::Meta
                            } else if imp.path.len() == 2
                                && imp.path[0] == "std"
                                && imp.path[1] == "syntax"
                            {
                                zil_check::ct_original::CtOriginalImportTarget::Syntax
                            } else {
                                let logical = imp.path.join(".");
                                let mid = *by_logical.get(logical.as_str())?;
                                zil_check::ct_original::CtOriginalImportTarget::Module(mid)
                            };
                        Some((alias, target))
                    })
                    .collect()
            })
            .collect();
    let ct_import_vecs: Vec<Vec<zil_check::ct_original::CtOriginalImport<'_>>> = ct_import_owned
        .iter()
        .map(|edges| {
            edges
                .iter()
                .map(|(alias, target)| zil_check::ct_original::CtOriginalImport {
                    alias: alias.as_str(),
                    target: *target,
                })
                .collect()
        })
        .collect();
    let ct_mods: Vec<zil_check::ct_original::CtOriginalModule<'_>> = ast_refs
        .iter()
        .zip(ct_import_vecs.iter())
        .map(|(syntax, imps)| zil_check::ct_original::CtOriginalModule {
            syntax,
            imports: imps.as_slice(),
        })
        .collect();
    let mut ct_meter = zil_check::ct_original::CtVerificationMeter::new(1_000_000);
    zil_check::ct_original::verify_ct_original_definitions(
        &ct_mods,
        &[],
        &zil_base::TargetProfile::host_llvm(),
        &zil_base::Limits::default(),
        &mut ct_meter,
    )
    .map_err(|diags| BodyCompileError {
        phase: Phase::Ct,
        message: format!(
            "CT original check failed with {} diagnostic(s)",
            diags.len()
        ),
        diagnostics: diags,
    })?;

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
            backend: zil_backend_llvm::NativeBackend::Clang,
            target: zil_base::TargetProfile::host_llvm(),
            windows: None,
            clang: None,
            extra_c_sources,
            emit_c_main: true,
            entry: None,
            codegen_units: 1,
            codegen_jobs: 1,
        },
    )
    .map(|o| o.ir)
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
            let bin = dir.join(if cfg!(windows) {
                format!("app_{label}.exe")
            } else {
                format!("app_{label}")
            });
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

    #[test]
    fn unused_invalid_ct_declaration_is_rejected_by_ct_original() {
        let src = r#"
fn never_called(t: Type) -> i32 {
    return unknown_ct_symbol
}

fn main() -> i32 {
    return 0
}
"#;
        let err = verify_body_program(&[("ct_unused", src)])
            .expect_err("unused invalid ct fn must be rejected by ct_original");
        assert_eq!(err.phase, Phase::Ct);
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn native_o0_and_o2_execute_store_lifo_drop_bytes_mutex_and_thread_scope() {
        let dir =
            std::env::temp_dir().join(format!("zil-body-store-native-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("create temp dir");

        // C observer recording exact drop order as base-10 digits (e.g., drop 2 then 1 -> 21).
        let c_path = dir.join("lifo_observer.c");
        std::fs::write(
            &c_path,
            r#"
#include <stdint.h>
static int32_t g_seq = 0;
void lifo_drop(int32_t tag) { g_seq = g_seq * 10 + tag; }
int32_t lifo_seq(void) { return g_seq; }
"#,
        )
        .expect("write lifo_observer.c");

        let zil_src = r#"
extern "C" fn lifo_drop(tag: i32);
extern "C" fn lifo_seq() -> i32;

resource Item {
    tag: i32;
}

impl Drop for Item {
    fn drop(self: Self) {
        unsafe { lifo_drop(self.tag) }
    }
}

fn fill_store_and_drop() -> i32 {
    let s = store_new();
    store_reserve(s, 4);
    store_push(s, Item { tag: 1 });
    store_push(s, Item { tag: 2 });
    store_push(s, Item { tag: 3 });
    let n = i32(store_len(s));
    store_pop(s);
    return n;
}

fn byte_buf_check() -> i32 {
    let b = bytes_new();
    bytes_reserve(b, 8);
    bytes_push(b, u8(10));
    bytes_push(b, u8(20));
    bytes_push(b, u8(30));
    bytes_truncate(b, 2);
    let view = bytes_view(b);
    return i32(view[0]) + i32(view[1]);
}

fn worker(m: Mutex<i32>) -> i32 {
    var guard = mutex_lock(m);
    let p = mutex_value(&mut guard);
    *p = *p + 4;
    return *p;
}

fn main() -> i32 {
    let n = fill_store_and_drop();
    var seq = 0;
    unsafe {
        seq = lifo_seq();
    }
    let bsum = byte_buf_check();
    var msum = 0;
    match mutex_create(6) {
        .ok(m) => {
            let (ok, r1, r2) = thread_scope(worker, m, () -> i32 => 2);
            if ok {
                msum = r1 + r2;
            }
        },
        .err(_) => {
            msum = 0;
        }
    }
    // n=3, seq=312 (Item 3 popped & dropped first, then Store drops remaining 1 then 2 in index order),
    // bsum=30 (10+20), msum=12 (10+2). Return (seq - 300) + bsum + msum + n = 12 + 30 + 12 + 3 = 57.
    return (seq - 300) + bsum + msum + n;
}
"#;

        for (label, opt) in [("O0", Optimization::O0), ("O2", Optimization::O2)] {
            let exe = dir.join(format!("store_native_{label}"));
            build_body_source_native(
                "store_native.zil",
                zil_src,
                exe.clone(),
                opt,
                vec![c_path.clone()],
            )
            .unwrap_or_else(|e| panic!("build failed at {label}: {e:?}"));

            let status = Command::new(&exe)
                .status()
                .unwrap_or_else(|e| panic!("failed to run {label} executable: {e}"));
            assert_eq!(
                status.code(),
                Some(57),
                "expected exit code 57 (seq=312, bsum=30, msum=12, n=3) at {label}"
            );
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
