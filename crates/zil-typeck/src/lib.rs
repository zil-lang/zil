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

use zil_base::{Diagnostic, FileId, ModuleId, SourceUnit};
use zil_ir::body::{FnId, Module, SourceMap};
use zil_syntax::ast;

/// The lowered program: candidate IR plus its source evidence.
#[derive(Debug)]
pub struct Lowered {
    pub module: Module,
    pub source_map: SourceMap,
    /// Entry roots (every non-generic normal function) by source name.
    pub roots: Vec<(String, FnId)>,
}

/// Lowers one parsed module.
pub fn lower_module(module: &ast::Module, limits: Limits) -> Result<Lowered, Vec<Diagnostic>> {
    let mut types = ty::Types::default();
    let mut diags = Vec::new();
    let items = items::collect(module, &mut types, &mut diags);
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
    let mut mono = build::Mono::new(&items, &mut types, &thir, &mut diags, limits);
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
    drop(mono);
    if !diags.is_empty() {
        return Err(diags);
    }
    Ok(Lowered {
        module,
        source_map: SourceMap::default(),
        roots,
    })
}

/// Parses and lowers a single source text (tests and tools).
pub fn lower_source(path: &str, text: &str) -> Result<Lowered, Vec<Diagnostic>> {
    let unit = SourceUnit {
        file_id: FileId(0),
        module_id: ModuleId(0),
        path: path.to_string(),
        text: text.to_string(),
    };
    let parsed = zil_syntax::parse(&unit).into_result()?;
    lower_module(&parsed, Limits::default())
}
