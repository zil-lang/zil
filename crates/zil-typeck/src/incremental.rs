//! Module export-signature dependency fingerprinting and incremental Body IR reuse.
//!
//! ## Soundness & Invariants
//! 1. `export_sig_fp(M)` hashes every externally visible declaration of `M`
//!    without depending on byte offsets (`Span`s), so adding/removing lines inside
//!    a private function or non-generic `pub fn` body does NOT perturb `export_sig_fp(M)`.
//! 2. `global_coherence_fp` hashes all `trait`, `impl`, and `extern` declarations
//!    across the entire program so cross-module method/trait resolution changes
//!    always invalidate conservatively.
//! 3. `dep_iface_fp(M)` hashes `(global_coherence_fp, transitive_imports(M) -> export_sig_fp)`.
//!    If `M` contains compile-time `#(...)` expressions or `Meta` items, `M` is
//!    marked `has_comptime = true` and always freshly typechecked/evaluated.
//! 4. A reused function's Body IR & `FunctionSourceMap` are remapped into the
//!    current `Module` transactionally (verifying exact structural match of every
//!    referenced `AdtDef`, `ForeignDecl`, and callee `FnSig`). Any mismatch rolls
//!    back cleanly and falls through to fresh `infer_fn` + `Mono::lower_fn`.
//! 5. Fresh `verify_ct_original_definitions`, `check_fidelity`, and `check_module`
//!    (`verification_runs == 1`) ALWAYS execute on the assembled module in `zil-driver`.

use crate::ModuleInput;
use crate::build::Mono;
use crate::items::FnRoleSpec;
use crate::thir::FnRef;
use std::collections::{HashMap, HashSet, VecDeque};
use zil_base::{Fingerprint, FingerprintBuilder, Span};
use zil_ir::body::{
    AdtDef, AdtId, AggregateKind, ArgsId, Block, BlockInvariant, Body, Callee, Certificate,
    DropFlag, FieldRef, FlagClaim, FnId, FnRole, FnSig, ForeignId, Function, FunctionCertificate,
    FunctionSourceMap, GenericArgs, Instruction, InstructionKind, LocalDecl, Module, OpLocation,
    Operand, Place, PlaceInit, Projection, Rvalue, SourceMap, SourceRelation, StaticId, TagClaim,
    Terminator, TyId, TyKind, VariantClaim,
};
use zil_syntax::ast;

/// Per-module structural fingerprint record stored in incremental baselines.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModuleDepRecord {
    pub logical_path: String,
    pub file_id: u32,
    pub text_fp: Fingerprint,
    pub export_sig_fp: Fingerprint,
    pub dep_iface_fp: Fingerprint,
    pub has_comptime: bool,
}

/// Incremental baseline supplied to [`crate::lower_program_incremental`].
pub struct IncrementalBaseline<'b> {
    pub prev_module: &'b Module,
    pub prev_source_map: &'b SourceMap,
    pub prev_cert: Option<&'b Certificate>,
    pub reusable_modules: &'b HashSet<usize>,
}

