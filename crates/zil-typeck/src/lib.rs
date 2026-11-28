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
mod incremental;
mod infer;
mod items;
mod literal;
mod thir;
mod ty;

pub use build::Limits;
pub use incremental::{
    IncrementalBaseline, ModuleDepRecord, compute_module_dep_records, compute_reusable_modules,
    module_export_signature_fp,
};

use std::collections::HashSet;
use zil_base::{Diagnostic, FileId, ModuleId, SourceUnit, Span};
use zil_ir::body::{FloatTy, FnId, FunctionCertificate, IntTy, Module, SourceMap};
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
    /// Number of non-generic root functions reused directly from an incremental baseline.
    pub reused_fns: usize,
    /// Total number of non-generic root functions in this program.
    pub total_root_fns: usize,
    /// Pre-remapped untrusted candidate [`FunctionCertificate`]s indexed by `FnId.index()`
    /// for functions reused from an incremental baseline.
    pub reused_certs: Vec<Option<FunctionCertificate>>,
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
    lower_program_incremental(modules, limits, ct, None)
}

/// Lowers a multi-module program, optionally reusing verified function bodies and
/// source maps from `baseline` for modules whose source text and transitive
/// imported export signatures are unchanged.
pub fn lower_program_incremental(
    modules: &[ModuleInput<'_>],
    limits: Limits,
    ct: Option<&mut dyn ConstEvaluator>,
    baseline: Option<IncrementalBaseline<'_>>,
) -> Result<Lowered, Vec<Diagnostic>> {
    let mut types = ty::Types::default();
    let mut diags = Vec::new();
    let items = items::collect_program(modules, &mut types, &mut diags);
    if !diags.is_empty() {
        return Err(diags);
    }

    let is_ct_type_fn = |f: &items::FnDecl<'_>, types: &ty::Types| -> bool {
        matches!(types.data(f.result), ty::TyData::Type)
            || f.params
                .iter()
                .any(|p| matches!(types.data(p.ty), ty::TyData::Type))
    };
    let is_ct_flags: Vec<bool> = items.fns.iter().map(|f| is_ct_type_fn(f, &types)).collect();

    // Fast path: when an incremental baseline is supplied with reusable modules,
    // pre-register all non-generic root functions & ADTs in `Mono` and remap
    // unchanged root functions directly from `prev_module`, skipping `infer_fn`,
    // `Mono::lower_fn`, and `drop_elab` for every reused function!
    if let Some(base) = baseline
        && !base.reusable_modules.is_empty()
    {
        return lower_program_with_baseline(items, types, is_ct_flags, limits, ct, base);
    }

    let mut ct = ct;
    let mut thir: Vec<Option<thir::ThirFn>> = vec![None; items.fns.len()];
    // Pass 1: infer compile-time Type-using functions first so `#(...)`
    // expressions in ordinary functions can evaluate them.
    for (i, _) in items.fns.iter().enumerate() {
        if is_ct_flags[i] {
            thir[i] = infer::infer_fn(
                &items,
                &mut types,
                &mut diags,
                &[],
                None,
                limits,
                thir::FnRef(i as u32),
            );
        }
    }
    // Pass 2: infer all remaining functions with access to completed CT-Type THIR.
    for (i, _) in items.fns.iter().enumerate() {
        if !is_ct_flags[i] {
            let ct_ref: Option<&mut dyn ConstEvaluator> = match &mut ct {
                Some(c) => Some(&mut **c),
                None => None,
            };
            let done = infer::infer_fn(
                &items,
                &mut types,
                &mut diags,
                &thir,
                ct_ref,
                limits,
                thir::FnRef(i as u32),
            );
            thir[i] = done;
        }
    }
    if !diags.is_empty() {
        return Err(diags);
    }
    infer::check_defer_suspend_effects(&items, &types, &thir, &mut diags);
    if !diags.is_empty() {
        return Err(diags);
    }
    let mut mono = build::Mono::new(&items, &mut types, &thir, &mut diags, limits, ct);
    mono.lower_foreigns();
    let mut roots = Vec::new();
    let mut total_root_fns = 0usize;
    for (i, f) in items.fns.iter().enumerate() {
        if f.generics.is_empty()
            && !is_ct_flags[i]
            && let Some(id) = mono.instance(thir::FnRef(i as u32), Vec::new(), f.ast.span)
        {
            total_root_fns += 1;
            if f.role == items::FnRoleSpec::Normal {
                roots.push((f.name.clone(), id));
            }
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
        reused_fns: 0,
        total_root_fns,
        reused_certs: Vec::new(),
    })
}

