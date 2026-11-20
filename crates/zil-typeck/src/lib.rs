//! typeck: Source AST to a monomorphic body IR module.
//!
//! Contract: `spec/source-lowering.md`. The layers are resolve/collect
//! ([`items`]), infer ([`infer`], producing [`thir`]) and mono/build
//! ([`build`]). typeck decides names, types and structure only; it grants no
//! safety admission. Every function it produces is checked by the semantic
//! kernel through `zil-solve`/`zil-check`.
#![forbid(unsafe_code)]
#![deny(clippy::wildcard_enum_match_arm)]

mod build;
mod diag;
mod drop_elab;
mod infer;
mod items;
mod literal;
mod thir;
mod ty;

pub use build::Limits;

use zil_base::{Diagnostic, FileId, ModuleId, SourceUnit, Span};
use zil_ir::body::{FloatTy, FnId, IntTy, Module, SourceMap};
use zil_syntax::ast;

/// A compile-time value returned by a host [`ConstEvaluator`].
/// Types are carried by the receiving expression in the caller's module,
/// so `CtValue` never mixes `TyId`s across modules.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CtValue {
    Unit,
    Bool(bool),
    Int {
        ty: IntTy,
        bits: u128,
    },
    Float {
        ty: FloatTy,
        bits: u64,
    },
    Aggregate {
        variant: Option<u32>,
        fields: Vec<CtValue>,
    },
    ErrorUnion {
        ok: bool,
        payload: Box<CtValue>,
    },
}

/// Host-injected compile-time evaluator (`spec/source-lowering.md` §1, A2).
/// `zil-typeck` lowers `#(expr)` to a self-contained closed body IR module
/// and delegates verification + execution (`zil-solve` -> `zil-check` ->
/// `zil-interp`) to the host.
pub trait ConstEvaluator {
    #[allow(clippy::result_large_err)]
    fn eval(&mut self, module: Module, entry: FnId, span: Span) -> Result<CtValue, Diagnostic>;
}

/// The lowered program: candidate IR plus its source evidence.
#[derive(Debug)]
pub struct Lowered {
    pub module: Module,
    pub source_map: SourceMap,
    /// Entry roots (every non-generic normal function) by source name.
    pub roots: Vec<(String, FnId)>,
}

/// One parsed source module in a multi-module program.
pub struct ModuleInput<'a> {
    /// Canonical dot-separated module path (e.g. `"main"`, `"io"`, `"std.store"`).
    /// Empty string for a standalone single-file module.
    pub path: String,
    pub ast: &'a ast::Module,
}

/// Lowers one parsed module without a compile-time evaluator.
pub fn lower_module(module: &ast::Module, limits: Limits) -> Result<Lowered, Vec<Diagnostic>> {
    lower_module_with_ct(module, limits, None)
}

/// Lowers one parsed module, delegating `#(...)` expressions to `ct`.
pub fn lower_module_with_ct(
    module: &ast::Module,
    limits: Limits,
    ct: Option<&mut dyn ConstEvaluator>,
) -> Result<Lowered, Vec<Diagnostic>> {
    lower_program(
        &[ModuleInput {
            path: String::new(),
            ast: module,
        }],
        limits,
        ct,
    )
}

/// Lowers a multi-module program into a single monomorphic body IR module.
pub fn lower_program(
    modules: &[ModuleInput<'_>],
    limits: Limits,
    ct: Option<&mut dyn ConstEvaluator>,
) -> Result<Lowered, Vec<Diagnostic>> {
    let mut types = ty::Types::default();
    let mut diags = Vec::new();
    let items = items::collect_program(modules, &mut types, &mut diags);
    if !diags.is_empty() {
        return Err(diags);
    }
    let mut thir = Vec::new();
    for i in 0..items.fns.len() {
        thir.push(infer::infer_fn(
            &items,
            &mut types,
            &mut diags,
            thir::FnRef(i as u32),
        ));
    }
    if !diags.is_empty() {
        return Err(diags);
    }
    let mut mono = build::Mono::new(&items, &mut types, &thir, &mut diags, limits, ct);
    mono.lower_foreigns();
    let mut roots = Vec::new();
    for (i, f) in items.fns.iter().enumerate() {
        if f.generics.is_empty()
            && let Some(id) = mono.instance(thir::FnRef(i as u32), Vec::new(), f.ast.span)
            && f.role == items::FnRoleSpec::Normal
        {
            roots.push((f.name.clone(), id));
        }
    }
    mono.run();
    let module = std::mem::take(&mut mono.module);
    let source_map = std::mem::take(&mut mono.source_map);
    drop(mono);
    if !diags.is_empty() {
        return Err(diags);
    }
    Ok(Lowered {
        module,
        source_map,
        roots,
    })
}

/// Parses and lowers a single source text (tests and tools).
pub fn lower_source(path: &str, text: &str) -> Result<Lowered, Vec<Diagnostic>> {
    lower_source_with_ct(path, text, None)
}

/// Parses and lowers a single source text with a compile-time evaluator.
pub fn lower_source_with_ct(
    path: &str,
    text: &str,
    ct: Option<&mut dyn ConstEvaluator>,
) -> Result<Lowered, Vec<Diagnostic>> {
    let unit = SourceUnit {
        file_id: FileId(0),
        module_id: ModuleId(0),
        path: path.to_string(),
        text: text.to_string(),
    };
    let parsed = zil_syntax::parse(&unit).into_result()?;
    lower_module_with_ct(&parsed, Limits::default(), ct)
}

/// Parses and lowers multiple named modules `(module_path, source_text)`.
pub fn lower_sources(
    sources: &[(&str, &str)],
    ct: Option<&mut dyn ConstEvaluator>,
) -> Result<Lowered, Vec<Diagnostic>> {
    let mut parsed = Vec::with_capacity(sources.len());
    for (i, (path, text)) in sources.iter().enumerate() {
        let unit = SourceUnit {
            file_id: FileId(i as u32),
            module_id: ModuleId(i as u32),
            path: format!("{path}.zil"),
            text: (*text).to_string(),
        };
        parsed.push((*path, zil_syntax::parse(&unit).into_result()?));
    }
    let inputs: Vec<ModuleInput<'_>> = parsed
        .iter()
        .map(|(path, ast)| ModuleInput {
            path: (*path).to_string(),
            ast,
        })
        .collect();
    lower_program(&inputs, Limits::default(), ct)
}