/// Computes per-module export-signature and transitive dependency fingerprints.
///
/// `source_texts[i]` is the raw UTF-8 text for `modules[i]` (used to slice exact
/// span-offset-invariant bytes for exported generic/CT/const expressions).
/// `import_edges[i]` lists the indices of modules directly imported by `modules[i]`.
pub fn compute_module_dep_records(
    modules: &[ModuleInput<'_>],
    file_ids: &[u32],
    text_fps: &[Fingerprint],
    source_texts: &[&str],
    import_edges: &[Vec<usize>],
) -> Vec<ModuleDepRecord> {
    let n = modules.len();
    let mut export_fps = Vec::with_capacity(n);
    let mut has_comptimes = Vec::with_capacity(n);

    for (i, m) in modules.iter().enumerate() {
        let src = source_texts.get(i).copied().unwrap_or("");
        export_fps.push(module_export_signature_fp(&m.path, m.ast, src));
        has_comptimes.push(module_has_comptime(m.ast));
    }

    let global_coherence = global_coherence_fp(modules, source_texts);

    let mut records = Vec::with_capacity(n);
    let mut visited = vec![false; n];
    let mut queue = VecDeque::new();
    let mut deps = Vec::new();

    for i in 0..n {
        visited.fill(false);
        queue.clear();
        deps.clear();
        visited[i] = true;
        if let Some(edges) = import_edges.get(i) {
            for &to in edges {
                if to < n && !visited[to] {
                    visited[to] = true;
                    queue.push_back(to);
                }
            }
        }
        while let Some(cur) = queue.pop_front() {
            deps.push(cur);
            if let Some(edges) = import_edges.get(cur) {
                for &to in edges {
                    if to < n && !visited[to] {
                        visited[to] = true;
                        queue.push_back(to);
                    }
                }
            }
        }
        deps.sort_by(|&a, &b| modules[a].path.cmp(&modules[b].path));

        let mut h = FingerprintBuilder::new();
        h.update(b"zil-mod-dep-v1\0");
        h.update(&global_coherence.0);
        h.update(&(deps.len() as u64).to_le_bytes());
        for &d in &deps {
            h.update(modules[d].path.as_bytes());
            h.update(b"\0");
            h.update(&export_fps[d].0);
            if has_comptimes[i] {
                h.update(&text_fps[d].0);
            }
        }

        records.push(ModuleDepRecord {
            logical_path: modules[i].path.clone(),
            file_id: file_ids.get(i).copied().unwrap_or(i as u32),
            text_fp: text_fps[i],
            export_sig_fp: export_fps[i],
            dep_iface_fp: h.digest(),
            has_comptime: has_comptimes[i],
        });
    }

    records
}

/// Returns the set of module indices whose source text, `FileId`, and transitive
/// imported export signatures are 100% unchanged relative to `prev`.
pub fn compute_reusable_modules(
    curr: &[ModuleDepRecord],
    prev: &[ModuleDepRecord],
) -> HashSet<usize> {
    let prev_by_path: HashMap<&str, &ModuleDepRecord> =
        prev.iter().map(|r| (r.logical_path.as_str(), r)).collect();
    let mut reusable = HashSet::new();
    for (i, c) in curr.iter().enumerate() {
        if c.has_comptime {
            continue;
        }
        if let Some(p) = prev_by_path.get(c.logical_path.as_str())
            && p.file_id == c.file_id
            && p.text_fp == c.text_fp
            && p.dep_iface_fp == c.dep_iface_fp
            && !p.has_comptime
        {
            reusable.insert(i);
        }
    }
    reusable
}

// ---------------------------------------------------------------------------
// Span-independent export-signature hashing
// ---------------------------------------------------------------------------

fn hash_str(h: &mut FingerprintBuilder, s: &str) {
    h.update(&(s.len() as u64).to_le_bytes());
    h.update(s.as_bytes());
}

fn hash_path(h: &mut FingerprintBuilder, p: &[String]) {
    h.update(&(p.len() as u64).to_le_bytes());
    for seg in p {
        hash_str(h, seg);
    }
}

fn hash_span_slice(h: &mut FingerprintBuilder, src: &str, span: Span) {
    let start = span.start;
    let end = span.end;
    if let Some(slice) = src.get(start..end) {
        h.update(b"S");
        hash_str(h, slice);
    } else {
        h.update(b"X");
        h.update(&span.start.to_le_bytes());
        h.update(&span.end.to_le_bytes());
    }
}

fn type_ref_mentions_ct_type(ty: &ast::TypeRef) -> bool {
    match &ty.kind {
        ast::TypeKind::Named { path, args } => {
            (path.len() == 1 && path[0] == "Type")
                || args.iter().any(|a| match a {
                    ast::GenericArg::Type(t) => type_ref_mentions_ct_type(t),
                    ast::GenericArg::Lifetime(_)
                    | ast::GenericArg::Const(_)
                    | ast::GenericArg::AssociatedBinding(_) => false,
                })
        }
        ast::TypeKind::Reference { inner, .. }
        | ast::TypeKind::Group(inner)
        | ast::TypeKind::Array { element: inner, .. }
        | ast::TypeKind::Optional(inner)
        | ast::TypeKind::ForLifetime { inner, .. } => type_ref_mentions_ct_type(inner),
        ast::TypeKind::Tuple(ts) | ast::TypeKind::Union(ts) | ast::TypeKind::Impl(ts) => {
            ts.iter().any(type_ref_mentions_ct_type)
        }
        ast::TypeKind::Function { params, result }
        | ast::TypeKind::ForeignFunction { params, result, .. } => {
            params.iter().any(type_ref_mentions_ct_type) || type_ref_mentions_ct_type(result)
        }
        ast::TypeKind::Error { error, result } => {
            type_ref_mentions_ct_type(error) || type_ref_mentions_ct_type(result)
        }
        ast::TypeKind::Dyn { .. } => false,
        ast::TypeKind::MetaEval(_) | ast::TypeKind::MetaSplice(_) | ast::TypeKind::MetaBlock(_) => {
            true
        }
    }
}

fn fn_is_ct_type_ast(f: &ast::Function) -> bool {
    f.return_type
        .as_ref()
        .is_some_and(type_ref_mentions_ct_type)
        || f.params.iter().any(|p| type_ref_mentions_ct_type(&p.ty))
}

fn hash_attrs(h: &mut FingerprintBuilder, attrs: &[ast::Attribute], src: &str) {
    h.update(&(attrs.len() as u64).to_le_bytes());
    for a in attrs {
        hash_path(h, &a.path);
        if let Some(sv) = &a.string_value {
            h.update(b"1");
            hash_str(h, sv);
        } else {
            h.update(b"0");
        }
        h.update(&(a.arguments.len() as u64).to_le_bytes());
        for arg in &a.arguments {
            hash_span_slice(h, src, arg.span);
        }
    }
}

fn hash_generic_arg(h: &mut FingerprintBuilder, arg: &ast::GenericArg, src: &str) {
    match arg {
        ast::GenericArg::Lifetime(l) => {
            h.update(b"L");
            hash_str(h, l);
        }
        ast::GenericArg::Type(t) => {
            h.update(b"T");
            hash_type_ref(h, t, src);
        }
        ast::GenericArg::Const(e) => {
            h.update(b"C");
            hash_span_slice(h, src, e.span);
        }
        ast::GenericArg::AssociatedBinding(b) => {
            h.update(b"B");
            hash_dyn_binding(h, b, src);
        }
    }
}

fn hash_dyn_binding(h: &mut FingerprintBuilder, b: &ast::DynBinding, src: &str) {
    match &b.kind {
        ast::DynBindingKind::Type { member, value } => {
            h.update(b"BT");
            hash_path(h, member);
            hash_type_ref(h, value, src);
        }
        ast::DynBindingKind::Constant { member, ty, value } => {
            h.update(b"BC");
            hash_path(h, member);
            hash_type_ref(h, ty, src);
            hash_span_slice(h, src, value.span);
        }
        ast::DynBindingKind::Method { member, arguments } => {
            h.update(b"BM");
            hash_path(h, member);
            h.update(&(arguments.len() as u64).to_le_bytes());
            for a in arguments {
                hash_generic_arg(h, a, src);
            }
        }
    }
}

fn hash_type_ref(h: &mut FingerprintBuilder, ty: &ast::TypeRef, src: &str) {
    match &ty.kind {
        ast::TypeKind::Named { path, args } => {
            h.update(b"TN");
            hash_path(h, path);
            h.update(&(args.len() as u64).to_le_bytes());
            for a in args {
                hash_generic_arg(h, a, src);
            }
        }
        ast::TypeKind::Reference {
            lifetime,
            mutable,
            inner,
        } => {
            h.update(b"TR");
            h.update(&[u8::from(*mutable)]);
            if let Some(l) = lifetime {
                h.update(b"1");
                hash_str(h, l);
            } else {
                h.update(b"0");
            }
            hash_type_ref(h, inner, src);
        }
        ast::TypeKind::Tuple(ts) => {
            h.update(b"TT");
            h.update(&(ts.len() as u64).to_le_bytes());
            for t in ts {
                hash_type_ref(h, t, src);
            }
        }
        ast::TypeKind::Group(inner) => {
            h.update(b"TG");
            hash_type_ref(h, inner, src);
        }
        ast::TypeKind::Array { element, length } => {
            h.update(b"TA");
            hash_type_ref(h, element, src);
            if let Some(len) = length {
                h.update(b"1");
                hash_span_slice(h, src, len.span);
            } else {
                h.update(b"0");
            }
        }
        ast::TypeKind::Function { params, result } => {
            h.update(b"TF");
            h.update(&(params.len() as u64).to_le_bytes());
            for p in params {
                hash_type_ref(h, p, src);
            }
            hash_type_ref(h, result, src);
        }
        ast::TypeKind::ForeignFunction {
            abi,
            params,
            result,
        } => {
            h.update(b"TX");
            hash_str(h, abi);
            h.update(&(params.len() as u64).to_le_bytes());
            for p in params {
                hash_type_ref(h, p, src);
            }
            hash_type_ref(h, result, src);
        }
        ast::TypeKind::Optional(inner) => {
            h.update(b"TO");
            hash_type_ref(h, inner, src);
        }
        ast::TypeKind::Error { error, result } => {
            h.update(b"TE");
            hash_type_ref(h, error, src);
            hash_type_ref(h, result, src);
        }
        ast::TypeKind::Union(ts) => {
            h.update(b"TU");
            h.update(&(ts.len() as u64).to_le_bytes());
            for t in ts {
                hash_type_ref(h, t, src);
            }
        }
        ast::TypeKind::Dyn {
            path,
            args,
            bindings,
        } => {
            h.update(b"TD");
            hash_path(h, path);
            h.update(&(args.len() as u64).to_le_bytes());
            for a in args {
                hash_generic_arg(h, a, src);
            }
            h.update(&(bindings.len() as u64).to_le_bytes());
            for b in bindings {
                hash_dyn_binding(h, b, src);
            }
        }
        ast::TypeKind::ForLifetime { lifetime, inner } => {
            h.update(b"TL");
            hash_str(h, lifetime);
            hash_type_ref(h, inner, src);
        }
        ast::TypeKind::Impl(ts) => {
            h.update(b"TI");
            h.update(&(ts.len() as u64).to_le_bytes());
            for t in ts {
                hash_type_ref(h, t, src);
            }
        }
        ast::TypeKind::MetaEval(e) => {
            h.update(b"TME");
            hash_span_slice(h, src, e.span);
        }
        ast::TypeKind::MetaSplice(e) => {
            h.update(b"TMS");
            hash_span_slice(h, src, e.span);
        }
        ast::TypeKind::MetaBlock(b) => {
            h.update(b"TMB");
            hash_span_slice(h, src, b.span);
        }
    }
}

fn hash_generic_params(h: &mut FingerprintBuilder, gs: &[ast::GenericParam], src: &str) {
    h.update(&(gs.len() as u64).to_le_bytes());
    for g in gs {
        hash_str(h, &g.name);
        match &g.kind {
            ast::GenericParamKind::Type(bounds) => {
                h.update(b"GT");
                h.update(&(bounds.len() as u64).to_le_bytes());
                for b in bounds {
                    hash_type_ref(h, b, src);
                }
            }
            ast::GenericParamKind::Lifetime => h.update(b"GL"),
            ast::GenericParamKind::Const(ty) => {
                h.update(b"GC");
                hash_type_ref(h, ty, src);
            }
        }
    }
}

fn hash_bound_clauses(h: &mut FingerprintBuilder, clauses: &[ast::BoundClause], src: &str) {
    h.update(&(clauses.len() as u64).to_le_bytes());
    for c in clauses {
        match &c.subject {
            ast::BoundSubject::Type(t) => {
                h.update(b"BT");
                hash_type_ref(h, t, src);
            }
            ast::BoundSubject::Lifetime(l) => {
                h.update(b"BL");
                hash_str(h, l);
            }
        }
        h.update(&(c.bounds.len() as u64).to_le_bytes());
        for b in &c.bounds {
            hash_type_ref(h, b, src);
        }
        if let Some(lb) = &c.lifetime_bound {
            h.update(b"1");
            hash_str(h, lb);
        } else {
            h.update(b"0");
        }
    }
}

fn hash_contract_term(h: &mut FingerprintBuilder, term: &ast::ContractTerm, src: &str) {
    hash_path(h, &term.path);
    h.update(&(term.type_args.len() as u64).to_le_bytes());
    for a in &term.type_args {
        hash_generic_arg(h, a, src);
    }
    h.update(&(term.arguments.len() as u64).to_le_bytes());
    for a in &term.arguments {
        hash_contract_term(h, a, src);
    }
    if let Some(idx) = &term.index {
        h.update(b"1");
        hash_contract_term(h, idx, src);
    } else {
        h.update(b"0");
    }
}

fn hash_contracts(h: &mut FingerprintBuilder, clauses: &[ast::ContractClause], src: &str) {
    h.update(&(clauses.len() as u64).to_le_bytes());
    for c in clauses {
        hash_str(h, &c.name);
        h.update(&(c.terms.len() as u64).to_le_bytes());
        for t in &c.terms {
            hash_contract_term(h, t, src);
        }
    }
}

fn hash_fn_signature(
    h: &mut FingerprintBuilder,
    f: &ast::Function,
    public: bool,
    attrs: &[ast::Attribute],
    src: &str,
    include_body: bool,
) {
    h.update(b"FN");
    h.update(&[u8::from(public)]);
    hash_attrs(h, attrs, src);
    hash_str(h, &f.name);
    hash_generic_params(h, &f.generics, src);
    h.update(&(f.params.len() as u64).to_le_bytes());
    for p in &f.params {
        hash_str(h, &p.name);
        h.update(&[u8::from(p.give)]);
        hash_attrs(h, &p.attributes, src);
        hash_type_ref(h, &p.ty, src);
    }
    if let Some(rt) = &f.return_type {
        h.update(b"1");
        hash_type_ref(h, rt, src);
    } else {
        h.update(b"0");
    }
    hash_bound_clauses(h, &f.where_clause, src);
    hash_contracts(h, &f.contract, src);
    // Conservative suspend bit: if a pub fn's body mentions suspend/yield, mark it.
    let body_has_suspend = match &f.body {
        ast::FunctionBody::Block(b) => src
            .get(b.span.start..b.span.end)
            .is_some_and(|s| s.contains("suspend") || s.contains("task_yield")),
        ast::FunctionBody::Expression(e) => src
            .get(e.span.start..e.span.end)
            .is_some_and(|s| s.contains("suspend") || s.contains("task_yield")),
        ast::FunctionBody::Declaration => false,
    };
    h.update(&[u8::from(body_has_suspend)]);
    if include_body {
        match &f.body {
            ast::FunctionBody::Block(b) => {
                h.update(b"BB");
                hash_span_slice(h, src, b.span);
            }
            ast::FunctionBody::Expression(e) => {
                h.update(b"BE");
                hash_span_slice(h, src, e.span);
            }
            ast::FunctionBody::Declaration => h.update(b"BD"),
        }
    }
}

fn hash_field(h: &mut FingerprintBuilder, f: &ast::Field, src: &str) {
    hash_str(h, &f.name);
    h.update(&[u8::from(f.public), u8::from(f.manages)]);
    hash_attrs(h, &f.attributes, src);
    hash_type_ref(h, &f.ty, src);
    if let Some(def) = &f.default {
        h.update(b"1");
        hash_span_slice(h, src, def.span);
    } else {
        h.update(b"0");
    }
}

fn hash_aggregate(
    h: &mut FingerprintBuilder,
    agg: &ast::Aggregate,
    public: bool,
    attrs: &[ast::Attribute],
    src: &str,
) {
    h.update(b"AGG");
    h.update(&[
        u8::from(public),
        match agg.kind {
            ast::AggregateKind::Struct => 0,
            ast::AggregateKind::Resource => 1,
            ast::AggregateKind::Package => 2,
        },
    ]);
    hash_attrs(h, attrs, src);
    hash_str(h, &agg.name);
    hash_generic_params(h, &agg.generics, src);
    h.update(&(agg.fields.len() as u64).to_le_bytes());
    for f in &agg.fields {
        hash_field(h, f, src);
    }
    h.update(&(agg.methods.len() as u64).to_le_bytes());
    for m in &agg.methods {
        if let ast::ItemKind::Function(f) = &m.kind {
            let inc_body =
                !agg.generics.is_empty() || !f.generics.is_empty() || fn_is_ct_type_ast(f);
            hash_fn_signature(h, f, m.public, &m.attributes, src, inc_body);
        }
    }
}

fn hash_enum(
    h: &mut FingerprintBuilder,
    en: &ast::EnumDecl,
    public: bool,
    attrs: &[ast::Attribute],
    src: &str,
) {
    h.update(b"EN");
    h.update(&[u8::from(public)]);
    hash_attrs(h, attrs, src);
    hash_str(h, &en.name);
    hash_generic_params(h, &en.generics, src);
    h.update(&(en.variants.len() as u64).to_le_bytes());
    for v in &en.variants {
        hash_str(h, &v.name);
        hash_attrs(h, &v.attributes, src);
        match &v.fields {
            ast::VariantFields::Unit => h.update(b"U"),
            ast::VariantFields::Tuple(fs) => {
                h.update(b"T");
                h.update(&(fs.len() as u64).to_le_bytes());
                for f in fs {
                    h.update(&[u8::from(f.manages)]);
                    hash_type_ref(h, &f.ty, src);
                }
            }
            ast::VariantFields::Record(fs) => {
                h.update(b"R");
                h.update(&(fs.len() as u64).to_le_bytes());
                for f in fs {
                    hash_field(h, f, src);
                }
            }
        }
    }
}

fn hash_trait(
    h: &mut FingerprintBuilder,
    tr: &ast::TraitDecl,
    public: bool,
    attrs: &[ast::Attribute],
    src: &str,
) {
    h.update(b"TR");
    h.update(&[u8::from(public)]);
    hash_attrs(h, attrs, src);
    hash_str(h, &tr.name);
    hash_generic_params(h, &tr.generics, src);
    h.update(&(tr.bounds.len() as u64).to_le_bytes());
    for b in &tr.bounds {
        hash_type_ref(h, b, src);
    }
    h.update(&(tr.members.len() as u64).to_le_bytes());
    for m in &tr.members {
        match &m.kind {
            ast::ItemKind::Function(f) => {
                // Default method bodies affect callers/implementors, so include body.
                hash_fn_signature(h, f, m.public, &m.attributes, src, true);
            }
            ast::ItemKind::TypeAlias(ta) => {
                h.update(b"TA");
                hash_str(h, &ta.name);
                hash_generic_params(h, &ta.generics, src);
                for b in &ta.bounds {
                    hash_type_ref(h, b, src);
                }
                if let Some(v) = &ta.value {
                    h.update(b"1");
                    hash_type_ref(h, v, src);
                } else {
                    h.update(b"0");
                }
            }
            ast::ItemKind::Const(c) => {
                h.update(b"TC");
                hash_str(h, &c.name);
                if let Some(t) = &c.ty {
                    hash_type_ref(h, t, src);
                }
                if let Some(v) = &c.value {
                    hash_span_slice(h, src, v.span);
                }
            }
            ast::ItemKind::Import(_)
            | ast::ItemKind::Aggregate(_)
            | ast::ItemKind::Enum(_)
            | ast::ItemKind::Trait(_)
            | ast::ItemKind::Impl(_)
            | ast::ItemKind::Extern { .. }
            | ast::ItemKind::Meta(_) => {
                hash_span_slice(h, src, m.span);
            }
        }
    }
}

fn hash_impl(
    h: &mut FingerprintBuilder,
    imp: &ast::ImplBlock,
    attrs: &[ast::Attribute],
    src: &str,
) {
    h.update(b"IMP");
    hash_attrs(h, attrs, src);
    hash_generic_params(h, &imp.generics, src);
    if let Some(tr) = &imp.trait_type {
        h.update(b"1");
        hash_type_ref(h, tr, src);
    } else {
        h.update(b"0");
    }
    hash_type_ref(h, &imp.target, src);
    if let Some(rd) = &imp.receiver_default {
        h.update(b"1");
        hash_str(h, &rd.name);
        hash_type_ref(h, &rd.ty, src);
    } else {
        h.update(b"0");
    }
    hash_bound_clauses(h, &imp.where_clause, src);
    h.update(&(imp.members.len() as u64).to_le_bytes());
    for m in &imp.members {
        match &m.kind {
            ast::ItemKind::Function(f) => {
                let inc_body =
                    !imp.generics.is_empty() || !f.generics.is_empty() || fn_is_ct_type_ast(f);
                hash_fn_signature(h, f, m.public, &m.attributes, src, inc_body);
            }
            ast::ItemKind::TypeAlias(ta) => {
                h.update(b"ITA");
                hash_str(h, &ta.name);
                if let Some(v) = &ta.value {
                    hash_type_ref(h, v, src);
                }
            }
            ast::ItemKind::Const(c) => {
                h.update(b"ITC");
                hash_str(h, &c.name);
                if let Some(t) = &c.ty {
                    hash_type_ref(h, t, src);
                }
                if let Some(v) = &c.value {
                    hash_span_slice(h, src, v.span);
                }
            }
            ast::ItemKind::Import(_)
            | ast::ItemKind::Aggregate(_)
            | ast::ItemKind::Enum(_)
            | ast::ItemKind::Trait(_)
            | ast::ItemKind::Impl(_)
            | ast::ItemKind::Extern { .. }
            | ast::ItemKind::Meta(_) => {
                hash_span_slice(h, src, m.span);
            }
        }
    }
}

/// Computes the span-offset-independent export signature fingerprint of one module.
pub fn module_export_signature_fp(
    logical_path: &str,
    ast_mod: &ast::Module,
    src: &str,
) -> Fingerprint {
    let mut h = FingerprintBuilder::new();
    h.update(b"zil-mod-export-v1\0");
    hash_str(&mut h, logical_path);

    for item in &ast_mod.items {
        match &item.kind {
            ast::ItemKind::Import(imp) => {
                h.update(b"IM");
                hash_path(&mut h, &imp.path);
                if let Some(a) = &imp.alias {
                    h.update(b"1");
                    hash_str(&mut h, a);
                } else {
                    h.update(b"0");
                }
                h.update(&[u8::from(imp.syntax)]);
            }
            ast::ItemKind::Function(f) => {
                if !item.public {
                    // Private non-exported function: does NOT affect other modules' exports!
                    continue;
                }
                let inc_body = !f.generics.is_empty() || fn_is_ct_type_ast(f);
                hash_fn_signature(&mut h, f, item.public, &item.attributes, src, inc_body);
            }
            ast::ItemKind::Aggregate(agg) => {
                // Hash all aggregates (even private ones may appear in public type aliases or impls).
                hash_aggregate(&mut h, agg, item.public, &item.attributes, src);
            }
            ast::ItemKind::Enum(en) => {
                hash_enum(&mut h, en, item.public, &item.attributes, src);
            }
            ast::ItemKind::Trait(tr) => {
                hash_trait(&mut h, tr, item.public, &item.attributes, src);
            }
            ast::ItemKind::Impl(imp) => {
                hash_impl(&mut h, imp, &item.attributes, src);
            }
            ast::ItemKind::TypeAlias(ta) => {
                h.update(b"TYA");
                h.update(&[u8::from(item.public)]);
                hash_attrs(&mut h, &item.attributes, src);
                hash_str(&mut h, &ta.name);
                hash_generic_params(&mut h, &ta.generics, src);
                for b in &ta.bounds {
                    hash_type_ref(&mut h, b, src);
                }
                if let Some(v) = &ta.value {
                    h.update(b"1");
                    hash_type_ref(&mut h, v, src);
                } else {
                    h.update(b"0");
                }
            }
            ast::ItemKind::Const(c) => {
                if !item.public {
                    continue;
                }
                h.update(b"CN");
                hash_attrs(&mut h, &item.attributes, src);
                hash_str(&mut h, &c.name);
                if let Some(t) = &c.ty {
                    h.update(b"1");
                    hash_type_ref(&mut h, t, src);
                } else {
                    h.update(b"0");
                }
                if let Some(v) = &c.value {
                    h.update(b"1");
                    hash_span_slice(&mut h, src, v.span);
                } else {
                    h.update(b"0");
                }
            }
            ast::ItemKind::Extern { abi, function } => {
                h.update(b"EXT");
                hash_str(&mut h, abi);
                hash_fn_signature(&mut h, function, item.public, &item.attributes, src, false);
            }
            ast::ItemKind::Meta(_) => {
                h.update(b"META");
                hash_span_slice(&mut h, src, item.span);
            }
        }
    }
    h.digest()
}

fn global_coherence_fp(modules: &[ModuleInput<'_>], source_texts: &[&str]) -> Fingerprint {
    let mut h = FingerprintBuilder::new();
    h.update(b"zil-global-coherence-v1\0");
    for (i, m) in modules.iter().enumerate() {
        let src = source_texts.get(i).copied().unwrap_or("");
        for item in &m.ast.items {
            match &item.kind {
                ast::ItemKind::Trait(tr) => {
                    hash_str(&mut h, &m.path);
                    hash_trait(&mut h, tr, item.public, &item.attributes, src);
                }
                ast::ItemKind::Impl(imp) => {
                    hash_str(&mut h, &m.path);
                    hash_impl(&mut h, imp, &item.attributes, src);
                }
                ast::ItemKind::Extern { abi, function } => {
                    hash_str(&mut h, &m.path);
                    h.update(b"EXT");
                    hash_str(&mut h, abi);
                    hash_fn_signature(&mut h, function, item.public, &item.attributes, src, false);
                }
                ast::ItemKind::Import(_)
                | ast::ItemKind::Function(_)
                | ast::ItemKind::Aggregate(_)
                | ast::ItemKind::Enum(_)
                | ast::ItemKind::TypeAlias(_)
                | ast::ItemKind::Const(_)
                | ast::ItemKind::Meta(_) => {}
            }
        }
    }
    h.digest()
}

fn module_has_comptime(ast_mod: &ast::Module) -> bool {
    for item in &ast_mod.items {
        match &item.kind {
            ast::ItemKind::Meta(_) => return true,
            ast::ItemKind::Function(f) => {
                if fn_is_ct_type_ast(f) || fn_body_has_comptime(&f.body) {
                    return true;
                }
            }
            ast::ItemKind::Aggregate(a) => {
                if a.fields.iter().any(|f| {
                    type_ref_mentions_ct_type(&f.ty)
                        || f.default.as_ref().is_some_and(expr_has_comptime)
                }) {
                    return true;
                }
                for m in &a.methods {
                    if let ast::ItemKind::Function(f) = &m.kind
                        && (fn_is_ct_type_ast(f) || fn_body_has_comptime(&f.body))
                    {
                        return true;
                    }
                }
            }
            ast::ItemKind::Impl(imp) => {
                for m in &imp.members {
                    if let ast::ItemKind::Function(f) = &m.kind
                        && (fn_is_ct_type_ast(f) || fn_body_has_comptime(&f.body))
                    {
                        return true;
                    }
                }
            }
            ast::ItemKind::Trait(tr) => {
                for m in &tr.members {
                    if let ast::ItemKind::Function(f) = &m.kind
                        && (fn_is_ct_type_ast(f) || fn_body_has_comptime(&f.body))
                    {
                        return true;
                    }
                }
            }
            ast::ItemKind::Const(c) => {
                if c.value.as_ref().is_some_and(expr_has_comptime) {
                    return true;
                }
            }
            ast::ItemKind::Import(_)
            | ast::ItemKind::Enum(_)
            | ast::ItemKind::TypeAlias(_)
            | ast::ItemKind::Extern { .. } => {}
        }
    }
    false
}

fn fn_body_has_comptime(body: &ast::FunctionBody) -> bool {
    match body {
        ast::FunctionBody::Block(b) => block_has_comptime(b),
        ast::FunctionBody::Expression(e) => expr_has_comptime(e),
        ast::FunctionBody::Declaration => false,
    }
}

fn block_has_comptime(block: &ast::Block) -> bool {
    block.statements.iter().any(stmt_has_comptime)
}

fn stmt_has_comptime(stmt: &ast::Stmt) -> bool {
    match &stmt.kind {
        ast::StmtKind::Binding { value, ty, .. } => {
            ty.as_ref().is_some_and(type_ref_mentions_ct_type) || expr_has_comptime(value)
        }
        ast::StmtKind::Return(e) => e.as_ref().is_some_and(expr_has_comptime),
        ast::StmtKind::Yield(e) => expr_has_comptime(e),
        ast::StmtKind::Break | ast::StmtKind::Continue => false,
        ast::StmtKind::Defer(b) | ast::StmtKind::Loop(b) | ast::StmtKind::Unsafe(b) => {
            block_has_comptime(b)
        }
        ast::StmtKind::If {
            condition,
            then_body,
            else_body,
        } => {
            cond_has_comptime(condition)
                || block_has_comptime(then_body)
                || else_body.as_ref().is_some_and(|eb| match eb {
                    ast::ElseBranch::Block(b) => block_has_comptime(b),
                    ast::ElseBranch::If(s) => stmt_has_comptime(s),
                })
        }
        ast::StmtKind::While { condition, body } => {
            cond_has_comptime(condition) || block_has_comptime(body)
        }
        ast::StmtKind::For { iterable, body, .. } => {
            expr_has_comptime(iterable) || block_has_comptime(body)
        }
        ast::StmtKind::Match { value, arms } => {
            expr_has_comptime(value)
                || arms.iter().any(|a| {
                    a.guard.as_ref().is_some_and(expr_has_comptime)
                        || match &a.body {
                            ast::ArrowBody::Expression(ex) => expr_has_comptime(ex),
                            ast::ArrowBody::Block(b) => block_has_comptime(b),
                        }
                })
        }
        ast::StmtKind::With { value, body, .. } => {
            expr_has_comptime(value) || block_has_comptime(body)
        }
        ast::StmtKind::Assign { target, value, .. } => {
            expr_has_comptime(target) || expr_has_comptime(value)
        }
        ast::StmtKind::Expression(e) => expr_has_comptime(e),
        ast::StmtKind::Meta(_) => true,
    }
}

fn cond_has_comptime(c: &ast::Condition) -> bool {
    match c {
        ast::Condition::Expression(e) => expr_has_comptime(e),
        ast::Condition::Let { value, .. } => expr_has_comptime(value),
    }
}

fn expr_has_comptime(e: &ast::Expr) -> bool {
    match &e.kind {
        ast::ExprKind::MetaEval(_)
        | ast::ExprKind::MetaSplice(_)
        | ast::ExprKind::MetaBlock(_)
        | ast::ExprKind::Quote { .. }
        | ast::ExprKind::SyntaxIsland { .. } => true,
        ast::ExprKind::Literal(_) | ast::ExprKind::Name(_) | ast::ExprKind::ReceiverMember(_) => {
            false
        }
        ast::ExprKind::Group(inner)
        | ast::ExprKind::Unary { operand: inner, .. }
        | ast::ExprKind::Try(inner)
        | ast::ExprKind::Member { object: inner, .. } => expr_has_comptime(inner),
        ast::ExprKind::Ascribed { ty, value } => {
            type_ref_mentions_ct_type(ty) || expr_has_comptime(value)
        }
        ast::ExprKind::Tuple(es) | ast::ExprKind::Array(es) => es.iter().any(expr_has_comptime),
        ast::ExprKind::Binary { left, right, .. }
        | ast::ExprKind::Pipeline {
            input: left,
            target: right,
        }
        | ast::ExprKind::Index {
            object: left,
            index: right,
        } => expr_has_comptime(left) || expr_has_comptime(right),
        ast::ExprKind::Slice { object, start, end } => {
            expr_has_comptime(object)
                || start.as_ref().is_some_and(|x| expr_has_comptime(x))
                || end.as_ref().is_some_and(|x| expr_has_comptime(x))
        }
        ast::ExprKind::TypeApply { callee, arguments } => {
            expr_has_comptime(callee)
                || arguments.iter().any(|a| match a {
                    ast::GenericArg::Type(t) => type_ref_mentions_ct_type(t),
                    ast::GenericArg::Const(ce) => expr_has_comptime(ce),
                    ast::GenericArg::Lifetime(_) | ast::GenericArg::AssociatedBinding(_) => false,
                })
        }
        ast::ExprKind::Call { callee, arguments } => {
            expr_has_comptime(callee) || arguments.iter().any(|a| expr_has_comptime(&a.value))
        }
        ast::ExprKind::Record { ty, base, fields } => {
            ty.as_ref().is_some_and(|t| expr_has_comptime(t))
                || base.as_ref().is_some_and(|b| expr_has_comptime(b))
                || fields
                    .iter()
                    .any(|f| f.value.as_ref().is_some_and(expr_has_comptime))
        }
        ast::ExprKind::Closure { body, .. } => match body {
            ast::ArrowBody::Expression(ex) => expr_has_comptime(ex),
            ast::ArrowBody::Block(b) => block_has_comptime(b),
        },
        ast::ExprKind::If {
            condition,
            then_value,
            else_value,
        } => {
            expr_has_comptime(condition)
                || expr_has_comptime(then_value)
                || expr_has_comptime(else_value)
        }
    }
}

// ---------------------------------------------------------------------------
// Transactional Body IR Function Remapper (prev_module -> curr_module)
// ---------------------------------------------------------------------------

pub(crate) struct PrevModuleIndex<'b> {
    prev_module: &'b Module,
    prev_fn_by_name: HashMap<&'b str, FnId>,
    prev_smap_by_fn: HashMap<FnId, &'b FunctionSourceMap>,
    prev_cert_by_fn: HashMap<FnId, &'b FunctionCertificate>,
}

impl<'b> PrevModuleIndex<'b> {
    pub(crate) fn new(
        prev_module: &'b Module,
        prev_source_map: &'b SourceMap,
        prev_cert: Option<&'b Certificate>,
    ) -> Self {
        let mut prev_fn_by_name = HashMap::with_capacity(prev_module.functions.len());
        for (i, f) in prev_module.functions.iter().enumerate() {
            prev_fn_by_name.insert(f.name.as_str(), FnId::from_index(i));
        }
        let mut prev_smap_by_fn = HashMap::with_capacity(prev_source_map.functions.len());
        for sm in &prev_source_map.functions {
            prev_smap_by_fn.insert(sm.function, sm);
        }
        let mut prev_cert_by_fn = HashMap::new();
        if let Some(cert) = prev_cert {
            prev_cert_by_fn.reserve(cert.functions.len());
            for fc in &cert.functions {
                prev_cert_by_fn.insert(fc.function, fc);
            }
        }
        Self {
            prev_module,
            prev_fn_by_name,
            prev_smap_by_fn,
            prev_cert_by_fn,
        }
    }
}

/// Checks whether a body contains any `TaskYield` or suspending call (`cancel.is_some()`).
pub(crate) fn body_contains_suspend(body: &Body) -> bool {
    for blk in &body.blocks {
        if let Terminator::Call { callee, cancel, .. } = &blk.terminator
            && (cancel.is_some()
                || matches!(
                    callee,
                    Callee::Intrinsic(zil_ir::body::IntrinsicOp::TaskYield)
                ))
        {
            return true;
        }
    }
    false
}

/// Attempts to reuse `prev_module`'s lowered `Body`, `FunctionSourceMap`, and
/// optional candidate `FunctionCertificate` for root function `curr_fn_id`
/// (canonical name `fn_name`) in `mono`.
///
/// Returns `Some(remapped_cert)` when the body & source map were reused cleanly,
/// or `None` after rolling back cleanly so the caller runs normal `infer_fn` +
/// `Mono::lower_fn`.
pub(crate) fn try_reuse_root_fn(
    prev_idx: &PrevModuleIndex<'_>,
    fn_name: &str,
    curr_fn_id: FnId,
    curr_root_by_name: &HashMap<String, FnId>,
    curr_adt_by_name: &HashMap<String, AdtId>,
    curr_foreign_by_sym: &HashMap<String, ForeignId>,
    mono: &mut Mono<'_, '_, '_>,
) -> Option<Option<FunctionCertificate>> {
    let &prev_fn_id = prev_idx.prev_fn_by_name.get(fn_name)?;
    let prev_smap = prev_idx.prev_smap_by_fn.get(&prev_fn_id)?;
    let prev_fn = &prev_idx.prev_module.functions[prev_fn_id.index()];
    if prev_fn.body.closure.is_some() || !prev_fn.body.captures.is_empty() {
        return None;
    }

    let checkpoint_statics = mono.module.statics.len();
    let checkpoint_smaps = mono.source_map.functions.len();

    let mut remapper = FnRemapper {
        prev: prev_idx.prev_module,
        curr_root_by_name,
        curr_adt_by_name,
        curr_foreign_by_sym,
        ty_map: HashMap::new(),
        args_map: HashMap::new(),
        adt_verified: HashMap::new(),
        mono,
    };

    let Some(remapped_sig) = remapper.remap_fn_sig(&prev_fn.sig) else {
        remapper.rollback(checkpoint_statics, checkpoint_smaps);
        return None;
    };
    if remapped_sig != remapper.mono.module.functions[curr_fn_id.index()].sig {
        remapper.rollback(checkpoint_statics, checkpoint_smaps);
        return None;
    }
    let Some(remapped_body) = remapper.remap_body(&prev_fn.body) else {
        remapper.rollback(checkpoint_statics, checkpoint_smaps);
        return None;
    };
    let Some(remapped_smap) = remapper.remap_source_map(prev_smap, curr_fn_id) else {
        remapper.rollback(checkpoint_statics, checkpoint_smaps);
        return None;
    };
    let remapped_cert = prev_idx
        .prev_cert_by_fn
        .get(&prev_fn_id)
        .and_then(|prev_fc| remapper.remap_fn_cert(prev_fc, curr_fn_id));

    remapper.mono.module.functions[curr_fn_id.index()].body = remapped_body;
    remapper.mono.source_map.functions.push(remapped_smap);
    Some(remapped_cert)
}

struct FnRemapper<'m, 'c, 'a, 'e, 'b> {
    prev: &'b Module,
    curr_root_by_name: &'b HashMap<String, FnId>,
    curr_adt_by_name: &'b HashMap<String, AdtId>,
    curr_foreign_by_sym: &'b HashMap<String, ForeignId>,
    ty_map: HashMap<TyId, TyId>,
    args_map: HashMap<ArgsId, ArgsId>,
    adt_verified: HashMap<AdtId, AdtId>,
    mono: &'m mut Mono<'c, 'a, 'e>,
}