fn lower_program_with_baseline(
    items: items::Items<'_>,
    mut types: ty::Types,
    is_ct_flags: Vec<bool>,
    limits: Limits,
    mut ct: Option<&mut dyn ConstEvaluator>,
    base: IncrementalBaseline<'_>,
) -> Result<Lowered, Vec<Diagnostic>> {
    let mut diags = Vec::new();
    let prev_idx =
        incremental::PrevModuleIndex::new(base.prev_module, base.prev_source_map, base.prev_cert);

    // Phase A: pre-register all non-generic ADTs, foreigns, and root functions,
    // and reuse lowered bodies for clean modules BEFORE running `infer_fn`.
    let empty_thir: [Option<thir::ThirFn>; 0] = [];
    let mut pre_mono = build::Mono::new(&items, &mut types, &empty_thir, &mut diags, limits, None);
    pre_mono.lower_foreigns();
    let pre = incremental::pre_register_non_generic_items(&mut pre_mono, &is_ct_flags);

    let fn_count = items.fns.len();
    let mut reused = vec![false; fn_count];
    let mut seeded_fn_susp = vec![false; fn_count];
    let mut reused_certs: Vec<Option<FunctionCertificate>> =
        vec![None; pre_mono.module.functions.len()];
    let mut reused_fns = 0usize;
    let mut total_root_fns = 0usize;

    for (i, f) in items.fns.iter().enumerate() {
        let Some(curr_fn_id) = pre.root_fn_ids[i] else {
            continue;
        };
        total_root_fns += 1;
        if base.reusable_modules.contains(&(f.module.0 as usize))
            && let Some(remapped_cert) = incremental::try_reuse_root_fn(
                &prev_idx,
                &f.name,
                curr_fn_id,
                &pre.root_by_name,
                &pre.adt_by_name,
                &pre.foreign_by_sym,
                &mut pre_mono,
            )
        {
            reused[i] = true;
            reused_fns += 1;
            if let Some(slot) = reused_certs.get_mut(curr_fn_id.index()) {
                *slot = remapped_cert;
            }
            let func = &pre_mono.module.functions[curr_fn_id.index()];
            if incremental::body_contains_suspend(&func.body) {
                seeded_fn_susp[i] = true;
            }
        }
    }

    let pre_parts = pre_mono.into_pre_parts();
    if !diags.is_empty() {
        return Err(diags);
    }

    // Determine which modules are transitively imported by any non-reused (dirty)
    // function's module so generic templates in untouched leaf modules are skipped too.
    let mut dirty_mods: HashSet<usize> = HashSet::new();
    for (i, f) in items.fns.iter().enumerate() {
        if !reused[i]
            && (!f.generics.is_empty() && !base.reusable_modules.contains(&(f.module.0 as usize))
                || f.generics.is_empty())
        {
            dirty_mods.insert(f.module.0 as usize);
        }
    }
    let mut needed_generic_mods: HashSet<usize> = dirty_mods.clone();
    let mut work: Vec<usize> = dirty_mods.into_iter().collect();
    while let Some(m) = work.pop() {
        if let Some(scope) = items.modules.get(m) {
            for &imp_mid in scope.imports.values() {
                let idx = imp_mid.0 as usize;
                if needed_generic_mods.insert(idx) {
                    work.push(idx);
                }
            }
        }
    }

    // Phase B: run `infer_fn` ONLY on non-reused functions (and generic templates
    // reachable from dirty modules).
    let mut thir: Vec<Option<thir::ThirFn>> = vec![None; fn_count];
    for i in 0..fn_count {
        if is_ct_flags[i] {
            thir[i] = infer::infer_fn(
                &items,
                &mut types,
                &mut diags,
                &[],
                None,
                limits,
                thir::FnRef(i as u32),
            );
        }
    }
    for i in 0..fn_count {
        if is_ct_flags[i] || reused[i] {
            continue;
        }
        let f = &items.fns[i];
        if !f.generics.is_empty()
            && base.reusable_modules.contains(&(f.module.0 as usize))
            && !needed_generic_mods.contains(&(f.module.0 as usize))
        {
            continue;
        }
        let ct_ref: Option<&mut dyn ConstEvaluator> = match &mut ct {
            Some(c) => Some(&mut **c),
            None => None,
        };
        thir[i] = infer::infer_fn(
            &items,
            &mut types,
            &mut diags,
            &thir,
            ct_ref,
            limits,
            thir::FnRef(i as u32),
        );
    }
    if !diags.is_empty() {
        return Err(diags);
    }
    infer::check_defer_suspend_effects_with_seeds(
        &items,
        &types,
        &thir,
        &seeded_fn_susp,
        &mut diags,
    );
    if !diags.is_empty() {
        return Err(diags);
    }

    // Phase C: resume `Mono` with the pre-populated module and lower ONLY the
    // dirty non-reused roots (plus any closures/generic instances they demand).
    let mut mono = build::Mono::resume_from_pre_parts(
        pre_parts,
        &items,
        &mut types,
        &thir,
        &seeded_fn_susp,
        &mut diags,
        limits,
        ct,
    );
    for (i, root_id_opt) in pre.root_fn_ids.iter().enumerate() {
        if let Some(id) = *root_id_opt
            && !reused[i]
        {
            mono.enqueue_registered_root(thir::FnRef(i as u32), id);
        }
    }
    mono.run();
    let module = std::mem::take(&mut mono.module);
    let source_map = std::mem::take(&mut mono.source_map);
    drop(mono);
    if !diags.is_empty() {
        return Err(diags);
    }
    reused_certs.resize(module.functions.len(), None);
    Ok(Lowered {
        module,
        source_map,
        roots: pre.roots,
        reused_fns,
        total_root_fns,
        reused_certs,
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