impl<'m, 'c, 'a, 'e, 'b> FnRemapper<'m, 'c, 'a, 'e, 'b> {
    fn rollback(&mut self, checkpoint_statics: usize, checkpoint_smaps: usize) {
        self.mono.module.statics.truncate(checkpoint_statics);
        self.mono.source_map.functions.truncate(checkpoint_smaps);
    }

    fn remap_adt_id(&mut self, prev_id: AdtId) -> Option<AdtId> {
        if let Some(&mapped) = self.adt_verified.get(&prev_id) {
            return Some(mapped);
        }
        let prev_def: &AdtDef = self.prev.tys.adt(prev_id);
        let &curr_id = self.curr_adt_by_name.get(&prev_def.name)?;
        // Tentatively insert before checking fields to handle recursive ADT references cleanly.
        self.adt_verified.insert(prev_id, curr_id);
        let curr_def = self.mono.module.tys.adt(curr_id).clone();
        if prev_def.kind != curr_def.kind
            || prev_def.type_params != curr_def.type_params
            || prev_def.region_params != curr_def.region_params
            || prev_def.user_drop != curr_def.user_drop
            || prev_def.variants.len() != curr_def.variants.len()
        {
            self.adt_verified.remove(&prev_id);
            return None;
        }
        for (pv, cv) in prev_def.variants.iter().zip(curr_def.variants.iter()) {
            if pv.name != cv.name || pv.fields.len() != cv.fields.len() {
                self.adt_verified.remove(&prev_id);
                return None;
            }
            for (pf, cf) in pv.fields.iter().zip(cv.fields.iter()) {
                if pf.name != cf.name {
                    self.adt_verified.remove(&prev_id);
                    return None;
                }
                let Some(remapped_ty) = self.remap_ty(pf.ty) else {
                    self.adt_verified.remove(&prev_id);
                    return None;
                };
                if remapped_ty != cf.ty {
                    self.adt_verified.remove(&prev_id);
                    return None;
                }
            }
        }
        Some(curr_id)
    }

    fn remap_args(&mut self, prev_args: ArgsId) -> Option<ArgsId> {
        if let Some(&mapped) = self.args_map.get(&prev_args) {
            return Some(mapped);
        }
        let pa = self.prev.tys.args(prev_args);
        let mut tys = Vec::with_capacity(pa.tys.len());
        for &t in &pa.tys {
            tys.push(self.remap_ty(t)?);
        }
        let regions = pa.regions.clone();
        let curr_args = self
            .mono
            .module
            .tys
            .intern_args(GenericArgs { tys, regions });
        self.args_map.insert(prev_args, curr_args);
        Some(curr_args)
    }

    fn remap_ty(&mut self, prev_ty: TyId) -> Option<TyId> {
        if let Some(&mapped) = self.ty_map.get(&prev_ty) {
            return Some(mapped);
        }
        let kind = match self.prev.tys.kind(prev_ty) {
            TyKind::Unit => TyKind::Unit,
            TyKind::Bool => TyKind::Bool,
            TyKind::Never => TyKind::Never,
            TyKind::Int(i) => TyKind::Int(*i),
            TyKind::Float(f) => TyKind::Float(*f),
            TyKind::Adt { adt, args } => {
                let curr_adt = self.remap_adt_id(*adt)?;
                let curr_args = self.remap_args(*args)?;
                TyKind::Adt {
                    adt: curr_adt,
                    args: curr_args,
                }
            }
            TyKind::Tuple(ts) => {
                let mut out = Vec::with_capacity(ts.len());
                for &t in ts {
                    out.push(self.remap_ty(t)?);
                }
                TyKind::Tuple(out)
            }
            TyKind::Array { element, length } => TyKind::Array {
                element: self.remap_ty(*element)?,
                length: *length,
            },
            TyKind::Slice(elem) => TyKind::Slice(self.remap_ty(*elem)?),
            TyKind::Ref {
                region,
                mutability,
                pointee,
            } => TyKind::Ref {
                region: *region,
                mutability: *mutability,
                pointee: self.remap_ty(*pointee)?,
            },
            TyKind::Managed(inner) => TyKind::Managed(self.remap_ty(*inner)?),
            TyKind::Desc(inner) => TyKind::Desc(self.remap_ty(*inner)?),
            TyKind::ErrorUnion { ok, err } => TyKind::ErrorUnion {
                ok: self.remap_ty(*ok)?,
                err: self.remap_ty(*err)?,
            },
            TyKind::FnPtr { params, result } => {
                let mut out = Vec::with_capacity(params.len());
                for &p in params {
                    out.push(self.remap_ty(p)?);
                }
                TyKind::FnPtr {
                    params: out,
                    result: self.remap_ty(*result)?,
                }
            }
            // Bail out cleanly for closures/dyn so fresh Mono creates ClosureDef/DynImpl tables identically.
            TyKind::Closure { .. } | TyKind::Dyn { .. } | TyKind::OwnedDyn { .. } => return None,
            TyKind::Param(p) => TyKind::Param(*p),
            TyKind::Opaque(o) => TyKind::Opaque(o.clone()),
        };
        let curr_ty = self.mono.module.tys.intern(kind);
        self.mono.ensure_instance_pub(curr_ty);
        self.ty_map.insert(prev_ty, curr_ty);
        Some(curr_ty)
    }

    fn remap_fn_sig(&mut self, sig: &FnSig) -> Option<FnSig> {
        let mut params = Vec::with_capacity(sig.params.len());
        for &p in &sig.params {
            params.push(self.remap_ty(p)?);
        }
        let result = self.remap_ty(sig.result)?;
        let role = match sig.role {
            FnRole::Normal => FnRole::Normal,
            FnRole::Drop(adt) => FnRole::Drop(self.remap_adt_id(adt)?),
            FnRole::DynSlotImpl { .. } => return None,
        };
        Some(FnSig {
            params,
            result,
            type_params: sig.type_params,
            region_params: sig.region_params,
            role,
            resume_mode: sig.resume_mode,
            state_contracts: sig.state_contracts.clone(),
        })
    }

    fn remap_fn_id(&mut self, prev_id: FnId) -> Option<FnId> {
        let prev_fn = self.prev.functions.get(prev_id.index())?;
        let &curr_id = self.curr_root_by_name.get(&prev_fn.name)?;
        let remapped_sig = self.remap_fn_sig(&prev_fn.sig)?;
        if remapped_sig != self.mono.module.functions[curr_id.index()].sig {
            return None;
        }
        Some(curr_id)
    }

    fn remap_foreign_id(&mut self, prev_id: ForeignId) -> Option<ForeignId> {
        let prev_f = self.prev.foreigns.get(prev_id.index())?;
        let &curr_id = self.curr_foreign_by_sym.get(&prev_f.symbol)?;
        let curr_f = self.mono.module.foreigns.get(curr_id.index())?.clone();
        if prev_f.abi != curr_f.abi
            || prev_f.params.len() != curr_f.params.len()
            || prev_f.noescape_params != curr_f.noescape_params
            || prev_f.callback_role != curr_f.callback_role
            || prev_f.thread_async_driver_transfer != curr_f.thread_async_driver_transfer
        {
            return None;
        }
        for (&pp, &cp) in prev_f.params.iter().zip(curr_f.params.iter()) {
            if self.remap_ty(pp)? != cp {
                return None;
            }
        }
        if self.remap_ty(prev_f.result)? != curr_f.result {
            return None;
        }
        Some(curr_id)
    }

    fn remap_static_id(&mut self, prev_id: StaticId) -> Option<StaticId> {
        let bytes = self.prev.statics.get(prev_id.index())?.clone();
        let curr_id = StaticId(self.mono.module.statics.len() as u32);
        self.mono.module.statics.push(bytes);
        Some(curr_id)
    }

    fn remap_field_ref(&mut self, f: FieldRef) -> Option<FieldRef> {
        Some(FieldRef {
            adt: self.remap_adt_id(f.adt)?,
            variant: f.variant,
            field: f.field,
            args: self.remap_args(f.args)?,
        })
    }

    fn remap_place(&mut self, p: &Place) -> Option<Place> {
        let mut projections = Vec::with_capacity(p.projections.len());
        for proj in &p.projections {
            let rp = match *proj {
                Projection::Field(fr) => Projection::Field(self.remap_field_ref(fr)?),
                Projection::TupleField(i) => Projection::TupleField(i),
                Projection::ErrorArm(ok) => Projection::ErrorArm(ok),
                Projection::Index(l) => Projection::Index(l),
                Projection::ConstIndex(i) => Projection::ConstIndex(i),
                Projection::Deref => Projection::Deref,
            };
            projections.push(rp);
        }
        Some(Place {
            root: p.root,
            projections,
        })
    }

    fn remap_operand(&mut self, op: &Operand) -> Option<Operand> {
        match op {
            Operand::Copy(p) => Some(Operand::Copy(self.remap_place(p)?)),
            Operand::Move(p) => Some(Operand::Move(self.remap_place(p)?)),
            Operand::Const(c) => Some(Operand::Const(*c)),
            Operand::Pooled(_) => None,
        }
    }

    fn remap_rvalue(&mut self, rv: &Rvalue) -> Option<Rvalue> {
        Some(match rv {
            Rvalue::Use(op) => Rvalue::Use(self.remap_operand(op)?),
            Rvalue::Ref {
                region,
                mutability,
                place,
            } => Rvalue::Ref {
                region: *region,
                mutability: *mutability,
                place: self.remap_place(place)?,
            },
            Rvalue::SliceRef {
                region,
                mutability,
                place,
                start,
                end,
            } => Rvalue::SliceRef {
                region: *region,
                mutability: *mutability,
                place: self.remap_place(place)?,
                start: match start {
                    Some(s) => Some(self.remap_operand(s)?),
                    None => None,
                },
                end: match end {
                    Some(e) => Some(self.remap_operand(e)?),
                    None => None,
                },
            },
            Rvalue::Describe { place } => Rvalue::Describe {
                place: self.remap_place(place)?,
            },
            Rvalue::Aggregate { kind, operands } => {
                let rkind = match kind {
                    AggregateKind::Tuple => AggregateKind::Tuple,
                    AggregateKind::Adt { adt, variant, args } => AggregateKind::Adt {
                        adt: self.remap_adt_id(*adt)?,
                        variant: *variant,
                        args: self.remap_args(*args)?,
                    },
                    AggregateKind::Array { element } => AggregateKind::Array {
                        element: self.remap_ty(*element)?,
                    },
                    AggregateKind::Closure { .. } => return None,
                };
                let mut rops = Vec::with_capacity(operands.len());
                for o in operands {
                    rops.push(self.remap_operand(o)?);
                }
                Rvalue::Aggregate {
                    kind: rkind,
                    operands: rops,
                }
            }
            Rvalue::ErrorUnion { ok, value, ty } => Rvalue::ErrorUnion {
                ok: *ok,
                value: self.remap_operand(value)?,
                ty: self.remap_ty(*ty)?,
            },
            Rvalue::Unary { op, operand } => Rvalue::Unary {
                op: *op,
                operand: self.remap_operand(operand)?,
            },
            Rvalue::Binary { op, lhs, rhs } => Rvalue::Binary {
                op: *op,
                lhs: self.remap_operand(lhs)?,
                rhs: self.remap_operand(rhs)?,
            },
            Rvalue::Convert { operand, to } => Rvalue::Convert {
                operand: self.remap_operand(operand)?,
                to: self.remap_ty(*to)?,
            },
            Rvalue::Discriminant(p) => Rvalue::Discriminant(self.remap_place(p)?),
            Rvalue::ErrorArm(p) => Rvalue::ErrorArm(self.remap_place(p)?),
            Rvalue::Len(p) => Rvalue::Len(self.remap_place(p)?),
            Rvalue::FnRef(fid) => Rvalue::FnRef(self.remap_fn_id(*fid)?),
            Rvalue::ForeignFnRef(fid) => Rvalue::ForeignFnRef(self.remap_foreign_id(*fid)?),
            Rvalue::StaticUtf(sid) => Rvalue::StaticUtf(self.remap_static_id(*sid)?),
        })
    }

    fn remap_body(&mut self, body: &Body) -> Option<Body> {
        let mut locals = Vec::with_capacity(body.locals.len());
        for l in &body.locals {
            locals.push(LocalDecl {
                ty: self.remap_ty(l.ty)?,
                name: l.name.clone(),
            });
        }
        let result = self.remap_ty(body.result)?;
        let mut drop_flags = Vec::with_capacity(body.drop_flags.len());
        for df in &body.drop_flags {
            drop_flags.push(DropFlag {
                flag: df.flag,
                place: self.remap_place(&df.place)?,
            });
        }
        let mut blocks = Vec::with_capacity(body.blocks.len());
        for blk in &body.blocks {
            let mut instructions = Vec::with_capacity(blk.instructions.len());
            for inst in &blk.instructions {
                let kind = match &inst.kind {
                    InstructionKind::Assign { dest, value } => InstructionKind::Assign {
                        dest: self.remap_place(dest)?,
                        value: self.remap_rvalue(value)?,
                    },
                    InstructionKind::Drop { place } => InstructionKind::Drop {
                        place: self.remap_place(place)?,
                    },
                    InstructionKind::Replace { place, value, old } => InstructionKind::Replace {
                        place: self.remap_place(place)?,
                        value: self.remap_operand(value)?,
                        old: *old,
                    },
                    InstructionKind::StorageLive(l) => InstructionKind::StorageLive(*l),
                    InstructionKind::StorageDead(l) => InstructionKind::StorageDead(*l),
                    InstructionKind::ScopeEnter(s) => InstructionKind::ScopeEnter(*s),
                    InstructionKind::ScopeExit { scope, exit } => InstructionKind::ScopeExit {
                        scope: *scope,
                        exit: *exit,
                    },
                    InstructionKind::RegisterDefer { scope, closure } => {
                        InstructionKind::RegisterDefer {
                            scope: *scope,
                            closure: self.remap_operand(closure)?,
                        }
                    }
                };
                instructions.push(Instruction {
                    kind,
                    span: inst.span,
                });
            }
            let terminator = match &blk.terminator {
                Terminator::Goto { target } => Terminator::Goto { target: *target },
                Terminator::Switch {
                    discr,
                    arms,
                    otherwise,
                } => Terminator::Switch {
                    discr: self.remap_operand(discr)?,
                    arms: arms.clone(),
                    otherwise: *otherwise,
                },
                Terminator::Return { value } => Terminator::Return {
                    value: self.remap_operand(value)?,
                },
                Terminator::Call {
                    callee,
                    args,
                    dest,
                    target,
                    cancel,
                } => {
                    let rcallee = match callee {
                        Callee::Direct(fid) => Callee::Direct(self.remap_fn_id(*fid)?),
                        Callee::Indirect(op) => Callee::Indirect(self.remap_operand(op)?),
                        Callee::Closure { closure, mode } => Callee::Closure {
                            closure: self.remap_place(closure)?,
                            mode: *mode,
                        },
                        Callee::DynSlot { object, slot } => Callee::DynSlot {
                            object: self.remap_operand(object)?,
                            slot: *slot,
                        },
                        Callee::Foreign(fid) => Callee::Foreign(self.remap_foreign_id(*fid)?),
                        Callee::Intrinsic(op) => Callee::Intrinsic(*op),
                    };
                    let mut rargs = Vec::with_capacity(args.len());
                    for a in args {
                        rargs.push(self.remap_operand(a)?);
                    }
                    Terminator::Call {
                        callee: rcallee,
                        args: rargs,
                        dest: self.remap_place(dest)?,
                        target: *target,
                        cancel: *cancel,
                    }
                }
                Terminator::Assert {
                    cond,
                    expected,
                    target,
                    trap,
                } => Terminator::Assert {
                    cond: self.remap_operand(cond)?,
                    expected: *expected,
                    target: *target,
                    trap: *trap,
                },
                Terminator::Unreachable => Terminator::Unreachable,
                Terminator::Trap(t) => Terminator::Trap(*t),
            };
            blocks.push(Block {
                instructions,
                terminator,
                terminator_span: blk.terminator_span,
            });
        }
        Some(Body {
            param_count: body.param_count,
            locals,
            captures: Vec::new(),
            closure: None,
            result,
            param_env: body.param_env.clone(),
            drop_flags,
            blocks,
        })
    }

    fn remap_source_map(
        &mut self,
        prev_smap: &FunctionSourceMap,
        curr_fn_id: FnId,
    ) -> Option<FunctionSourceMap> {
        let mut relations = Vec::with_capacity(prev_smap.relations.len());
        for rel in &prev_smap.relations {
            let r = match rel {
                SourceRelation::Binding { node, local } => SourceRelation::Binding {
                    node: *node,
                    local: *local,
                },
                SourceRelation::FieldSelect { node, field, at } => SourceRelation::FieldSelect {
                    node: *node,
                    field: self.remap_field_ref(*field)?,
                    at: *at,
                },
                SourceRelation::Evaluation { node, ordinal, at } => SourceRelation::Evaluation {
                    node: *node,
                    ordinal: *ordinal,
                    at: *at,
                },
                SourceRelation::ResourceOp { node, at } => SourceRelation::ResourceOp {
                    node: *node,
                    at: *at,
                },
            };
            relations.push(r);
        }
        Some(FunctionSourceMap {
            function: curr_fn_id,
            declaration: prev_smap.declaration,
            relations,
            spans: prev_smap.spans.clone(),
        })
    }

    fn remap_fn_cert(
        &mut self,
        prev_cert: &FunctionCertificate,
        curr_fn_id: FnId,
    ) -> Option<FunctionCertificate> {
        let mut invariants = Vec::with_capacity(prev_cert.invariants.len());
        for inv in &prev_cert.invariants {
            let mut init = Vec::with_capacity(inv.init.len());
            for pi in &inv.init {
                init.push(PlaceInit {
                    place: self.remap_place(&pi.place)?,
                    state: pi.state,
                });
            }
            let mut variants = Vec::with_capacity(inv.variants.len());
            for vc in &inv.variants {
                variants.push(VariantClaim {
                    place: self.remap_place(&vc.place)?,
                    variant: vc.variant,
                });
            }
            let mut tags = Vec::with_capacity(inv.tags.len());
            for tc in &inv.tags {
                tags.push(TagClaim {
                    local: tc.local,
                    place: self.remap_place(&tc.place)?,
                });
            }
            let mut flags = Vec::with_capacity(inv.flags.len());
            for fc in &inv.flags {
                flags.push(FlagClaim {
                    flag: fc.flag,
                    place: self.remap_place(&fc.place)?,
                });
            }
            invariants.push(BlockInvariant {
                block: inv.block,
                init,
                variants,
                tags,
                flags,
                loans: inv.loans.clone(),
            });
        }
        Some(FunctionCertificate {
            function: curr_fn_id,
            invariants,
        })
    }
}

/// Helper used by [`crate::lower_program_incremental`] to pre-populate non-generic
/// root functions and ADTs in `mono` so reused bodies can resolve callees & ADTs.
pub(crate) struct PreRegistered {
    pub(crate) root_fn_ids: Vec<Option<FnId>>,
    pub(crate) root_by_name: HashMap<String, FnId>,
    pub(crate) adt_by_name: HashMap<String, AdtId>,
    pub(crate) foreign_by_sym: HashMap<String, ForeignId>,
    pub(crate) roots: Vec<(String, FnId)>,
}

pub(crate) fn pre_register_non_generic_items(
    mono: &mut Mono<'_, '_, '_>,
    is_ct_type: &[bool],
) -> PreRegistered {
    let mut adt_by_name = HashMap::new();
    for i in 0..mono.items.adts.len() {
        let decl = &mono.items.adts[i];
        if decl.generics.is_empty()
            && let Some(id) = mono.adt_id_pub(crate::ty::AdtRef(i as u32), Span::default())
        {
            adt_by_name.insert(mono.module.tys.adt(id).name.clone(), id);
        }
    }
    let mut foreign_by_sym = HashMap::new();
    for (i, f) in mono.module.foreigns.iter().enumerate() {
        foreign_by_sym.insert(f.symbol.clone(), ForeignId(i as u32));
    }
    let mut root_fn_ids = vec![None; mono.items.fns.len()];
    let mut root_by_name = HashMap::new();
    let mut roots = Vec::new();
    for i in 0..mono.items.fns.len() {
        let is_non_generic = mono.items.fns[i].generics.is_empty();
        let ct_only = is_ct_type.get(i).copied().unwrap_or(false);
        let span = mono.items.fns[i].ast.span;
        let name = mono.items.fns[i].name.clone();
        let role = mono.items.fns[i].role;
        if is_non_generic
            && !ct_only
            && let Some(id) = mono.register_root_without_queue(FnRef(i as u32), span)
        {
            root_fn_ids[i] = Some(id);
            root_by_name.insert(name.clone(), id);
            if role == FnRoleSpec::Normal {
                roots.push((name, id));
            }
        }
    }
    PreRegistered {
        root_fn_ids,
        root_by_name,
        adt_by_name,
        foreign_by_sym,
        roots,
    }
}

#[allow(dead_code)]
fn _keep_imports(_: (OpLocation, Function)) {}
