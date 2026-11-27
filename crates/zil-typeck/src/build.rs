//! mono + build: instantiate generic functions on demand and lower THIR to
//! body IR.
//!
//! The builder follows the rustc MIR builder: an expression is lowered as a
//! place, an operand, an rvalue, or "into" a destination place. User
//! bindings get `StorageLive`/`StorageDead` at their lexical scope;
//! compiler temporaries carry no storage markers (their liveness is derived
//! from uses by the kernel). Matches are compiled to decision trees whose
//! exhaustive tag switches send the remaining edge to `Unreachable`; the
//! kernel proves that edge infeasible.

use crate::infer::{POINTER_BITS, compute_suspend_flags, resolve_closure_for_suspend};
use crate::items::{FnRoleSpec, Items};
use crate::literal;
use crate::thir::*;
use crate::ty::{AdtRef, ClosureRef, OpaqueKind, TraitRef, Ty, TyData, Types};
use crate::{ConstEvaluator, CtValue, diag};
use std::collections::{HashMap, HashSet, VecDeque};
use zil_base::{Diagnostic, NodeId, Span};
use zil_ir::body::{
    AdtDef, AdtId, AdtKind, AggregateKind, ArgsId, Block, BlockId, Body, Callee, ClosureDef,
    ClosureId, ConstValue, DynDef, DynId, DynImpl, DynSlotSig, FieldDef, FieldIdx, FieldRef,
    FloatTy, FnId, FnRole, FnSig, ForeignAbi, ForeignDecl, ForeignId, Function, FunctionSourceMap,
    GenericArgs, Instruction, InstructionKind, IntTy, IntrinsicOp, Local, LocalDecl, Module,
    Mutability, OpLocation, OpaqueTy, Operand, ParamEnv, Place, Projection, Region, ResumeMode,
    Rvalue, SourceMap, SourceRelation, StaticId, Terminator, TyId, TyKind, ValueClass, VariantDef,
    VariantIdx,
};

/// Work limits of one lowering request.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub instances: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Limits { instances: 4096 }
    }
}

/// Computes the minimum `FnSig.region_params` count needed to declare every
/// `Region::Param(idx)` appearing in `params` or `result` (which can arise when
/// a generic function is monomorphized with reference-bearing type arguments).
fn max_region_param_count(tys: &zil_ir::body::TyTable, params: &[TyId], result: TyId) -> u32 {
    fn walk_region(r: Region, out: &mut u32) {
        if let Region::Param(idx) = r {
            *out = (*out).max(idx + 1);
        }
    }
    fn walk_ty(tys: &zil_ir::body::TyTable, ty: TyId, visited: &mut HashSet<TyId>, out: &mut u32) {
        if !visited.insert(ty) {
            return;
        }
        match tys.kind(ty) {
            TyKind::Ref {
                region, pointee, ..
            } => {
                walk_region(*region, out);
                walk_ty(tys, *pointee, visited, out);
            }
            TyKind::Tuple(elems) => {
                for &e in elems {
                    walk_ty(tys, e, visited, out);
                }
            }
            TyKind::Array { element, .. } | TyKind::Slice(element) => {
                walk_ty(tys, *element, visited, out);
            }
            TyKind::Adt { args, .. } => {
                let a = tys.args(*args);
                for &r in &a.regions {
                    walk_region(r, out);
                }
                for &t in &a.tys {
                    walk_ty(tys, t, visited, out);
                }
            }
            TyKind::FnPtr { params, result } => {
                for &p in params {
                    walk_ty(tys, p, visited, out);
                }
                walk_ty(tys, *result, visited, out);
            }
            TyKind::Closure { def, args } => {
                let a = tys.args(*args);
                for &r in &a.regions {
                    walk_region(r, out);
                }
                for &t in &a.tys {
                    walk_ty(tys, t, visited, out);
                }
                if let Some(cdef) = tys.closure(*def) {
                    for &c in &cdef.captures {
                        walk_ty(tys, c, visited, out);
                    }
                    for &p in &cdef.params {
                        walk_ty(tys, p, visited, out);
                    }
                    walk_ty(tys, cdef.result, visited, out);
                }
            }
            TyKind::ErrorUnion { ok, err } => {
                walk_ty(tys, *ok, visited, out);
                walk_ty(tys, *err, visited, out);
            }
            TyKind::Managed(inner) | TyKind::Desc(inner) => {
                walk_ty(tys, *inner, visited, out);
            }
            TyKind::Dyn { regions, .. } => {
                for r in regions {
                    if let Region::Param(i) = r {
                        *out = (*out).max(*i + 1);
                    }
                }
            }
            TyKind::OwnedDyn {
                regions, context, ..
            } => {
                for r in regions {
                    if let Region::Param(i) = r {
                        *out = (*out).max(*i + 1);
                    }
                }
                if let Some(c) = context {
                    walk_ty(tys, *c, visited, out);
                }
            }
            TyKind::Unit
            | TyKind::Bool
            | TyKind::Int(_)
            | TyKind::Float(_)
            | TyKind::Never
            | TyKind::Param(_)
            | TyKind::Opaque(_) => {}
        }
    }
    let mut visited = HashSet::new();
    let mut out = 0u32;
    for &p in params {
        walk_ty(tys, p, &mut visited, &mut out);
    }
    walk_ty(tys, result, &mut visited, &mut out);
    out
}

pub struct Mono<'c, 'a, 'e> {
    items: &'c Items<'a>,
    types: &'c mut Types,
    thir: &'c [Option<ThirFn>],
    diags: &'c mut Vec<Diagnostic>,
    limits: Limits,
    ct: Option<&'e mut dyn ConstEvaluator>,
    is_ct_eval: bool,
    pub module: Module,
    pub source_map: SourceMap,
    instances: HashMap<(FnRef, Vec<Ty>), FnId>,
    queue: VecDeque<(FnRef, Vec<Ty>, FnId)>,
    adts: HashMap<(AdtRef, Vec<u32>), AdtId>,
    unions: HashMap<Vec<TyId>, AdtId>,
    dyns: HashMap<DynKey, DynId>,
    closures: HashMap<(FnRef, Vec<Ty>, ClosureRef), ClosureId>,
    closure_queue: VecDeque<(FnRef, Vec<Ty>, ClosureRef, ClosureId)>,
    closure_fns: HashMap<(FnRef, Vec<Ty>, ClosureRef), FnId>,
    closure_fn_queue: VecDeque<(FnRef, Vec<Ty>, ClosureRef, FnId)>,
    ensured: HashSet<TyId>,
    fn_susp: Vec<bool>,
    cl_susp: Vec<Vec<bool>>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct DynKey {
    trait_ref: TraitRef,
    trait_args: Vec<Ty>,
    assoc_bindings: Vec<(String, Ty)>,
}

impl<'c, 'a, 'e> Mono<'c, 'a, 'e> {
    pub fn new(
        items: &'c Items<'a>,
        types: &'c mut Types,
        thir: &'c [Option<ThirFn>],
        diags: &'c mut Vec<Diagnostic>,
        limits: Limits,
        ct: Option<&'e mut dyn ConstEvaluator>,
    ) -> Self {
        let (fn_susp, cl_susp) = if thir.is_empty() {
            (Vec::new(), Vec::new())
        } else {
            compute_suspend_flags(items, types, thir)
        };
        Mono {
            items,
            types,
            thir,
            diags,
            limits,
            ct,
            is_ct_eval: false,
            module: Module::default(),
            source_map: SourceMap::default(),
            instances: HashMap::new(),
            queue: VecDeque::new(),
            adts: HashMap::new(),
            unions: HashMap::new(),
            dyns: HashMap::new(),
            closures: HashMap::new(),
            closure_queue: VecDeque::new(),
            closure_fns: HashMap::new(),
            closure_fn_queue: VecDeque::new(),
            ensured: HashSet::new(),
            fn_susp,
            cl_susp,
        }
    }

    pub fn ty_data(&self, ty: Ty) -> &TyData {
        self.types.data(ty)
    }

    /// Lowers all `extern "C"` declarations into `module.foreigns`.
    pub fn lower_foreigns(&mut self) {
        let foreigns = self.items.foreigns.clone();
        for f in foreigns {
            let mut params = Vec::new();
            for p in f.params {
                if let Some(t) = self.lower_ty(p, f.span) {
                    params.push(t);
                }
            }
            let result = self
                .lower_ty(f.result, f.span)
                .unwrap_or_else(|| self.module.tys.intern(TyKind::Unit));
            self.module.foreigns.push(ForeignDecl {
                symbol: f.symbol,
                abi: ForeignAbi::C,
                params,
                noescape_params: f.noescape_params,
                callback_role: f.callback_role,
                thread_async_driver_transfer: f.thread_async_driver_transfer,
                result,
            });
        }
    }

    /// Requests the instance of `f` at closed `args`.
    pub fn instance(&mut self, f: FnRef, args: Vec<Ty>, span: Span) -> Option<FnId> {
        if let Some(id) = self.instances.get(&(f, args.clone())) {
            return Some(*id);
        }
        if self.module.functions.len() >= self.limits.instances {
            self.diags
                .push(diag::limit("too many function instances", span));
            return None;
        }
        let decl = self.items.func(f);
        let mut params = Vec::new();
        for p in &decl.params {
            let t = self.types.subst(p.ty, &args);
            params.push(self.lower_ty(t, span)?);
        }
        let r = self.types.subst(decl.result, &args);
        let result = self.lower_ty(r, span)?;
        let role = match decl.role {
            FnRoleSpec::Normal => FnRole::Normal,
            FnRoleSpec::Drop(adt) => FnRole::Drop(self.adt_id(adt, span)?),
        };
        let resume_mode = decl.resume_mode;
        let state_contracts = decl.state_contracts.clone();
        let id = FnId::from_index(self.module.functions.len());
        let mut name = decl.name.clone();
        if !args.is_empty() {
            name.push_str(&format_mono_args_tag(self.types, self.items, &args));
        }
        let region_params = (decl.region_params.len() as u32).max(max_region_param_count(
            &self.module.tys,
            &params,
            result,
        ));
        self.module.functions.push(Function {
            name,
            sig: FnSig {
                params,
                result,
                type_params: 0,
                region_params,
                role,
                resume_mode,
                state_contracts,
            },
            body: Body {
                param_count: 0,
                locals: Vec::new(),
                captures: Vec::new(),
                closure: None,
                result,
                param_env: ParamEnv::default(),
                drop_flags: Vec::new(),
                blocks: Vec::new(),
            },
        });
        self.instances.insert((f, args.clone()), id);
        self.queue.push_back((f, args, id));
        Some(id)
    }

    /// Instantiates a dedicated `FnRole::DynSlotImpl` vtable entry for
    /// `(dyn_id, concrete_ty, slot)` whose declaration span remains anchored
    /// to the concrete `impl Trait for ConcreteType` method AST node.
    fn instance_dyn_slot(
        &mut self,
        f: FnRef,
        args: Vec<Ty>,
        dyn_id: DynId,
        concrete_ty: TyId,
        slot: u32,
        span: Span,
    ) -> Option<FnId> {
        if self.module.functions.len() >= self.limits.instances {
            self.diags
                .push(diag::limit("too many function instances", span));
            return None;
        }
        let decl = self.items.func(f);
        let mut params = Vec::new();
        for p in &decl.params {
            let t = self.types.subst(p.ty, &args);
            params.push(self.lower_ty(t, span)?);
        }
        let r = self.types.subst(decl.result, &args);
        let result = self.lower_ty(r, span)?;
        let id = FnId::from_index(self.module.functions.len());
        let name = format!("{}::dyn_slot#{slot}", decl.name);
        let region_params = (decl.region_params.len() as u32).max(max_region_param_count(
            &self.module.tys,
            &params,
            result,
        ));
        self.module.functions.push(Function {
            name,
            sig: FnSig {
                params,
                result,
                type_params: 0,
                region_params,
                role: FnRole::DynSlotImpl {
                    dyn_id,
                    concrete_ty,
                    slot,
                },
                resume_mode: ResumeMode::Local,
                state_contracts: Vec::new(),
            },
            body: Body {
                param_count: 0,
                locals: Vec::new(),
                captures: Vec::new(),
                closure: None,
                result,
                param_env: ParamEnv::default(),
                drop_flags: Vec::new(),
                blocks: Vec::new(),
            },
        });
        self.queue.push_back((f, args, id));
        Some(id)
    }

    /// Registers a closure `(owner, args, cid)` on demand and queues its body.
    fn ensure_closure(
        &mut self,
        owner: FnRef,
        args: Vec<Ty>,
        cid: ClosureRef,
        span: Span,
    ) -> Option<ClosureId> {
        let key = (owner, args.clone(), cid);
        if let Some(&id) = self.closures.get(&key) {
            return Some(id);
        }
        let thir = self.thir.get(owner.0 as usize).and_then(Option::as_ref)?;
        let c = thir.closures.get(cid.0 as usize)?;
        let mut capture_tys = Vec::with_capacity(c.capture_tys.len());
        for t in &c.capture_tys {
            let t = self.types.subst(*t, &args);
            capture_tys.push(self.lower_ty(t, span)?);
        }
        let mut param_tys = Vec::with_capacity(c.param_tys.len());
        for t in &c.param_tys {
            let t = self.types.subst(*t, &args);
            param_tys.push(self.lower_ty(t, span)?);
        }
        let r = self.types.subst(c.result, &args);
        let result = self.lower_ty(r, span)?;
        let def_id = self.module.tys.add_closure(ClosureDef {
            kind: c.kind,
            type_params: 0,
            captures: capture_tys,
            params: param_tys,
            result,
        });
        self.closures.insert(key, def_id);
        self.closure_queue.push_back((owner, args, cid, def_id));
        Some(def_id)
    }

    fn ensure_closure_fn(
        &mut self,
        owner: FnRef,
        args: Vec<Ty>,
        cid: ClosureRef,
        span: Span,
    ) -> Option<FnId> {
        let key = (owner, args.clone(), cid);
        if let Some(&id) = self.closure_fns.get(&key) {
            return Some(id);
        }
        if self.module.functions.len() >= self.limits.instances {
            self.diags
                .push(diag::limit("too many function instances", span));
            return None;
        }
        let thir = self.thir.get(owner.0 as usize).and_then(Option::as_ref)?;
        let c = thir.closures.get(cid.0 as usize)?;
        let mut param_tys = Vec::with_capacity(c.param_tys.len());
        for t in &c.param_tys {
            let t = self.types.subst(*t, &args);
            param_tys.push(self.lower_ty(t, span)?);
        }
        let r = self.types.subst(c.result, &args);
        let result = self.lower_ty(r, span)?;
        let region_params = (self.items.func(owner).region_params.len() as u32)
            .max(max_region_param_count(&self.module.tys, &param_tys, result));
        let sig = FnSig {
            params: param_tys,
            result,
            type_params: 0,
            region_params,
            role: FnRole::Normal,
            resume_mode: ResumeMode::Local,
            state_contracts: Vec::new(),
        };
        let owner_name = self.items.func(owner).name.clone();
        let id = FnId::from_index(self.module.functions.len());
        self.module.functions.push(Function {
            name: format!("{owner_name}::$fn_closure#{}", cid.0),
            sig,
            body: Body {
                param_count: 0,
                locals: Vec::new(),
                captures: Vec::new(),
                closure: None,
                result,
                param_env: ParamEnv::default(),
                drop_flags: Vec::new(),
                blocks: Vec::new(),
            },
        });
        self.closure_fns.insert(key, id);
        self.closure_fn_queue.push_back((owner, args, cid, id));
        Some(id)
    }

    /// Builds every requested function and closure instance.
    pub fn run(&mut self) {
        while !self.queue.is_empty()
            || !self.closure_queue.is_empty()
            || !self.closure_fn_queue.is_empty()
        {
            while let Some((f, args, id)) = self.queue.pop_front() {
                let Some(thir) = self.thir.get(f.0 as usize).and_then(Option::as_ref) else {
                    continue;
                };
                let sig = self.module.functions[id.index()].sig.clone();
                let mut b = Builder::new(self, f, thir, args, sig);
                b.function(thir);
                let (body, eval_rels) = b.finish();
                let decl_span = self.items.func(f).ast.span;
                self.source_map
                    .functions
                    .push(build_fn_source_map(id, decl_span, &body, eval_rels));
                self.module.functions[id.index()].body = body;
            }
            while let Some((owner, args, cid, def_id)) = self.closure_queue.pop_front() {
                let Some(thir) = self.thir.get(owner.0 as usize).and_then(Option::as_ref) else {
                    continue;
                };
                let Some(c) = thir.closures.get(cid.0 as usize) else {
                    continue;
                };
                let def = self
                    .module
                    .tys
                    .closure(def_id)
                    .cloned()
                    .expect("registered");
                let mut sig_tys = def.params.clone();
                sig_tys.extend_from_slice(&def.captures);
                let region_params = (self.items.func(owner).region_params.len() as u32).max(
                    max_region_param_count(&self.module.tys, &sig_tys, def.result),
                );
                let csig = FnSig {
                    params: def.params.clone(),
                    result: def.result,
                    type_params: 0,
                    region_params,
                    role: FnRole::Normal,
                    resume_mode: ResumeMode::Local,
                    state_contracts: Vec::new(),
                };
                let cthir = ThirFn {
                    locals: c.locals.clone(),
                    params: c.params.clone(),
                    body: c.body.clone(),
                    closures: Vec::new(),
                    opaque_return: None,
                };
                let mut cb = Builder::new(self, owner, &cthir, args, csig.clone());
                cb.captures = def.captures;
                cb.closure_id = Some(def_id);
                cb.function(&cthir);
                let (cbody, ceval_rels) = cb.finish();
                let owner_decl = self.items.func(owner);
                let owner_name = owner_decl.name.clone();
                let closure_fn_id = FnId::from_index(self.module.functions.len());
                self.source_map.functions.push(build_fn_source_map(
                    closure_fn_id,
                    owner_decl.ast.span,
                    &cbody,
                    ceval_rels,
                ));
                self.module.functions.push(Function {
                    name: format!("{owner_name}::$closure#{}", cid.0),
                    sig: csig,
                    body: cbody,
                });
            }
            while let Some((owner, args, cid, fn_id)) = self.closure_fn_queue.pop_front() {
                let Some(thir) = self.thir.get(owner.0 as usize).and_then(Option::as_ref) else {
                    continue;
                };
                let Some(c) = thir.closures.get(cid.0 as usize) else {
                    continue;
                };
                let sig = self.module.functions[fn_id.index()].sig.clone();
                let cthir = ThirFn {
                    locals: c.locals.clone(),
                    params: c.params.clone(),
                    body: c.body.clone(),
                    closures: Vec::new(),
                    opaque_return: None,
                };
                let mut cb = Builder::new(self, owner, &cthir, args, sig);
                cb.function(&cthir);
                let (cbody, ceval_rels) = cb.finish();
                let decl_span = self.items.func(owner).ast.span;
                self.source_map
                    .functions
                    .push(build_fn_source_map(fn_id, decl_span, &cbody, ceval_rels));
                self.module.functions[fn_id.index()].body = cbody;
            }
        }
    }

    // ---- types -------------------------------------------------------------

    fn adt_id(&mut self, adt: AdtRef, span: Span) -> Option<AdtId> {
        self.adt_id_for(adt, &[], span)
    }

    fn adt_id_for(&mut self, adt: AdtRef, args: &[Ty], span: Span) -> Option<AdtId> {
        let decl = self.items.adt(adt);
        let has_const = args
            .iter()
            .any(|&t| matches!(self.types.data(t), TyData::ConstU32(_)));
        let mut const_key = Vec::new();
        let mut const_subst = Vec::new();
        if has_const {
            for (i, _) in decl.generics.iter().enumerate() {
                if let Some(&t) = args.get(i)
                    && let TyData::ConstU32(n) = *self.types.data(t)
                {
                    const_key.push(n);
                    const_subst.push(self.types.intern(TyData::ConstU32(n)));
                } else {
                    const_subst.push(self.types.intern(TyData::Param(i as u32)));
                }
            }
        }
        let key = (adt, const_key.clone());
        if let Some(id) = self.adts.get(&key) {
            return Some(*id);
        }
        let name = if const_key.is_empty() {
            decl.name.clone()
        } else {
            let suffix: Vec<String> = const_key.iter().map(|n| n.to_string()).collect();
            format!("{}${}", decl.name, suffix.join("_"))
        };
        let id = self.module.tys.add_adt(AdtDef {
            name: name.clone(),
            kind: decl.kind,
            type_params: decl.generics.len() as u32,
            region_params: 0,
            variants: Vec::new(),
            user_drop: decl.user_drop,
        });
        self.adts.insert(key, id);
        let mut variants = Vec::new();
        for v in &decl.variants {
            let mut fields = Vec::new();
            for f in &v.fields {
                let fty = if has_const {
                    self.types.subst(f.ty, &const_subst)
                } else {
                    f.ty
                };
                fields.push(FieldDef {
                    name: f.name.clone(),
                    ty: self.lower_ty(fty, span)?,
                });
            }
            variants.push(VariantDef {
                name: v.name.clone(),
                fields,
            });
        }
        self.module.tys.set_adt(
            id,
            AdtDef {
                name,
                kind: decl.kind,
                type_params: decl.generics.len() as u32,
                region_params: decl.region_params,
                variants,
                user_drop: decl.user_drop,
            },
        );
        Some(id)
    }

    fn union_adt(&mut self, members: &[Ty], span: Span) -> Option<(AdtId, Vec<TyId>)> {
        let mut lowered = Vec::with_capacity(members.len());
        for &m in members {
            lowered.push(self.lower_ty(m, span)?);
        }
        let mut seen = HashSet::with_capacity(lowered.len());
        for &t in &lowered {
            if !seen.insert(t) {
                self.diags.push(diag::error(
                    diag::TYPE_MISMATCH,
                    span,
                    "anonymous union contains duplicate member types",
                ));
                return None;
            }
        }
        if lowered.len() < 2 {
            self.diags.push(diag::error(
                diag::TYPE_MISMATCH,
                span,
                "anonymous union requires at least two distinct member types",
            ));
            return None;
        }
        if let Some(&id) = self.unions.get(&lowered) {
            return Some((id, lowered));
        }
        let suffix: Vec<String> = lowered.iter().map(|t| t.0.to_string()).collect();
        let name = format!("__zil_union_{}", suffix.join("_"));
        let variants = lowered
            .iter()
            .enumerate()
            .map(|(i, &ty)| VariantDef {
                name: format!("_{i}"),
                fields: vec![FieldDef {
                    name: "0".to_string(),
                    ty,
                }],
            })
            .collect();
        let id = self.module.tys.add_adt(AdtDef {
            name,
            kind: AdtKind::Enum,
            type_params: 0,
            region_params: 0,
            variants,
            user_drop: false,
        });
        self.unions.insert(lowered.clone(), id);
        Some((id, lowered))
    }

    fn args_id(&mut self, args: &[Ty], span: Span) -> Option<ArgsId> {
        self.args_id_with_regions(args, &[], span)
    }

    fn args_id_with_regions(
        &mut self,
        args: &[Ty],
        regions: &[Option<u32>],
        span: Span,
    ) -> Option<ArgsId> {
        let mut tys = Vec::new();
        for a in args {
            tys.push(self.lower_ty(*a, span)?);
        }
        let regions = regions
            .iter()
            .map(|r| r.map_or(Region::Static, Region::Param))
            .collect();
        Some(self.module.tys.intern_args(GenericArgs { tys, regions }))
    }

    fn dyn_id(
        &mut self,
        tr: TraitRef,
        trait_args: &[Ty],
        assoc_bindings: &[(String, Ty)],
        span: Span,
    ) -> Option<DynId> {
        let norm_args: Vec<Ty> = trait_args
            .iter()
            .map(|t| self.items.normalize_ty(*t, self.types))
            .collect();
        let mut norm_assoc: Vec<(String, Ty)> = assoc_bindings
            .iter()
            .map(|(n, t)| (n.clone(), self.items.normalize_ty(*t, self.types)))
            .collect();
        norm_assoc.sort_by(|a, b| a.0.cmp(&b.0));
        let key = DynKey {
            trait_ref: tr,
            trait_args: norm_args.clone(),
            assoc_bindings: norm_assoc.clone(),
        };
        if let Some(&id) = self.dyns.get(&key) {
            return Some(id);
        }
        if let Err(msg) = self
            .items
            .check_dyn_object_safe(tr, &norm_args, &norm_assoc, self.types)
        {
            self.diags.push(diag::error(diag::GENERIC_BOUND, span, msg));
            return None;
        }
        let decl = self.items.trait_decl(tr);
        let name = decl.name.clone();
        let tr_region_count = decl.region_params.len();
        let methods = decl.methods.clone();
        let self_param = self.types.intern(TyData::SelfParam);
        let mut slots = Vec::with_capacity(methods.len());
        for m in methods {
            let receiver = m.params.first().and_then(|p| {
                if let TyData::Ref { mutable, .. } = *self.types.data(*p) {
                    Some(if mutable {
                        Mutability::Exclusive
                    } else {
                        Mutability::Shared
                    })
                } else {
                    None
                }
            });
            let recv_region = m.params.first().and_then(|p| {
                if let TyData::Ref { region, .. } = *self.types.data(*p) {
                    region
                } else {
                    None
                }
            });
            let slot_region_map: Vec<Option<u32>> = (0..tr_region_count)
                .map(|i| {
                    if recv_region == Some(i as u32) {
                        None
                    } else {
                        Some(i as u32)
                    }
                })
                .collect();
            let mut params = Vec::new();
            for p in m.params.iter().skip(1) {
                let pt = self.types.subst(*p, &norm_args);
                let pt = self.types.subst_self_assoc(pt, self_param, tr, &norm_assoc);
                let pt = self.types.subst_regions(pt, &slot_region_map);
                params.push(self.lower_ty(pt, span)?);
            }
            let rt = self.types.subst(m.result, &norm_args);
            let rt = self.types.subst_self_assoc(rt, self_param, tr, &norm_assoc);
            let rt = self.types.subst_regions(rt, &slot_region_map);
            let result = self.lower_ty(rt, span)?;
            slots.push(DynSlotSig {
                name: m.name,
                receiver,
                params,
                result,
            });
        }
        let id = self.module.tys.add_dyn(DynDef { name, slots });
        self.dyns.insert(key, id);
        Some(id)
    }

    fn ensure_dyn_impl(
        &mut self,
        tr: TraitRef,
        trait_args: &[Ty],
        assoc_bindings: &[(String, Ty)],
        concrete: Ty,
        span: Span,
    ) {
        let norm_args: Vec<Ty> = trait_args
            .iter()
            .map(|t| self.items.normalize_ty(*t, self.types))
            .collect();
        let mut norm_assoc: Vec<(String, Ty)> = assoc_bindings
            .iter()
            .map(|(n, t)| (n.clone(), self.items.normalize_ty(*t, self.types)))
            .collect();
        norm_assoc.sort_by(|a, b| a.0.cmp(&b.0));
        let Some(dyn_id) = self.dyn_id(tr, &norm_args, &norm_assoc, span) else {
            return;
        };
        let Some(concrete_ty) = self.lower_ty(concrete, span) else {
            return;
        };
        if self
            .module
            .dyn_impls
            .iter()
            .any(|d| d.dyn_id == dyn_id && d.concrete_ty == concrete_ty)
        {
            return;
        }
        let slot_names: Vec<String> = self
            .module
            .tys
            .dyn_def(dyn_id)
            .slots
            .iter()
            .map(|s| s.name.clone())
            .collect();
        let impls = self.items.trait_impls.clone();
        let mut matched = None;
        for imp in &impls {
            if imp.trait_ref != tr {
                continue;
            }
            let Some(impl_args) = self.items.match_impl(imp, concrete, self.types) else {
                continue;
            };
            let imp_targs: Vec<Ty> = imp
                .trait_args
                .iter()
                .map(|t| {
                    let s = self.types.subst(*t, &impl_args);
                    self.items.normalize_ty(s, self.types)
                })
                .collect();
            if imp_targs != norm_args {
                continue;
            }
            let assoc_ok = norm_assoc.iter().all(|(aname, req_ty)| {
                if let Some((_, rhs)) = imp.assoc_types.iter().find(|(n, _)| n == aname) {
                    let s = self.types.subst(*rhs, &impl_args);
                    self.items.normalize_ty(s, self.types) == *req_ty
                } else {
                    false
                }
            });
            if !assoc_ok {
                continue;
            }
            matched = Some((imp.methods.clone(), impl_args));
            break;
        }
        let Some((imp_methods, impl_args)) = matched else {
            return;
        };
        let mut methods = Vec::with_capacity(slot_names.len());
        for (slot_idx, sname) in slot_names.iter().enumerate() {
            let Some((_, fn_ref)) = imp_methods.iter().find(|(n, _)| n == sname) else {
                return;
            };
            let Some(fn_id) = self.instance_dyn_slot(
                *fn_ref,
                impl_args.clone(),
                dyn_id,
                concrete_ty,
                slot_idx as u32,
                span,
            ) else {
                return;
            };
            methods.push(fn_id);
        }
        self.module.dyn_impls.push(DynImpl {
            dyn_id,
            concrete_ty,
            methods,
        });
    }

    /// Interns a typeck type. Declaration parameters map to `TyKind::Param`;
    /// instance bodies only ever pass closed types.
    pub fn lower_ty(&mut self, ty: Ty, span: Span) -> Option<TyId> {
        let ty = self.items.normalize_ty(ty, self.types);
        let kind = match self.types.data(ty).clone() {
            TyData::Unit => TyKind::Unit,
            TyData::Type => {
                if self.is_ct_eval {
                    TyKind::Int(IntTy::U32)
                } else {
                    self.diags
                        .push(diag::unsupported("runtime Type value", span));
                    return None;
                }
            }
            TyData::Bool => TyKind::Bool,
            TyData::Never => TyKind::Never,
            TyData::Int(i) => TyKind::Int(i),
            TyData::Float(f) => TyKind::Float(f),
            TyData::Param(i) => TyKind::Param(i),
            TyData::Tuple(items) => {
                let mut out = Vec::new();
                for i in items {
                    out.push(self.lower_ty(i, span)?);
                }
                TyKind::Tuple(out)
            }
            TyData::Array { element, length } => TyKind::Array {
                element: self.lower_ty(element, span)?,
                length,
            },
            TyData::Slice(element) => TyKind::Slice(self.lower_ty(element, span)?),
            TyData::Adt { adt, args, regions } => {
                let adt = self.adt_id_for(adt, &args, span)?;
                let args = self.args_id_with_regions(&args, &regions, span)?;
                TyKind::Adt { adt, args }
            }
            TyData::Ref {
                region,
                mutable,
                pointee,
            } => TyKind::Ref {
                region: region.map(Region::Param).unwrap_or(Region::Static),
                mutability: if mutable {
                    Mutability::Exclusive
                } else {
                    Mutability::Shared
                },
                pointee: self.lower_ty(pointee, span)?,
            },
            TyData::Managed(inner) => TyKind::Managed(self.lower_ty(inner, span)?),
            TyData::Desc(inner) => TyKind::Desc(self.lower_ty(inner, span)?),
            TyData::Opaque(OpaqueKind::ByteBuffer) => TyKind::Opaque(OpaqueTy::ByteBuffer),
            TyData::Opaque(OpaqueKind::ForeignHandle) => TyKind::Opaque(OpaqueTy::ForeignHandle),
            TyData::Opaque(OpaqueKind::OwnedStore(elem)) => TyKind::Opaque(OpaqueTy::OwnedStore {
                element: self.lower_ty(elem, span)?,
            }),
            TyData::Opaque(OpaqueKind::Mutex(payload)) => TyKind::Opaque(OpaqueTy::Mutex {
                payload: self.lower_ty(payload, span)?,
            }),
            TyData::Opaque(OpaqueKind::MutexGuard(payload)) => {
                TyKind::Opaque(OpaqueTy::MutexGuard {
                    payload: self.lower_ty(payload, span)?,
                    region: Region::Static,
                })
            }
            TyData::Opaque(OpaqueKind::Task { result, context }) => {
                let result = self.lower_ty(result, span)?;
                let context = match context {
                    Some(c) => Some(self.lower_ty(c, span)?),
                    None => None,
                };
                TyKind::Opaque(OpaqueTy::Task { result, context })
            }
            TyData::Opaque(OpaqueKind::Registration(environment)) => {
                TyKind::Opaque(OpaqueTy::Registration {
                    environment: self.lower_ty(environment, span)?,
                })
            }
            TyData::Opaque(OpaqueKind::AsyncRegistration(environment)) => {
                TyKind::Opaque(OpaqueTy::AsyncRegistration {
                    environment: self.lower_ty(environment, span)?,
                })
            }
            TyData::Opaque(OpaqueKind::ForeignCallback { params, result }) => {
                let mut ps = Vec::with_capacity(params.len());
                for p in params {
                    ps.push(self.lower_ty(p, span)?);
                }
                TyKind::Opaque(OpaqueTy::ForeignCallback {
                    params: ps,
                    result: self.lower_ty(result, span)?,
                })
            }
            TyData::Opaque(OpaqueKind::SyncCell(k)) => TyKind::Opaque(OpaqueTy::SyncCell(k)),
            TyData::Union(members) => {
                let (adt, _) = self.union_adt(&members, span)?;
                let args = self.args_id(&[], span)?;
                TyKind::Adt { adt, args }
            }
            TyData::ErrorUnion { ok, err } => TyKind::ErrorUnion {
                ok: self.lower_ty(ok, span)?,
                err: self.lower_ty(err, span)?,
            },
            TyData::FnPtr { params, result } => {
                let mut ps = Vec::with_capacity(params.len());
                for p in params {
                    ps.push(self.lower_ty(p, span)?);
                }
                TyKind::FnPtr {
                    params: ps,
                    result: self.lower_ty(result, span)?,
                }
            }
            TyData::Closure {
                owner,
                id,
                owner_args,
                ..
            } => {
                let def = self.ensure_closure(owner, owner_args, id, span)?;
                let args = self.args_id(&[], span)?;
                TyKind::Closure { def, args }
            }
            TyData::ImplCallable {
                owner, owner_args, ..
            } => {
                let cid = self
                    .thir
                    .get(owner.0 as usize)
                    .and_then(Option::as_ref)
                    .and_then(|t| t.opaque_return)?;
                let def = self.ensure_closure(owner, owner_args, cid, span)?;
                let args = self.args_id(&[], span)?;
                TyKind::Closure { def, args }
            }
            TyData::Dyn {
                trait_ref,
                trait_args,
                trait_regions,
                assoc_bindings,
            } => {
                let def = self.dyn_id(trait_ref, &trait_args, &assoc_bindings, span)?;
                let regions = trait_regions
                    .into_iter()
                    .map(|r| r.map_or(Region::Static, Region::Param))
                    .collect();
                TyKind::Dyn { def, regions }
            }
            TyData::OwnedDyn {
                trait_ref,
                trait_args,
                trait_regions,
                assoc_bindings,
                context,
            } => {
                let def = self.dyn_id(trait_ref, &trait_args, &assoc_bindings, span)?;
                let regions = trait_regions
                    .into_iter()
                    .map(|r| r.map_or(Region::Static, Region::Param))
                    .collect();
                let context = match context {
                    Some(c) => Some(self.lower_ty(c, span)?),
                    None => None,
                };
                TyKind::OwnedDyn {
                    def,
                    regions,
                    context,
                }
            }
            TyData::ConstU32(_) => TyKind::Unit,
            TyData::ArrayConst { .. }
            | TyData::ConstParam(_)
            | TyData::SelfParam
            | TyData::Assoc { .. }
            | TyData::Var(_)
            | TyData::Error => {
                self.diags.push(diag::error(
                    diag::TYPE_INFERENCE,
                    span,
                    "internal: an unresolved type reached lowering",
                ));
                return None;
            }
        };
        let id = self.module.tys.intern(kind);
        self.ensure_instance(id);
        Some(id)
    }

    /// Interns the substituted field types of every nominal instance
    /// reachable from `ty`, so that consumers can look them up.
    fn ensure_instance(&mut self, ty: TyId) {
        if !self.ensured.insert(ty) {
            return;
        }
        match self.module.tys.kind(ty).clone() {
            TyKind::Adt { adt, args } => {
                let declared: Vec<TyId> = self
                    .module
                    .tys
                    .adt(adt)
                    .variants
                    .iter()
                    .flat_map(|v| v.fields.iter().map(|f| f.ty))
                    .collect();
                for d in declared {
                    let t = self.module.tys.subst(d, args);
                    self.ensure_instance(t);
                }
            }
            TyKind::Tuple(items) => items.into_iter().for_each(|t| self.ensure_instance(t)),
            TyKind::Array { element, .. }
            | TyKind::Slice(element)
            | TyKind::Ref {
                pointee: element, ..
            }
            | TyKind::Managed(element)
            | TyKind::Desc(element) => self.ensure_instance(element),
            TyKind::ErrorUnion { ok, err } => {
                self.ensure_instance(ok);
                self.ensure_instance(err);
            }
            TyKind::Opaque(OpaqueTy::OwnedStore { element }) => self.ensure_instance(element),
            TyKind::Unit
            | TyKind::Bool
            | TyKind::Never
            | TyKind::Int(_)
            | TyKind::Float(_)
            | TyKind::FnPtr { .. }
            | TyKind::Closure { .. }
            | TyKind::Dyn { .. }
            | TyKind::OwnedDyn { .. }
            | TyKind::Param(_)
            | TyKind::Opaque(_) => {}
        }
    }
}

// ---- function builder -------------------------------------------------------

#[derive(Clone)]
enum FrameCleanup {
    Local(Local),
    Defer(Expr),
}

struct Frame {
    /// User bindings and `defer` blocks declared in this lexical scope, in declaration order.
    cleanups: Vec<FrameCleanup>,
    /// Compiler temporaries needing cleanup created in this scope (no StorageLive/Dead).
    temps: Vec<Local>,
}

struct LoopFrame {
    break_to: BlockId,
    continue_to: BlockId,
    depth: usize,
}

/// One row of the match decision tree.
#[derive(Clone)]
struct Row<'t> {
    cols: Vec<(Place, &'t Pat)>,
    bindings: Vec<(Local, Place, Option<Mutability>)>,
    arm: usize,
}

struct Builder<'m, 'c, 'a, 'e> {
    mono: &'m mut Mono<'c, 'a, 'e>,
    owner_fn: FnRef,
    subst: Vec<Ty>,
    locals: Vec<LocalDecl>,
    blocks: Vec<Block>,
    cur: BlockId,
    map: Vec<Option<Local>>,
    frames: Vec<Frame>,
    loops: Vec<LoopFrame>,
    ret: Option<Local>,
    sig: FnSig,
    locals_info: Vec<LocalInfo>,
    param_locals: Vec<Local>,
    captures: Vec<TyId>,
    closure_id: Option<ClosureId>,
    auto_drops: HashSet<(BlockId, usize)>,
    eval_relations: Vec<SourceRelation>,
    failed: bool,
    reported_exhaustive: bool,
    in_defer_unwind: bool,
}

impl<'m, 'c, 'a, 'e> Builder<'m, 'c, 'a, 'e> {
    fn new(
        mono: &'m mut Mono<'c, 'a, 'e>,
        owner_fn: FnRef,
        thir: &ThirFn,
        subst: Vec<Ty>,
        sig: FnSig,
    ) -> Self {
        let mut b = Builder {
            mono,
            owner_fn,
            subst,
            locals: Vec::new(),
            blocks: Vec::new(),
            cur: BlockId(0),
            map: vec![None; thir.locals.len()],
            frames: vec![Frame {
                cleanups: Vec::new(),
                temps: Vec::new(),
            }],
            loops: Vec::new(),
            ret: None,
            sig,
            locals_info: Vec::new(),
            param_locals: Vec::new(),
            captures: Vec::new(),
            closure_id: None,
            auto_drops: HashSet::new(),
            eval_relations: Vec::new(),
            failed: false,
            reported_exhaustive: false,
            in_defer_unwind: false,
        };
        b.cur = b.new_block(Span::default());
        b
    }

    fn record_eval(&mut self, parent_span: Span, ordinal: u32) {
        let len = self.blocks[self.cur.index()].instructions.len();
        if let Some(last) = len.checked_sub(1) {
            let at = OpLocation {
                block: self.cur,
                instruction: Some(last as u32),
            };
            self.eval_relations.push(SourceRelation::Evaluation {
                node: NodeId(u32::try_from(parent_span.start).unwrap_or(u32::MAX)),
                ordinal,
                at,
            });
        }
    }

    fn eval_operands(&mut self, parent_span: Span, exprs: &[Expr]) -> Vec<Operand> {
        let mut out = Vec::with_capacity(exprs.len());
        for (i, expr) in exprs.iter().enumerate() {
            out.push(self.operand(expr));
            self.record_eval(parent_span, i as u32);
        }
        out
    }

    fn tys(&self) -> &zil_ir::body::TyTable {
        &self.mono.module.tys
    }

    fn ty(&mut self, ty: Ty, span: Span) -> TyId {
        let t = self.mono.types.subst(ty, &self.subst);
        match self.mono.lower_ty(t, span) {
            Some(t) => t,
            None => {
                self.failed = true;
                self.mono.module.tys.intern(TyKind::Unit)
            }
        }
    }

    fn closed(&mut self, ty: Ty) -> Ty {
        let t = self.mono.types.subst(ty, &self.subst);
        self.mono.items.normalize_ty(t, self.mono.types)
    }

    fn new_block(&mut self, span: Span) -> BlockId {
        let id = BlockId::from_index(self.blocks.len());
        self.blocks.push(Block {
            instructions: Vec::new(),
            terminator: Terminator::Unreachable,
            terminator_span: span,
        });
        id
    }

    fn push(&mut self, kind: InstructionKind, span: Span) {
        self.blocks[self.cur.index()]
            .instructions
            .push(Instruction { kind, span });
    }

    /// Ends the current block; code after it lands in a fresh block that is
    /// pruned unless something jumps to it.
    fn terminate(&mut self, term: Terminator, span: Span) {
        let block = &mut self.blocks[self.cur.index()];
        block.terminator = term;
        block.terminator_span = span;
        self.cur = self.new_block(span);
    }

    fn goto(&mut self, target: BlockId, span: Span) {
        self.terminate(Terminator::Goto { target }, span);
    }

    fn temp(&mut self, ty: TyId) -> Local {
        let l = Local::from_index(self.locals.len());
        self.locals.push(LocalDecl { ty, name: None });
        if self.needs_cleanup(ty)
            && let Some(f) = self.frames.last_mut()
        {
            f.temps.push(l);
        }
        l
    }

    fn class(&self, ty: TyId) -> ValueClass {
        self.tys().class(ty, &ParamEnv::default())
    }

    fn use_place(&self, place: Place, ty: TyId) -> Operand {
        match self.class(ty) {
            ValueClass::Copy => Operand::Copy(place),
            ValueClass::Affine | ValueClass::NotValue => Operand::Move(place),
        }
    }

    fn bind_local(&mut self, l: LocalId, info: &LocalInfo) -> Local {
        let ty = self.ty(info.ty, info.span);
        let local = Local::from_index(self.locals.len());
        self.locals.push(LocalDecl {
            ty,
            name: Some(info.name.clone()),
        });
        self.map[l.0 as usize] = Some(local);
        local
    }

    fn local(&self, l: LocalId) -> Local {
        // Every THIR local is bound before use by construction of the
        // builder (parameters on entry, patterns before their scope).
        self.map[l.0 as usize].unwrap_or(Local(u32::MAX))
    }

    fn enter_frame(&mut self) {
        self.frames.push(Frame {
            cleanups: Vec::new(),
            temps: Vec::new(),
        });
    }

    fn needs_cleanup(&self, ty: TyId) -> bool {
        self.tys().needs_cleanup(ty, &ParamEnv::default())
    }

    fn push_auto_drop(&mut self, local: Local, span: Span) {
        let ty = self.locals[local.index()].ty;
        if self.needs_cleanup(ty) {
            let idx = self.blocks[self.cur.index()].instructions.len();
            self.push(
                InstructionKind::Drop {
                    place: Place::local(local),
                },
                span,
            );
            self.auto_drops.insert((self.cur, idx));
        }
    }

    fn exit_frame(&mut self, span: Span) {
        if let Some(frame) = self.frames.pop() {
            for t in frame.temps.into_iter().rev() {
                self.push_auto_drop(t, span);
            }
            for item in frame.cleanups.into_iter().rev() {
                match item {
                    FrameCleanup::Local(l) => {
                        self.push_auto_drop(l, span);
                        self.push(InstructionKind::StorageDead(l), span);
                    }
                    FrameCleanup::Defer(e) => {
                        self.effect(&e);
                    }
                }
            }
        }
    }

    /// Storage ends for every frame deeper than `depth`, without popping
    /// them (a jump out of nested scopes). When unwinding to depth 0 (function
    /// exit), owning parameters are also dropped in reverse order.
    fn build_cancel_block(&mut self, span: Span) -> BlockId {
        let cancel_blk = self.new_block(span);
        let saved = self.cur;
        self.cur = cancel_blk;
        self.unwind_to(0, span);
        self.terminate(Terminator::Trap(zil_ir::body::TrapKind::Cancel), span);
        self.cur = saved;
        cancel_blk
    }

    fn unwind_to(&mut self, depth: usize, span: Span) {
        let frames: Vec<(Vec<Local>, Vec<FrameCleanup>)> = self.frames[depth..]
            .iter()
            .rev()
            .map(|f| (f.temps.clone(), f.cleanups.clone()))
            .collect();
        for (temps, cleanups) in frames {
            for t in temps.into_iter().rev() {
                self.push_auto_drop(t, span);
            }
            for item in cleanups.into_iter().rev() {
                match item {
                    FrameCleanup::Local(l) => {
                        self.push_auto_drop(l, span);
                        self.push(InstructionKind::StorageDead(l), span);
                    }
                    FrameCleanup::Defer(e) => {
                        if !self.in_defer_unwind {
                            self.in_defer_unwind = true;
                            self.effect(&e);
                            self.in_defer_unwind = false;
                        }
                    }
                }
            }
        }
        if depth == 0 {
            let params: Vec<Local> = self.param_locals.iter().rev().copied().collect();
            for p in params {
                self.push_auto_drop(p, span);
            }
            if let Some(cid) = self.closure_id
                && self
                    .tys()
                    .closure(cid)
                    .is_some_and(|c| c.kind == zil_ir::body::ClosureKind::FnOnce)
            {
                for ci in (0..self.captures.len()).rev() {
                    let ty = self.captures[ci];
                    if self.needs_cleanup(ty) {
                        let idx = self.blocks[self.cur.index()].instructions.len();
                        self.push(
                            InstructionKind::Drop {
                                place: Place::capture(ci as u32),
                            },
                            span,
                        );
                        self.auto_drops.insert((self.cur, idx));
                    }
                }
            }
        }
    }

    fn declare(&mut self, l: Local, span: Span) {
        self.push(InstructionKind::StorageLive(l), span);
        if let Some(f) = self.frames.last_mut() {
            f.cleanups.push(FrameCleanup::Local(l));
        }
    }

    fn function(&mut self, thir: &ThirFn) {
        // Parameters are locals 0..n and live on entry.
        for (i, p) in thir.params.iter().enumerate() {
            let info = &thir.locals[p.0 as usize];
            let ty = self.sig.params.get(i).copied().unwrap_or(self.sig.result);
            let local = Local::from_index(self.locals.len());
            self.locals.push(LocalDecl {
                ty,
                name: Some(info.name.clone()),
            });
            self.map[p.0 as usize] = Some(local);
            self.param_locals.push(local);
        }
        let unit = self.tys().lookup(&TyKind::Unit);
        if Some(self.sig.result) != unit {
            let r = Local::from_index(self.locals.len());
            self.locals.push(LocalDecl {
                ty: self.sig.result,
                name: None,
            });
            self.ret = Some(r);
        }
        self.locals_info = thir.locals.clone();
        self.effect(&thir.body);
        let span = thir.body.span;
        if self.ret.is_none() {
            self.unwind_to(0, span);
            self.terminate(
                Terminator::Return {
                    value: Operand::Const(ConstValue::Unit),
                },
                span,
            );
        } else {
            self.terminate(Terminator::Unreachable, span);
        }
    }

    fn finish(mut self) -> (Body, Vec<SourceRelation>) {
        if self.failed {
            self.blocks.clear();
            self.eval_relations.clear();
        }
        let mut body = Body {
            param_count: self.sig.params.len() as u32,
            locals: std::mem::take(&mut self.locals),
            captures: std::mem::take(&mut self.captures),
            closure: self.closure_id,
            result: self.sig.result,
            param_env: ParamEnv::default(),
            drop_flags: Vec::new(),
            blocks: std::mem::take(&mut self.blocks),
        };
        let mut eval_rels = Vec::new();
        if !self.failed {
            crate::drop_elab::elaborate(&mut body, &mut self.mono.module.tys, &self.auto_drops);
            let (blocks, remap) = prune(body.blocks);
            body.blocks = blocks;
            for rel in self.eval_relations {
                if let SourceRelation::Evaluation { node, ordinal, at } = rel
                    && let Some(&new_block) = remap.get(at.block.index())
                    && new_block.0 != u32::MAX
                {
                    let max_inst = body.blocks[new_block.index()].instructions.len();
                    let instruction = at.instruction.and_then(|idx| {
                        if max_inst == 0 {
                            None
                        } else {
                            Some(idx.min((max_inst - 1) as u32))
                        }
                    });
                    eval_rels.push(SourceRelation::Evaluation {
                        node,
                        ordinal,
                        at: OpLocation {
                            block: new_block,
                            instruction,
                        },
                    });
                }
            }
        }
        (body, eval_rels)
    }

    // ---- constants ---------------------------------------------------------

    fn constant(&mut self, lit: Lit, ty: TyId, span: Span) -> Operand {
        let c = match (lit, self.tys().kind(ty).clone()) {
            (Lit::Unit, _) => ConstValue::Unit,
            (Lit::Bool(b), _) => ConstValue::Bool(b),
            (Lit::Int(v), TyKind::Int(t)) => ConstValue::Int {
                ty: t,
                bits: literal::int_bits(v, t, POINTER_BITS),
            },
            (Lit::Float(v), TyKind::Float(FloatTy::F64)) => ConstValue::Float {
                ty: FloatTy::F64,
                bits: v.to_bits(),
            },
            (Lit::Float(v), TyKind::Float(FloatTy::F32)) => ConstValue::Float {
                ty: FloatTy::F32,
                bits: u64::from((v as f32).to_bits()),
            },
            (Lit::Int(_) | Lit::Float(_), _) => {
                self.mono.diags.push(diag::error(
                    diag::TYPE_MISMATCH,
                    span,
                    "internal: literal of a non-numeric type",
                ));
                self.failed = true;
                ConstValue::Unit
            }
        };
        Operand::Const(c)
    }

    // ---- places, operands, rvalues ----------------------------------------

    fn place_adt_args(&self, place: &Place, expected_adt: AdtId) -> Option<ArgsId> {
        let mut ty = match place.root {
            zil_ir::body::PlaceRoot::Local(l) => self.locals.get(l.index())?.ty,
            zil_ir::body::PlaceRoot::Capture(i) => *self.captures.get(i as usize)?,
        };
        for proj in &place.projections {
            ty = match proj {
                Projection::Deref => {
                    if let TyKind::Ref { pointee, .. } = *self.tys().get(ty)? {
                        pointee
                    } else {
                        return None;
                    }
                }
                Projection::Field(f) => self.tys().field_ty(f.adt, f.variant, f.field, f.args)?,
                Projection::TupleField(i) => {
                    if let TyKind::Tuple(ts) = self.tys().get(ty)? {
                        *ts.get(*i as usize)?
                    } else {
                        return None;
                    }
                }
                Projection::Index(_) | Projection::ConstIndex(_) => {
                    if let TyKind::Array { element, .. } | TyKind::Slice(element) =
                        *self.tys().get(ty)?
                    {
                        element
                    } else {
                        return None;
                    }
                }
                Projection::ErrorArm(is_ok) => {
                    if let TyKind::ErrorUnion { ok, err } = *self.tys().get(ty)? {
                        if *is_ok { ok } else { err }
                    } else {
                        return None;
                    }
                }
            };
        }
        loop {
            if let TyKind::Adt { adt, args } = *self.tys().get(ty)? {
                return (adt == expected_adt).then_some(args);
            } else if let TyKind::Managed(inner) | TyKind::Desc(inner) = *self.tys().get(ty)? {
                ty = inner;
            } else {
                return None;
            }
        }
    }

    fn field_ref(
        &mut self,
        base_place: &Place,
        adt: AdtRef,
        variant: u32,
        field: u32,
        args: &[Ty],
        span: Span,
    ) -> Option<FieldRef> {
        let closed: Vec<Ty> = args.iter().map(|a| self.closed(*a)).collect();
        let adt_id = self.mono.adt_id_for(adt, &closed, span)?;
        let args_id = if let Some(exact) = self.place_adt_args(base_place, adt_id) {
            exact
        } else {
            let reg_n = self.mono.items.adt(adt).region_params as usize;
            self.mono
                .args_id_with_regions(&closed, &vec![None; reg_n], span)?
        };
        Some(FieldRef {
            adt: adt_id,
            variant: VariantIdx(variant),
            field: FieldIdx(field),
            args: args_id,
        })
    }

    fn place(&mut self, e: &Expr) -> Place {
        match &e.kind {
            ExprKind::Local(l) => Place::local(self.local(*l)),
            ExprKind::Capture(i) => Place::capture(*i),
            ExprKind::Field {
                base,
                adt,
                variant,
                field,
                args,
            } => {
                let mut p = self.place(base);
                match self.field_ref(&p, *adt, *variant, *field, args, e.span) {
                    Some(f) => p.projections.push(Projection::Field(f)),
                    None => self.failed = true,
                }
                p
            }
            ExprKind::TupleField { base, index } => {
                let mut p = self.place(base);
                p.projections.push(Projection::TupleField(*index));
                p
            }
            ExprKind::Index { base, index } => {
                let mut p = self.place(base);
                let i = self.index_local(index);
                p.projections.push(Projection::Index(i));
                p
            }
            // The address itself is read in place; going through it is a
            // projection, not a use of the address value.
            ExprKind::Deref(base) => self.place(base).deref(),
            ExprKind::Borrow { .. }
            | ExprKind::SliceBorrow { .. }
            | ExprKind::Len(_)
            | ExprKind::Give(_)
            | ExprKind::Describe(_)
            | ExprKind::ManagedCreate { .. }
            | ExprKind::ForeignCall { .. }
            | ExprKind::Intrinsic { .. }
            | ExprKind::ErrorInject { .. }
            | ExprKind::UnionInject { .. }
            | ExprKind::Try(_)
            | ExprKind::CtEval(_)
            | ExprKind::Lit(_)
            | ExprKind::ConstGeneric(_)
            | ExprKind::StaticUtf(_)
            | ExprKind::Unary { .. }
            | ExprKind::Binary { .. }
            | ExprKind::Logical { .. }
            | ExprKind::Convert { .. }
            | ExprKind::FnRef { .. }
            | ExprKind::ForeignRef(_)
            | ExprKind::Call { .. }
            | ExprKind::IndirectCall { .. }
            | ExprKind::Closure { .. }
            | ExprKind::ClosureFnRef(_)
            | ExprKind::ClosureCall { .. }
            | ExprKind::TraitCall { .. }
            | ExprKind::DynCall { .. }
            | ExprKind::Adt { .. }
            | ExprKind::Tuple(_)
            | ExprKind::Array(_)
            | ExprKind::If { .. }
            | ExprKind::Match { .. }
            | ExprKind::Block(_)
            | ExprKind::Loop(_)
            | ExprKind::While { .. }
            | ExprKind::Break
            | ExprKind::Continue
            | ExprKind::Return(_)
            | ExprKind::Assign { .. }
            | ExprKind::Trap => {
                let ty = self.ty(e.ty, e.span);
                let t = self.temp(ty);
                self.into(Place::local(t), e);
                Place::local(t)
            }
        }
    }

    /// The `usize` local an index projection reads. A narrower unsigned
    /// index is converted with the checked conversion.
    fn index_local(&mut self, index: &Expr) -> Local {
        let usize_ty = self.mono.module.tys.intern(TyKind::Int(IntTy::Usize));
        let ity = self.ty(index.ty, index.span);
        if ity == usize_ty
            && let ExprKind::Local(l) = index.kind
        {
            return self.local(l);
        }
        let value = self.operand(index);
        let t = self.temp(usize_ty);
        let rv = if ity == usize_ty {
            Rvalue::Use(value)
        } else {
            Rvalue::Convert {
                operand: value,
                to: usize_ty,
            }
        };
        self.push(
            InstructionKind::Assign {
                dest: Place::local(t),
                value: rv,
            },
            index.span,
        );
        t
    }

    fn operand(&mut self, e: &Expr) -> Operand {
        if let ExprKind::Lit(l) = &e.kind {
            let ty = self.ty(e.ty, e.span);
            return self.constant(*l, ty, e.span);
        }
        if let ExprKind::ConstGeneric(idx) = &e.kind {
            let ty = self.ty(e.ty, e.span);
            let val = self
                .subst
                .get(*idx as usize)
                .and_then(|&t| {
                    if let TyData::ConstU32(n) = self.mono.types.data(t) {
                        Some(*n)
                    } else {
                        None
                    }
                })
                .unwrap_or(0);
            return self.constant(Lit::Int(val as i128), ty, e.span);
        }
        if let ExprKind::Give(inner) = &e.kind {
            return Operand::Move(self.place(inner));
        }
        let ty = self.ty(e.ty, e.span);
        let p = self.place(e);
        if matches!(
            self.tys().kind(ty),
            TyKind::Ref {
                mutability: Mutability::Exclusive,
                ..
            }
        ) && p
            .projections
            .iter()
            .any(|pr| matches!(pr, Projection::Deref))
        {
            let mut derefed = p;
            derefed.projections.push(Projection::Deref);
            let tmp = self.temp(ty);
            self.assign(
                Place::local(tmp),
                Rvalue::Ref {
                    region: Region::Static,
                    mutability: Mutability::Exclusive,
                    place: derefed,
                },
                e.span,
            );
            return Operand::Move(Place::local(tmp));
        }
        self.use_place(p, ty)
    }

    fn rvalue(&mut self, e: &Expr) -> Rvalue {
        match &e.kind {
            ExprKind::Unary { op, operand } => Rvalue::Unary {
                op: *op,
                operand: self.operand(operand),
            },
            ExprKind::Binary { op, lhs, rhs } => {
                let lhs = self.operand(lhs);
                self.record_eval(e.span, 0);
                let rhs = self.operand(rhs);
                self.record_eval(e.span, 1);
                Rvalue::Binary { op: *op, lhs, rhs }
            }
            ExprKind::Convert { operand } => {
                let operand = self.operand(operand);
                let to = self.ty(e.ty, e.span);
                Rvalue::Convert { operand, to }
            }
            ExprKind::Adt {
                adt,
                variant,
                args,
                fields,
            } => {
                let closed: Vec<Ty> = args.iter().map(|a| self.closed(*a)).collect();
                let operands = self.eval_operands(e.span, fields);
                let Some(adt_id) = self.mono.adt_id_for(*adt, &closed, e.span) else {
                    self.failed = true;
                    return Rvalue::Use(Operand::Const(ConstValue::Unit));
                };
                let tid = self.ty(e.ty, e.span);
                let args_id = if let Some(TyKind::Adt { adt: a, args }) = self.tys().get(tid)
                    && *a == adt_id
                {
                    Some(*args)
                } else {
                    let reg_n = self.mono.items.adt(*adt).region_params as usize;
                    self.mono
                        .args_id_with_regions(&closed, &vec![None; reg_n], e.span)
                };
                let Some(args) = args_id else {
                    self.failed = true;
                    return Rvalue::Use(Operand::Const(ConstValue::Unit));
                };
                let adt = adt_id;
                Rvalue::Aggregate {
                    kind: AggregateKind::Adt {
                        adt,
                        variant: VariantIdx(*variant),
                        args,
                    },
                    operands,
                }
            }
            ExprKind::Tuple(items) => Rvalue::Aggregate {
                kind: AggregateKind::Tuple,
                operands: self.eval_operands(e.span, items),
            },
            ExprKind::Array(items) => {
                let ty = self.ty(e.ty, e.span);
                let element = if let TyKind::Array { element, .. } = self.tys().kind(ty) {
                    *element
                } else {
                    self.failed = true;
                    self.mono.module.tys.intern(TyKind::Unit)
                };
                Rvalue::Aggregate {
                    kind: AggregateKind::Array { element },
                    operands: self.eval_operands(e.span, items),
                }
            }
            ExprKind::Borrow { mutable, place } => {
                let place = self.place(place);
                let mutability = if *mutable {
                    Mutability::Exclusive
                } else {
                    Mutability::Shared
                };
                let ref_ty = self.ty(e.ty, e.span);
                if let Some(&TyKind::Ref { pointee, .. }) = self.mono.module.tys.get(ref_ty) {
                    self.mono.module.tys.intern(TyKind::Ref {
                        region: Region::Static,
                        mutability,
                        pointee,
                    });
                }
                Rvalue::Ref {
                    region: Region::Static,
                    mutability,
                    place,
                }
            }
            ExprKind::SliceBorrow {
                mutable,
                place,
                start,
                end,
            } => {
                let place = self.place(place);
                let start = start
                    .as_ref()
                    .map(|s| Operand::Copy(Place::local(self.index_local(s))));
                let end = end
                    .as_ref()
                    .map(|e| Operand::Copy(Place::local(self.index_local(e))));
                Rvalue::SliceRef {
                    region: Region::Static,
                    mutability: if *mutable {
                        Mutability::Exclusive
                    } else {
                        Mutability::Shared
                    },
                    place,
                    start,
                    end,
                }
            }
            ExprKind::Describe(inner) => Rvalue::Describe {
                place: self.place(inner),
            },
            ExprKind::Closure { id, captures } => {
                let operands = captures.iter().map(|c| self.operand(c)).collect();
                let Some(def) =
                    self.mono
                        .ensure_closure(self.owner_fn, self.subst.clone(), *id, e.span)
                else {
                    self.failed = true;
                    return Rvalue::Use(Operand::Const(ConstValue::Unit));
                };
                let Some(args) = self.mono.args_id(&[], e.span) else {
                    self.failed = true;
                    return Rvalue::Use(Operand::Const(ConstValue::Unit));
                };
                Rvalue::Aggregate {
                    kind: AggregateKind::Closure { def, args },
                    operands,
                }
            }
            ExprKind::ClosureFnRef(id) => {
                match self
                    .mono
                    .ensure_closure_fn(self.owner_fn, self.subst.clone(), *id, e.span)
                {
                    Some(fn_id) => Rvalue::FnRef(fn_id),
                    None => {
                        self.failed = true;
                        Rvalue::Use(Operand::Const(ConstValue::Unit))
                    }
                }
            }
            ExprKind::FnRef { func, type_args } => {
                let closed: Vec<Ty> = type_args.iter().map(|a| self.closed(*a)).collect();
                match self.mono.instance(*func, closed, e.span) {
                    Some(id) => Rvalue::FnRef(id),
                    None => {
                        self.failed = true;
                        Rvalue::Use(Operand::Const(ConstValue::Unit))
                    }
                }
            }
            ExprKind::ForeignRef(f) => Rvalue::ForeignFnRef(ForeignId(f.0)),
            ExprKind::ErrorInject { ok, value } => {
                let ty = self.ty(e.ty, e.span);
                Rvalue::ErrorUnion {
                    ok: *ok,
                    value: self.operand(value),
                    ty,
                }
            }
            ExprKind::UnionInject { value } => {
                let union_ty = self.closed(e.ty);
                let val_ty = self.ty(value.ty, value.span);
                if let TyData::Union(members) = self.mono.types.data(union_ty).clone()
                    && let Some((adt, member_ids)) = self.mono.union_adt(&members, e.span)
                    && let Some(variant) = member_ids.iter().position(|&m| m == val_ty)
                    && let Some(args) = self.mono.args_id(&[], e.span)
                {
                    let op = self.operand(value);
                    Rvalue::Aggregate {
                        kind: AggregateKind::Adt {
                            adt,
                            variant: VariantIdx(variant as u32),
                            args,
                        },
                        operands: vec![op],
                    }
                } else {
                    self.failed = true;
                    Rvalue::Use(Operand::Const(ConstValue::Unit))
                }
            }
            ExprKind::StaticUtf(bytes) => {
                let id = StaticId(self.mono.module.statics.len() as u32);
                self.mono.module.statics.push(bytes.clone());
                Rvalue::StaticUtf(id)
            }
            ExprKind::Lit(_)
            | ExprKind::Len(_)
            | ExprKind::Give(_)
            | ExprKind::ManagedCreate { .. }
            | ExprKind::ForeignCall { .. }
            | ExprKind::Intrinsic { .. }
            | ExprKind::Try(_)
            | ExprKind::CtEval(_)
            | ExprKind::ConstGeneric(_)
            | ExprKind::Local(_)
            | ExprKind::Capture(_)
            | ExprKind::Field { .. }
            | ExprKind::TupleField { .. }
            | ExprKind::Index { .. }
            | ExprKind::Deref(_)
            | ExprKind::Logical { .. }
            | ExprKind::Call { .. }
            | ExprKind::IndirectCall { .. }
            | ExprKind::ClosureCall { .. }
            | ExprKind::TraitCall { .. }
            | ExprKind::DynCall { .. }
            | ExprKind::If { .. }
            | ExprKind::Match { .. }
            | ExprKind::Block(_)
            | ExprKind::Loop(_)
            | ExprKind::While { .. }
            | ExprKind::Break
            | ExprKind::Continue
            | ExprKind::Return(_)
            | ExprKind::Assign { .. }
            | ExprKind::Trap => Rvalue::Use(self.operand(e)),
        }
    }

    // ---- into a destination ------------------------------------------------

    fn assign(&mut self, dest: Place, value: Rvalue, span: Span) {
        self.push(InstructionKind::Assign { dest, value }, span);
    }

    fn into(&mut self, dest: Place, e: &Expr) {
        let span = e.span;
        match &e.kind {
            ExprKind::If {
                cond,
                then,
                otherwise,
            } => {
                let (t, f) = self.branch(cond);
                let join = self.new_block(span);
                self.cur = t;
                self.into(dest.clone(), then);
                self.goto(join, span);
                self.cur = f;
                match otherwise {
                    Some(o) => self.into(dest, o),
                    None => self.assign(dest, Rvalue::Use(Operand::Const(ConstValue::Unit)), span),
                }
                self.goto(join, span);
                self.cur = join;
            }
            ExprKind::Logical { and, lhs, rhs } => {
                let (t, f) = self.branch(lhs);
                let join = self.new_block(span);
                let (eval_rhs, short) = if *and { (t, f) } else { (f, t) };
                self.cur = short;
                self.assign(
                    dest.clone(),
                    Rvalue::Use(Operand::Const(ConstValue::Bool(!*and))),
                    span,
                );
                self.goto(join, span);
                self.cur = eval_rhs;
                let v = self.operand(rhs);
                self.assign(dest, Rvalue::Use(v), span);
                self.goto(join, span);
                self.cur = join;
            }
            ExprKind::Call {
                callee,
                type_args,
                args,
            } => {
                let closed: Vec<Ty> = type_args.iter().map(|a| self.closed(*a)).collect();
                let operands: Vec<Operand> = args.iter().map(|a| self.operand(a)).collect();
                let Some(f) = self.mono.instance(*callee, closed, span) else {
                    self.failed = true;
                    return;
                };
                let target = self.new_block(span);
                let callee_suspends = self
                    .mono
                    .fn_susp
                    .get(callee.0 as usize)
                    .copied()
                    .unwrap_or(false);
                let cancel = if callee_suspends && !self.in_defer_unwind {
                    Some(self.build_cancel_block(span))
                } else {
                    None
                };
                self.terminate(
                    Terminator::Call {
                        callee: Callee::Direct(f),
                        args: operands,
                        dest,
                        target,
                        cancel,
                    },
                    span,
                );
                self.cur = target;
            }
            ExprKind::ClosureCall {
                closure,
                mode,
                args,
            } => {
                let place = self.place(closure);
                let operands: Vec<Operand> = args.iter().map(|a| self.operand(a)).collect();
                let target = self.new_block(span);
                let cl_ty = self.closed(closure.ty);
                let closure_suspends = resolve_closure_for_suspend(self.mono.types, cl_ty)
                    .is_some_and(|(o, i)| {
                        self.mono
                            .cl_susp
                            .get(o)
                            .and_then(|v| v.get(i))
                            .copied()
                            .unwrap_or(false)
                    });
                let cancel = if closure_suspends && !self.in_defer_unwind {
                    Some(self.build_cancel_block(span))
                } else {
                    None
                };
                self.terminate(
                    Terminator::Call {
                        callee: Callee::Closure {
                            closure: place,
                            mode: *mode,
                        },
                        args: operands,
                        dest,
                        target,
                        cancel,
                    },
                    span,
                );
                self.cur = target;
            }
            ExprKind::TraitCall {
                trait_ref,
                trait_args,
                method_args,
                self_ty,
                method,
                args,
            } => {
                let closed_self = self.closed(*self_ty);
                let closed_args: Vec<Ty> = trait_args.iter().map(|a| self.closed(*a)).collect();
                let closed_method_args: Vec<Ty> =
                    method_args.iter().map(|a| self.closed(*a)).collect();
                let mut found: Option<(FnRef, Vec<Ty>)> = None;
                let impls = self.mono.items.trait_impls.clone();
                for imp in &impls {
                    if imp.trait_ref != *trait_ref {
                        continue;
                    }
                    let Some(impl_args) =
                        self.mono
                            .items
                            .match_impl(imp, closed_self, self.mono.types)
                    else {
                        continue;
                    };
                    let imp_targs: Vec<Ty> = imp
                        .trait_args
                        .iter()
                        .map(|t| {
                            let s = self.mono.types.subst(*t, &impl_args);
                            self.mono.items.normalize_ty(s, self.mono.types)
                        })
                        .collect();
                    if imp_targs != closed_args {
                        continue;
                    }
                    if let Some((_, fn_ref)) = imp.methods.iter().find(|(n, _)| n == method) {
                        found = Some((*fn_ref, impl_args));
                        break;
                    }
                }
                let Some((fn_ref, mut full_args)) = found else {
                    self.failed = true;
                    return;
                };
                full_args.extend(closed_method_args);
                let operands: Vec<Operand> = args.iter().map(|a| self.operand(a)).collect();
                let Some(f) = self.mono.instance(fn_ref, full_args, span) else {
                    self.failed = true;
                    return;
                };
                let target = self.new_block(span);
                let callee_suspends = self
                    .mono
                    .fn_susp
                    .get(fn_ref.0 as usize)
                    .copied()
                    .unwrap_or(false);
                let cancel = if callee_suspends && !self.in_defer_unwind {
                    Some(self.build_cancel_block(span))
                } else {
                    None
                };
                self.terminate(
                    Terminator::Call {
                        callee: Callee::Direct(f),
                        args: operands,
                        dest,
                        target,
                        cancel,
                    },
                    span,
                );
                self.cur = target;
            }
            ExprKind::DynCall { object, slot, args } => {
                let obj = self.operand(object);
                let operands: Vec<Operand> = args.iter().map(|a| self.operand(a)).collect();
                let target = self.new_block(span);
                self.terminate(
                    Terminator::Call {
                        callee: Callee::DynSlot {
                            object: obj,
                            slot: *slot,
                        },
                        args: operands,
                        dest,
                        target,
                        cancel: None,
                    },
                    span,
                );
                self.cur = target;
            }
            ExprKind::IndirectCall { callee, args } => {
                let target_fn = self.operand(callee);
                let operands: Vec<Operand> = args.iter().map(|a| self.operand(a)).collect();
                let target = self.new_block(span);
                self.terminate(
                    Terminator::Call {
                        callee: Callee::Indirect(target_fn),
                        args: operands,
                        dest,
                        target,
                        cancel: None,
                    },
                    span,
                );
                self.cur = target;
            }
            ExprKind::Len(inner) => {
                let place = self.place(inner);
                let usize_ty = self.mono.module.tys.intern(TyKind::Int(IntTy::Usize));
                let i64_ty = self.mono.module.tys.intern(TyKind::Int(IntTy::I64));
                let raw = self.temp(usize_ty);
                self.assign(Place::local(raw), Rvalue::Len(place), span);
                self.assign(
                    dest,
                    Rvalue::Convert {
                        operand: Operand::Copy(Place::local(raw)),
                        to: i64_ty,
                    },
                    span,
                );
            }
            ExprKind::ForeignCall { callee, args } => {
                let operands: Vec<Operand> = args.iter().map(|a| self.operand(a)).collect();
                let target = self.new_block(span);
                self.terminate(
                    Terminator::Call {
                        callee: Callee::Foreign(ForeignId(callee.0)),
                        args: operands,
                        dest,
                        target,
                        cancel: None,
                    },
                    span,
                );
                self.cur = target;
            }
            ExprKind::Intrinsic { op, args } => {
                if *op == IntrinsicOp::OwnedDynCreate
                    && let [arg0] = args.as_slice()
                {
                    let closed_ret = self.closed(e.ty);
                    if let TyData::ErrorUnion { ok, .. } = *self.mono.types.data(closed_ret)
                        && let TyData::OwnedDyn {
                            trait_ref: tr,
                            trait_args,
                            assoc_bindings,
                            ..
                        } = self.mono.types.data(ok).clone()
                    {
                        let closed_concrete = self.closed(arg0.ty);
                        self.mono.ensure_dyn_impl(
                            tr,
                            &trait_args,
                            &assoc_bindings,
                            closed_concrete,
                            span,
                        );
                    }
                }
                let operands: Vec<Operand> = args.iter().map(|a| self.operand(a)).collect();
                let target = self.new_block(span);
                let cancel = if *op == IntrinsicOp::TaskYield && !self.in_defer_unwind {
                    Some(self.build_cancel_block(span))
                } else {
                    None
                };
                self.terminate(
                    Terminator::Call {
                        callee: Callee::Intrinsic(*op),
                        args: operands,
                        dest,
                        target,
                        cancel,
                    },
                    span,
                );
                self.cur = target;
            }
            ExprKind::ManagedCreate { fields, .. } => {
                let operands: Vec<Operand> = fields.iter().map(|f| self.operand(f)).collect();
                let target = self.new_block(span);
                self.terminate(
                    Terminator::Call {
                        callee: Callee::Intrinsic(IntrinsicOp::ManagedCreate),
                        args: operands,
                        dest,
                        target,
                        cancel: None,
                    },
                    span,
                );
                self.cur = target;
            }
            ExprKind::Try(inner) => {
                let is_temp = !is_place(&inner.kind);
                let src = self.place(inner);
                let inner_ty = self.ty(inner.ty, inner.span);
                if let TyKind::ErrorUnion {
                    ok: ok_ty,
                    err: err_ty,
                } = self.tys().kind(inner_ty).clone()
                {
                    let bool_ty = self.mono.module.tys.intern(TyKind::Bool);
                    let tag = self.temp(bool_ty);
                    self.assign(Place::local(tag), Rvalue::ErrorArm(src.clone()), span);
                    let ok_bb = self.new_block(span);
                    let err_bb = self.new_block(span);
                    self.terminate(
                        Terminator::Switch {
                            discr: Operand::Copy(Place::local(tag)),
                            arms: vec![(1, ok_bb)],
                            otherwise: err_bb,
                        },
                        span,
                    );
                    // Error arm: extract error payload, settle temp, wrap into enclosing error union, unwind and return.
                    self.cur = err_bb;
                    let err_place = src.clone().project(Projection::ErrorArm(false));
                    let err_val = self.use_place(err_place, err_ty);
                    if let Some(r) = self.ret {
                        self.assign(
                            Place::local(r),
                            Rvalue::ErrorUnion {
                                ok: false,
                                value: err_val,
                                ty: self.sig.result,
                            },
                            span,
                        );
                    }
                    if is_temp && let Some(tl) = src.as_local() {
                        self.push(InstructionKind::StorageDead(tl), span);
                    }
                    self.unwind_to(0, span);
                    let ret_op = match self.ret {
                        Some(r) => self.use_place(Place::local(r), self.sig.result),
                        None => Operand::Const(ConstValue::Unit),
                    };
                    self.terminate(Terminator::Return { value: ret_op }, span);
                    // Success arm: extract ok payload into `dest` and settle the temp immediately.
                    self.cur = ok_bb;
                    let ok_place = src.clone().project(Projection::ErrorArm(true));
                    let ok_val = self.use_place(ok_place, ok_ty);
                    self.assign(dest, Rvalue::Use(ok_val), span);
                    if is_temp && let Some(tl) = src.as_local() {
                        self.push(InstructionKind::StorageDead(tl), span);
                    }
                } else if matches!(self.tys().kind(inner_ty), TyKind::Adt { .. }) {
                    let t = self.mono.types.subst(inner.ty, &self.subst);
                    let closed = self.mono.items.normalize_ty(t, self.mono.types);
                    let closed = self.mono.types.shallow(closed);
                    let TyData::Adt {
                        adt: AdtRef(0),
                        args,
                        ..
                    } = self.mono.types.data(closed).clone()
                    else {
                        self.failed = true;
                        return;
                    };
                    if args.len() != 1 {
                        self.failed = true;
                        return;
                    }
                    let ok_ty = self.ty(args[0], inner.span);
                    let i32_ty = self.mono.module.tys.intern(TyKind::Int(IntTy::I32));
                    let tag = self.temp(i32_ty);
                    self.assign(Place::local(tag), Rvalue::Discriminant(src.clone()), span);
                    let ok_bb = self.new_block(span);
                    let none_bb = self.new_block(span);
                    self.terminate(
                        Terminator::Switch {
                            discr: Operand::Copy(Place::local(tag)),
                            arms: vec![(1, ok_bb)],
                            otherwise: none_bb,
                        },
                        span,
                    );
                    // None arm: return Option.None of enclosing return type.
                    self.cur = none_bb;
                    if let Some(r) = self.ret {
                        let TyKind::Adt {
                            adt: ret_adt,
                            args: ret_args,
                        } = *self.tys().kind(self.sig.result)
                        else {
                            self.failed = true;
                            return;
                        };
                        self.assign(
                            Place::local(r),
                            Rvalue::Aggregate {
                                kind: AggregateKind::Adt {
                                    adt: ret_adt,
                                    variant: VariantIdx(0),
                                    args: ret_args,
                                },
                                operands: Vec::new(),
                            },
                            span,
                        );
                    }
                    if is_temp && let Some(tl) = src.as_local() {
                        self.push(InstructionKind::StorageDead(tl), span);
                    }
                    self.unwind_to(0, span);
                    let ret_op = match self.ret {
                        Some(r) => self.use_place(Place::local(r), self.sig.result),
                        None => Operand::Const(ConstValue::Unit),
                    };
                    self.terminate(Terminator::Return { value: ret_op }, span);
                    // Some arm: extract payload field 0 into `dest` and settle temp.
                    self.cur = ok_bb;
                    let mut ok_place = src.clone();
                    match self.field_ref(&src, AdtRef(0), 1, 0, &args, span) {
                        Some(f) => ok_place.projections.push(Projection::Field(f)),
                        None => {
                            self.failed = true;
                            return;
                        }
                    }
                    let ok_val = self.use_place(ok_place, ok_ty);
                    self.assign(dest, Rvalue::Use(ok_val), span);
                    if is_temp && let Some(tl) = src.as_local() {
                        self.push(InstructionKind::StorageDead(tl), span);
                    }
                } else {
                    self.failed = true;
                }
            }
            ExprKind::CtEval(inner) => {
                let ty = self.ty(e.ty, span);
                if let Some(val) = self.eval_ct(inner, e.ty, span) {
                    self.emit_ct_value(dest, &val, ty, span);
                }
            }
            ExprKind::Block(stmts) => {
                self.enter_frame();
                let last_idx = stmts.len().saturating_sub(1);
                for (i, s) in stmts.iter().enumerate() {
                    match s {
                        Stmt::Let { pattern, init } => self.let_stmt(pattern, init),
                        Stmt::Expr(x) if i == last_idx => self.into(dest.clone(), x),
                        Stmt::Expr(x) => self.effect(x),
                        Stmt::Defer(x) => {
                            if let Some(f) = self.frames.last_mut() {
                                f.cleanups.push(FrameCleanup::Defer(x.clone()));
                            }
                        }
                    }
                }
                if !matches!(stmts.last(), Some(Stmt::Expr(_))) {
                    let unit = self.tys().lookup(&TyKind::Unit);
                    if Some(self.ty(e.ty, span)) == unit {
                        self.assign(dest, Rvalue::Use(Operand::Const(ConstValue::Unit)), span);
                    }
                }
                self.exit_frame(span);
            }
            ExprKind::Match { scrutinee, arms } => {
                let unit = self.tys().lookup(&TyKind::Unit);
                if Some(self.ty(e.ty, span)) == unit {
                    self.lower_match(scrutinee, arms, None, span);
                    self.assign(dest, Rvalue::Use(Operand::Const(ConstValue::Unit)), span);
                } else {
                    self.lower_match(scrutinee, arms, Some(dest), span);
                }
            }
            ExprKind::Trap => {
                self.terminate(Terminator::Trap(zil_ir::body::TrapKind::Explicit), span);
            }
            ExprKind::Loop(_)
            | ExprKind::While { .. }
            | ExprKind::Break
            | ExprKind::Continue
            | ExprKind::Return(_)
            | ExprKind::Assign { .. } => {
                self.effect(e);
                let unit = self.tys().lookup(&TyKind::Unit);
                if Some(self.ty(e.ty, span)) == unit {
                    self.assign(dest, Rvalue::Use(Operand::Const(ConstValue::Unit)), span);
                }
            }
            ExprKind::Lit(_)
            | ExprKind::ConstGeneric(_)
            | ExprKind::StaticUtf(_)
            | ExprKind::Local(_)
            | ExprKind::Capture(_)
            | ExprKind::Field { .. }
            | ExprKind::TupleField { .. }
            | ExprKind::Index { .. }
            | ExprKind::Deref(_)
            | ExprKind::Borrow { .. }
            | ExprKind::SliceBorrow { .. }
            | ExprKind::Give(_)
            | ExprKind::FnRef { .. }
            | ExprKind::ForeignRef(_)
            | ExprKind::Closure { .. }
            | ExprKind::ClosureFnRef(_)
            | ExprKind::Describe(_)
            | ExprKind::ErrorInject { .. }
            | ExprKind::UnionInject { .. }
            | ExprKind::Unary { .. }
            | ExprKind::Binary { .. }
            | ExprKind::Convert { .. }
            | ExprKind::Adt { .. }
            | ExprKind::Tuple(_)
            | ExprKind::Array(_) => {
                let rv = self.rvalue(e);
                self.assign(dest, rv, span);
            }
        }
    }

    fn is_ct_escapable(&mut self, ty: Ty, depth: u32) -> bool {
        if depth > 64 {
            return false;
        }
        let ty = self.mono.items.normalize_ty(ty, self.mono.types);
        let data = self.mono.types.data(self.mono.types.shallow(ty)).clone();
        match data {
            TyData::Unit
            | TyData::Bool
            | TyData::Int(_)
            | TyData::Float(_)
            | TyData::FnPtr { .. } => true,
            TyData::Tuple(items) => items.iter().all(|t| self.is_ct_escapable(*t, depth + 1)),
            TyData::Array { element, .. } => self.is_ct_escapable(element, depth + 1),
            TyData::ErrorUnion { ok, err } => {
                self.is_ct_escapable(ok, depth + 1) && self.is_ct_escapable(err, depth + 1)
            }
            TyData::Adt { adt, args, .. } => {
                let decl = self.mono.items.adt(adt);
                match decl.kind {
                    AdtKind::Record | AdtKind::Enum => {
                        let fields: Vec<Ty> = decl
                            .variants
                            .iter()
                            .flat_map(|v| v.fields.iter().map(|f| f.ty))
                            .collect();
                        fields.into_iter().all(|f| {
                            let ft = self.mono.types.subst(f, &args);
                            self.is_ct_escapable(ft, depth + 1)
                        })
                    }
                    AdtKind::Package | AdtKind::Resource => false,
                }
            }
            TyData::Type
            | TyData::Never
            | TyData::Slice(_)
            | TyData::Ref { .. }
            | TyData::Managed(_)
            | TyData::Desc(_)
            | TyData::Opaque(_)
            | TyData::Union(_)
            | TyData::Closure { .. }
            | TyData::ImplCallable { .. }
            | TyData::Dyn { .. }
            | TyData::OwnedDyn { .. }
            | TyData::Param(_)
            | TyData::ConstParam(_)
            | TyData::ConstU32(_)
            | TyData::ArrayConst { .. }
            | TyData::SelfParam
            | TyData::Assoc { .. }
            | TyData::Var(_)
            | TyData::Error => false,
        }
    }

    fn eval_ct(&mut self, inner: &Expr, result_ty: Ty, span: Span) -> Option<CtValue> {
        if self.mono.ct.is_none() {
            self.mono
                .diags
                .push(diag::unsupported("compile-time evaluation", span));
            self.failed = true;
            return None;
        }
        let closed_ty = self.closed(result_ty);
        if !self.mono.is_ct_eval && !self.is_ct_escapable(closed_ty, 0) {
            self.mono.diags.push(diag::error(
                diag::RESOURCE_TYPE,
                span,
                "non-serializable or owning value cannot escape from compile-time evaluation to runtime",
            ));
            self.failed = true;
            return None;
        }
        let subst = self.subst.clone();
        let ct_handle = self.mono.ct.take();
        let mut ct_mono = Mono::new(
            self.mono.items,
            self.mono.types,
            self.mono.thir,
            self.mono.diags,
            self.mono.limits,
            ct_handle,
        );
        ct_mono.is_ct_eval = true;
        ct_mono.lower_foreigns();
        let Some(result) = ct_mono.lower_ty(closed_ty, span) else {
            self.mono.ct = ct_mono.ct.take();
            self.failed = true;
            return None;
        };
        let sig = FnSig {
            params: Vec::new(),
            result,
            type_params: 0,
            region_params: 0,
            role: FnRole::Normal,
            resume_mode: ResumeMode::Local,
            state_contracts: Vec::new(),
        };
        let body_expr = if ct_mono.types.is_never(inner.ty) {
            inner.clone()
        } else {
            Expr {
                kind: ExprKind::Return(Some(Box::new(inner.clone()))),
                ty: inner.ty,
                span,
            }
        };
        let synthetic_thir = ThirFn {
            locals: self.locals_info.clone(),
            params: Vec::new(),
            body: body_expr,
            closures: Vec::new(),
            opaque_return: None,
        };
        let mut b = Builder::new(
            &mut ct_mono,
            self.owner_fn,
            &synthetic_thir,
            subst,
            sig.clone(),
        );
        b.function(&synthetic_thir);
        let failed = b.failed;
        let (body, _) = b.finish();
        if failed {
            self.mono.ct = ct_mono.ct.take();
            self.failed = true;
            return None;
        }
        let entry = FnId::from_index(ct_mono.module.functions.len());
        ct_mono.module.functions.push(Function {
            name: "$ct_eval".to_string(),
            sig,
            body,
        });
        ct_mono.run();
        let ct_module = std::mem::take(&mut ct_mono.module);
        self.mono.ct = ct_mono.ct.take();
        drop(ct_mono);
        let evaluator = self.mono.ct.as_deref_mut()?;
        match evaluator.eval(ct_module, entry, span) {
            Ok(v) => Some(v),
            Err(d) => {
                self.mono.diags.push(d);
                self.failed = true;
                None
            }
        }
    }

    fn emit_ct_value(&mut self, dest: Place, val: &CtValue, ty: TyId, span: Span) {
        match val {
            CtValue::Unit => {
                self.assign(dest, Rvalue::Use(Operand::Const(ConstValue::Unit)), span);
            }
            CtValue::Bool(b) => {
                self.assign(
                    dest,
                    Rvalue::Use(Operand::Const(ConstValue::Bool(*b))),
                    span,
                );
            }
            CtValue::Int { ty: ity, bits } => {
                self.assign(
                    dest,
                    Rvalue::Use(Operand::Const(ConstValue::Int {
                        ty: *ity,
                        bits: *bits,
                    })),
                    span,
                );
            }
            CtValue::Float { ty: fty, bits } => {
                self.assign(
                    dest,
                    Rvalue::Use(Operand::Const(ConstValue::Float {
                        ty: *fty,
                        bits: *bits,
                    })),
                    span,
                );
            }
            CtValue::Aggregate { variant, fields } => {
                let kind_ty = self.tys().kind(ty).clone();
                match kind_ty {
                    TyKind::Tuple(elem_tys) => {
                        let mut ops = Vec::with_capacity(fields.len());
                        for (f, ft) in fields.iter().zip(elem_tys) {
                            ops.push(self.emit_ct_operand(f, ft, span));
                        }
                        self.assign(
                            dest,
                            Rvalue::Aggregate {
                                kind: AggregateKind::Tuple,
                                operands: ops,
                            },
                            span,
                        );
                    }
                    TyKind::Array { element, .. } => {
                        let mut ops = Vec::with_capacity(fields.len());
                        for f in fields {
                            ops.push(self.emit_ct_operand(f, element, span));
                        }
                        self.assign(
                            dest,
                            Rvalue::Aggregate {
                                kind: AggregateKind::Array { element },
                                operands: ops,
                            },
                            span,
                        );
                    }
                    TyKind::Adt { adt, args } => {
                        let v_idx = VariantIdx(variant.unwrap_or(0));
                        let mut ops = Vec::with_capacity(fields.len());
                        for (i, f) in fields.iter().enumerate() {
                            let ft = self
                                .tys()
                                .field_ty(adt, v_idx, FieldIdx::from_index(i), args)
                                .unwrap_or_else(|| self.mono.module.tys.intern(TyKind::Unit));
                            ops.push(self.emit_ct_operand(f, ft, span));
                        }
                        self.assign(
                            dest,
                            Rvalue::Aggregate {
                                kind: AggregateKind::Adt {
                                    adt,
                                    variant: v_idx,
                                    args,
                                },
                                operands: ops,
                            },
                            span,
                        );
                    }
                    TyKind::Unit
                    | TyKind::Bool
                    | TyKind::Never
                    | TyKind::Int(_)
                    | TyKind::Float(_)
                    | TyKind::Slice(_)
                    | TyKind::Ref { .. }
                    | TyKind::Managed(_)
                    | TyKind::Desc(_)
                    | TyKind::ErrorUnion { .. }
                    | TyKind::FnPtr { .. }
                    | TyKind::Closure { .. }
                    | TyKind::Dyn { .. }
                    | TyKind::OwnedDyn { .. }
                    | TyKind::Param(_)
                    | TyKind::Opaque(_) => {
                        self.failed = true;
                    }
                }
            }
            CtValue::ErrorUnion { ok, payload } => {
                let TyKind::ErrorUnion {
                    ok: ok_ty,
                    err: err_ty,
                } = self.tys().kind(ty).clone()
                else {
                    self.failed = true;
                    return;
                };
                let arm_ty = if *ok { ok_ty } else { err_ty };
                let op = self.emit_ct_operand(payload, arm_ty, span);
                self.assign(
                    dest,
                    Rvalue::ErrorUnion {
                        ok: *ok,
                        value: op,
                        ty,
                    },
                    span,
                );
            }
        }
    }

    fn emit_ct_operand(&mut self, val: &CtValue, ty: TyId, span: Span) -> Operand {
        match val {
            CtValue::Unit => Operand::Const(ConstValue::Unit),
            CtValue::Bool(b) => Operand::Const(ConstValue::Bool(*b)),
            CtValue::Int { ty: ity, bits } => Operand::Const(ConstValue::Int {
                ty: *ity,
                bits: *bits,
            }),
            CtValue::Float { ty: fty, bits } => Operand::Const(ConstValue::Float {
                ty: *fty,
                bits: *bits,
            }),
            CtValue::Aggregate { .. } | CtValue::ErrorUnion { .. } => {
                let t = self.temp(ty);
                self.emit_ct_value(Place::local(t), val, ty, span);
                self.use_place(Place::local(t), ty)
            }
        }
    }

    /// Evaluates a boolean and returns the (true, false) successor blocks.
    fn branch(&mut self, cond: &Expr) -> (BlockId, BlockId) {
        let c = self.operand(cond);
        let t = self.new_block(cond.span);
        let f = self.new_block(cond.span);
        self.terminate(
            Terminator::Switch {
                discr: c,
                arms: vec![(0, f)],
                otherwise: t,
            },
            cond.span,
        );
        (t, f)
    }

    // ---- statements --------------------------------------------------------

    fn effect(&mut self, e: &Expr) {
        let span = e.span;
        match &e.kind {
            ExprKind::Block(stmts) => {
                self.enter_frame();
                for s in stmts {
                    match s {
                        Stmt::Let { pattern, init } => self.let_stmt(pattern, init),
                        Stmt::Expr(x) => self.effect(x),
                        Stmt::Defer(x) => {
                            if let Some(f) = self.frames.last_mut() {
                                f.cleanups.push(FrameCleanup::Defer(x.clone()));
                            }
                        }
                    }
                }
                self.exit_frame(span);
            }
            ExprKind::If {
                cond,
                then,
                otherwise,
            } => {
                let (t, f) = self.branch(cond);
                let join = self.new_block(span);
                self.cur = t;
                self.effect(then);
                self.goto(join, span);
                self.cur = f;
                if let Some(o) = otherwise {
                    self.effect(o);
                }
                self.goto(join, span);
                self.cur = join;
            }
            ExprKind::Match { scrutinee, arms } => self.lower_match(scrutinee, arms, None, span),
            ExprKind::Loop(body) => {
                let head = self.new_block(span);
                let exit = self.new_block(span);
                self.goto(head, span);
                self.cur = head;
                self.loops.push(LoopFrame {
                    break_to: exit,
                    continue_to: head,
                    depth: self.frames.len(),
                });
                self.effect(body);
                self.loops.pop();
                self.goto(head, span);
                self.cur = exit;
            }
            ExprKind::While { cond, body } => {
                let head = self.new_block(span);
                let exit = self.new_block(span);
                self.goto(head, span);
                self.cur = head;
                let (t, f) = self.branch(cond);
                self.cur = f;
                self.goto(exit, span);
                self.cur = t;
                self.loops.push(LoopFrame {
                    break_to: exit,
                    continue_to: head,
                    depth: self.frames.len(),
                });
                self.effect(body);
                self.loops.pop();
                self.goto(head, span);
                self.cur = exit;
            }
            ExprKind::Trap => {
                self.terminate(Terminator::Trap(zil_ir::body::TrapKind::Explicit), span);
            }
            ExprKind::Break | ExprKind::Continue => {
                let Some(l) = self.loops.last() else {
                    self.failed = true;
                    return;
                };
                let (depth, target) = if matches!(e.kind, ExprKind::Break) {
                    (l.depth, l.break_to)
                } else {
                    (l.depth, l.continue_to)
                };
                self.unwind_to(depth, span);
                self.goto(target, span);
            }
            ExprKind::Return(value) => {
                if let (Some(v), Some(r)) = (value, self.ret) {
                    self.into(Place::local(r), v);
                }
                self.unwind_to(0, span);
                let operand = match self.ret {
                    Some(r) => {
                        let ty = self.sig.result;
                        self.use_place(Place::local(r), ty)
                    }
                    None => Operand::Const(ConstValue::Unit),
                };
                self.terminate(Terminator::Return { value: operand }, span);
            }
            ExprKind::Assign { place, op, value } => {
                let ty = self.ty(place.ty, place.span);
                let p = self.place(place);
                let rv = match op {
                    Some(op) => {
                        let rhs = self.operand(value);
                        Rvalue::Binary {
                            op: *op,
                            lhs: Operand::Copy(p.clone()),
                            rhs,
                        }
                    }
                    None => self.rvalue(value),
                };
                if !self.needs_cleanup(ty) {
                    self.assign(p, rv, span);
                } else if p.first_deref().is_some() {
                    let v = self.temp(ty);
                    self.assign(Place::local(v), rv, span);
                    let old = self.temp(ty);
                    self.push(
                        InstructionKind::Replace {
                            place: p,
                            value: Operand::Move(Place::local(v)),
                            old,
                        },
                        span,
                    );
                    self.push(
                        InstructionKind::Drop {
                            place: Place::local(old),
                        },
                        span,
                    );
                } else {
                    let v = self.temp(ty);
                    self.assign(Place::local(v), rv, span);
                    let idx = self.blocks[self.cur.index()].instructions.len();
                    self.push(InstructionKind::Drop { place: p.clone() }, span);
                    self.auto_drops.insert((self.cur, idx));
                    self.assign(p, Rvalue::Use(Operand::Move(Place::local(v))), span);
                }
            }
            ExprKind::Lit(_)
            | ExprKind::ConstGeneric(_)
            | ExprKind::StaticUtf(_)
            | ExprKind::Local(_)
            | ExprKind::Capture(_) => {}
            ExprKind::Call { .. }
            | ExprKind::IndirectCall { .. }
            | ExprKind::Closure { .. }
            | ExprKind::ClosureFnRef(_)
            | ExprKind::ClosureCall { .. }
            | ExprKind::TraitCall { .. }
            | ExprKind::DynCall { .. }
            | ExprKind::FnRef { .. }
            | ExprKind::ForeignRef(_)
            | ExprKind::ForeignCall { .. }
            | ExprKind::Intrinsic { .. }
            | ExprKind::ManagedCreate { .. }
            | ExprKind::Try(_)
            | ExprKind::CtEval(_)
            | ExprKind::Logical { .. }
            | ExprKind::Field { .. }
            | ExprKind::TupleField { .. }
            | ExprKind::Index { .. }
            | ExprKind::Deref(_)
            | ExprKind::Borrow { .. }
            | ExprKind::SliceBorrow { .. }
            | ExprKind::Len(_)
            | ExprKind::Give(_)
            | ExprKind::Describe(_)
            | ExprKind::ErrorInject { .. }
            | ExprKind::UnionInject { .. }
            | ExprKind::Unary { .. }
            | ExprKind::Binary { .. }
            | ExprKind::Convert { .. }
            | ExprKind::Adt { .. }
            | ExprKind::Tuple(_)
            | ExprKind::Array(_) => {
                // Evaluated for its side effects/checks; the value is discarded.
                let ty = self.ty(e.ty, span);
                let t = self.temp(ty);
                self.into(Place::local(t), e);
                if self.needs_cleanup(ty) {
                    self.push(
                        InstructionKind::Drop {
                            place: Place::local(t),
                        },
                        span,
                    );
                }
            }
        }
    }

    fn let_stmt(&mut self, pattern: &Pat, init: &Expr) {
        if let PatKind::Binding(l) = pattern.kind {
            let info = self.locals_info[l.0 as usize].clone();
            let local = self.bind_local(l, &info);
            self.declare(local, pattern.span);
            self.into(Place::local(local), init);
            return;
        }
        let source = self.place(init);
        self.bind_irrefutable(pattern, source);
    }

    fn bind_irrefutable(&mut self, pattern: &Pat, source: Place) {
        let rows = vec![Row {
            cols: vec![(source, pattern)],
            bindings: Vec::new(),
            arm: 0,
        }];
        // An irrefutable pattern simplifies to bindings only.
        let mut rows = rows;
        self.simplify(&mut rows);
        let row = rows.pop();
        if let Some(row) = row {
            for (local, place, ref_mut) in row.bindings {
                self.declare(local, pattern.span);
                let rv = match ref_mut {
                    Some(mutability) => Rvalue::Ref {
                        region: Region::Static,
                        mutability,
                        place,
                    },
                    None => {
                        let ty = self.locals[local.index()].ty;
                        Rvalue::Use(self.use_place(place, ty))
                    }
                };
                self.assign(Place::local(local), rv, pattern.span);
            }
        }
    }

    // ---- match -------------------------------------------------------------

    fn lower_match(&mut self, scrutinee: &Expr, arms: &[Arm], dest: Option<Place>, span: Span) {
        self.enter_frame();
        let source = self.place(scrutinee);
        let match_temps = self
            .frames
            .last()
            .map(|f| f.temps.clone())
            .unwrap_or_default();
        let arm_blocks: Vec<BlockId> = arms.iter().map(|a| self.new_block(a.span)).collect();
        let join = self.new_block(span);
        let rows: Vec<Row<'_>> = arms
            .iter()
            .enumerate()
            .map(|(i, a)| Row {
                cols: vec![(source.clone(), &a.pattern)],
                bindings: Vec::new(),
                arm: i,
            })
            .collect();
        let mut arm_bindings: Vec<Vec<Local>> = vec![Vec::new(); arms.len()];
        self.decide(rows, arms, &arm_blocks, &mut arm_bindings, span);
        self.frames.pop();
        for (i, arm) in arms.iter().enumerate() {
            self.cur = arm_blocks[i];
            for t in match_temps.iter().rev().copied() {
                self.push_auto_drop(t, arm.span);
                self.push(InstructionKind::StorageDead(t), arm.span);
            }
            self.enter_frame();
            if let Some(f) = self.frames.last_mut() {
                f.cleanups = arm_bindings[i]
                    .iter()
                    .rev()
                    .copied()
                    .map(FrameCleanup::Local)
                    .collect();
            }
            match &dest {
                Some(d) => self.into(d.clone(), &arm.body),
                None => self.effect(&arm.body),
            }
            self.exit_frame(arm.span);
            self.goto(join, arm.span);
        }
        self.cur = join;
    }

    /// Expands irrefutable structure: wildcards vanish, bindings are
    /// recorded, tuples and single-variant records become sub-columns.
    fn simplify<'t>(&mut self, rows: &mut [Row<'t>]) {
        for row in rows.iter_mut() {
            let mut out = Vec::new();
            let mut work: VecDeque<(Place, &'t Pat)> = row.cols.drain(..).collect();
            while let Some((place, pat)) = work.pop_front() {
                match &pat.kind {
                    PatKind::Wild => {}
                    PatKind::Binding(l) => {
                        let local = match self.map[l.0 as usize] {
                            Some(local) => local,
                            None => {
                                let info = self.locals_info[l.0 as usize].clone();
                                self.bind_local(*l, &info)
                            }
                        };
                        row.bindings.push((local, place, None));
                    }
                    PatKind::RefBinding { local: l, mutable } => {
                        let local = match self.map[l.0 as usize] {
                            Some(local) => local,
                            None => {
                                let info = self.locals_info[l.0 as usize].clone();
                                self.bind_local(*l, &info)
                            }
                        };
                        let m = if *mutable {
                            Mutability::Exclusive
                        } else {
                            Mutability::Shared
                        };
                        row.bindings.push((local, place, Some(m)));
                    }
                    PatKind::Deref(inner) => {
                        let mut sub = place.clone();
                        sub.projections.push(Projection::Deref);
                        work.push_back((sub, inner.as_ref()));
                    }
                    PatKind::Tuple(items) => {
                        for (i, p) in items.iter().enumerate() {
                            let mut sub = place.clone();
                            sub.projections.push(Projection::TupleField(i as u32));
                            work.push_back((sub, p));
                        }
                    }
                    PatKind::Adt {
                        adt,
                        variant,
                        args,
                        fields,
                    } if self.mono.items.adt(*adt).kind != AdtKind::Enum => {
                        for (i, p) in fields.iter().enumerate() {
                            let mut sub = place.clone();
                            match self.field_ref(&place, *adt, *variant, i as u32, args, pat.span) {
                                Some(f) => sub.projections.push(Projection::Field(f)),
                                None => self.failed = true,
                            }
                            work.push_back((sub, p));
                        }
                    }
                    PatKind::Lit(_)
                    | PatKind::Adt { .. }
                    | PatKind::ErrorArm { .. }
                    | PatKind::UnionArm { .. } => out.push((place, pat)),
                }
            }
            row.cols = out;
        }
    }

    fn decide<'t>(
        &mut self,
        mut rows: Vec<Row<'t>>,
        match_arms: &[Arm],
        arm_blocks: &[BlockId],
        arm_bindings: &mut [Vec<Local>],
        span: Span,
    ) {
        self.simplify(&mut rows);
        let Some(first) = rows.first().cloned() else {
            if !self.reported_exhaustive {
                self.reported_exhaustive = true;
                self.mono.diags.push(diag::error(
                    diag::MATCH_NONEXHAUSTIVE,
                    span,
                    "the patterns do not cover every value",
                ));
            }
            self.failed = true;
            self.terminate(Terminator::Unreachable, span);
            return;
        };
        let Some((test_place, test_pat)) = first.cols.first().cloned() else {
            // Leaf: bind and enter the arm (or evaluate guard and fall through on false).
            if let Some(guard) = &match_arms[first.arm].guard {
                let mut guard_ref_map: HashMap<Local, Local> = HashMap::new();
                for &(arm_lid, guard_lid) in &match_arms[first.arm].guard_refs {
                    let arm_mir = self.local(arm_lid);
                    let guard_mir = match self.map[guard_lid.0 as usize] {
                        Some(local) => local,
                        None => {
                            let info = self.locals_info[guard_lid.0 as usize].clone();
                            self.bind_local(guard_lid, &info)
                        }
                    };
                    guard_ref_map.insert(arm_mir, guard_mir);
                }

                // 1. Bind guard-scope locals without moving non-Copy values or
                //    creating exclusive borrows of the scrutinee.
                let mut guard_only_locals = Vec::new();
                let mut copy_bound_locals = Vec::new();
                let mut deferred_arm_bindings = Vec::new();
                for (local, place, ref_mut) in first.bindings {
                    if !arm_bindings[first.arm].contains(&local) {
                        arm_bindings[first.arm].push(local);
                    }
                    if let Some(&guard_local) = guard_ref_map.get(&local) {
                        self.push(InstructionKind::StorageLive(guard_local), span);
                        guard_only_locals.push(guard_local);
                        self.assign(
                            Place::local(guard_local),
                            Rvalue::Ref {
                                region: Region::Static,
                                mutability: Mutability::Shared,
                                place: place.clone(),
                            },
                            span,
                        );
                        deferred_arm_bindings.push((local, place, ref_mut));
                    } else {
                        self.push(InstructionKind::StorageLive(local), span);
                        copy_bound_locals.push(local);
                        let rv = match ref_mut {
                            Some(mutability) => Rvalue::Ref {
                                region: Region::Static,
                                mutability,
                                place,
                            },
                            None => {
                                let ty = self.locals[local.index()].ty;
                                Rvalue::Use(self.use_place(place, ty))
                            }
                        };
                        self.assign(Place::local(local), rv, span);
                    }
                }

                // 2. Evaluate the guard in its own temp frame so any temporaries
                //    created during guard evaluation are cleaned up on both branches.
                self.enter_frame();
                let cond = self.operand(guard);
                let guard_temps = self.frames.pop().map(|f| f.temps).unwrap_or_default();

                let then_bb = self.new_block(guard.span);
                let else_bb = self.new_block(guard.span);
                self.terminate(
                    Terminator::Switch {
                        discr: cond,
                        arms: vec![(1, then_bb)],
                        otherwise: else_bb,
                    },
                    guard.span,
                );

                // 3. True branch: clean up guard temps, end guard-only shared
                //    borrows, then perform the real by-value / ref-mut bindings
                //    and enter the arm body.
                self.cur = then_bb;
                for t in guard_temps.iter().rev().copied() {
                    self.push_auto_drop(t, guard.span);
                    self.push(InstructionKind::StorageDead(t), guard.span);
                }
                for local in guard_only_locals.iter().rev().copied() {
                    self.push(InstructionKind::StorageDead(local), guard.span);
                }
                for (local, place, ref_mut) in deferred_arm_bindings {
                    self.push(InstructionKind::StorageLive(local), span);
                    let rv = match ref_mut {
                        Some(mutability) => Rvalue::Ref {
                            region: Region::Static,
                            mutability,
                            place,
                        },
                        None => {
                            let ty = self.locals[local.index()].ty;
                            Rvalue::Use(self.use_place(place, ty))
                        }
                    };
                    self.assign(Place::local(local), rv, span);
                }
                self.goto(arm_blocks[first.arm], span);

                // 4. False branch: clean up guard temps, guard-only shared
                //    borrows, and any Copy bindings created for the guard,
                //    leaving the scrutinee intact for subsequent arms.
                self.cur = else_bb;
                for t in guard_temps.into_iter().rev() {
                    self.push_auto_drop(t, guard.span);
                    self.push(InstructionKind::StorageDead(t), guard.span);
                }
                for local in guard_only_locals.into_iter().rev() {
                    self.push(InstructionKind::StorageDead(local), guard.span);
                }
                for local in copy_bound_locals.into_iter().rev() {
                    self.push(InstructionKind::StorageDead(local), guard.span);
                }
                self.decide(
                    rows[1..].to_vec(),
                    match_arms,
                    arm_blocks,
                    arm_bindings,
                    span,
                );
            } else {
                for (local, place, ref_mut) in first.bindings {
                    self.push(InstructionKind::StorageLive(local), span);
                    if !arm_bindings[first.arm].contains(&local) {
                        arm_bindings[first.arm].push(local);
                    }
                    let rv = match ref_mut {
                        Some(mutability) => Rvalue::Ref {
                            region: Region::Static,
                            mutability,
                            place,
                        },
                        None => {
                            let ty = self.locals[local.index()].ty;
                            Rvalue::Use(self.use_place(place, ty))
                        }
                    };
                    self.assign(Place::local(local), rv, span);
                }
                self.goto(arm_blocks[first.arm], span);
            }
            return;
        };
        match &test_pat.kind {
            PatKind::Adt { adt, args, .. } => {
                let adt = *adt;
                let args = args.clone();
                let count = self.mono.items.adt(adt).variants.len() as u32;
                let i32_ty = self.mono.module.tys.intern(TyKind::Int(IntTy::I32));
                let tag = self.temp(i32_ty);
                self.assign(
                    Place::local(tag),
                    Rvalue::Discriminant(test_place.clone()),
                    span,
                );
                let mut targets = Vec::new();
                let mut specialized = Vec::new();
                for v in 0..count {
                    let mut sub_rows = Vec::new();
                    for row in &rows {
                        let mut row = row.clone();
                        match row.cols.iter().position(|(p, _)| *p == test_place) {
                            None => sub_rows.push(row),
                            Some(k) => {
                                let (_, pat) = row.cols.remove(k);
                                if let PatKind::Adt {
                                    variant, fields, ..
                                } = &pat.kind
                                    && *variant == v
                                {
                                    for (i, fp) in fields.iter().enumerate() {
                                        let mut sub = test_place.clone();
                                        match self.field_ref(
                                            &test_place,
                                            adt,
                                            v,
                                            i as u32,
                                            &args,
                                            span,
                                        ) {
                                            Some(f) => sub.projections.push(Projection::Field(f)),
                                            None => self.failed = true,
                                        }
                                        row.cols.insert(k + i, (sub, fp));
                                    }
                                    sub_rows.push(row);
                                }
                            }
                        }
                    }
                    let bb = self.new_block(span);
                    targets.push((u128::from(v), bb));
                    specialized.push((bb, sub_rows));
                }
                let unreachable = self.new_block(span);
                self.terminate(
                    Terminator::Switch {
                        discr: Operand::Copy(Place::local(tag)),
                        arms: targets,
                        otherwise: unreachable,
                    },
                    span,
                );
                for (bb, sub_rows) in specialized {
                    self.cur = bb;
                    self.decide(sub_rows, match_arms, arm_blocks, arm_bindings, span);
                }
            }
            PatKind::Lit(_) => {
                let ty = self.ty(test_pat.ty, test_pat.span);
                let is_bool = matches!(self.tys().kind(ty), TyKind::Bool);
                if matches!(self.tys().kind(ty), TyKind::Float(_)) {
                    self.mono
                        .diags
                        .push(diag::unsupported("float literal patterns", test_pat.span));
                    self.failed = true;
                    return;
                }
                let mut values: Vec<u128> = Vec::new();
                for row in &rows {
                    for (p, pat) in &row.cols {
                        if *p == test_place
                            && let PatKind::Lit(l) = &pat.kind
                        {
                            let v = self.lit_bits(*l, ty);
                            if !values.contains(&v) {
                                values.push(v);
                            }
                        }
                    }
                }
                let specialize = |rows: &[Row<'t>], value: Option<u128>, b: &Self| {
                    rows.iter()
                        .filter_map(|row| {
                            let mut row = row.clone();
                            match row.cols.iter().position(|(p, _)| *p == test_place) {
                                None => Some(row),
                                Some(k) => {
                                    let (_, pat) = row.cols.remove(k);
                                    match (&pat.kind, value) {
                                        (PatKind::Lit(l), Some(v)) if b.lit_bits(*l, ty) == v => {
                                            Some(row)
                                        }
                                        _ => None,
                                    }
                                }
                            }
                        })
                        .collect::<Vec<_>>()
                };
                let mut arms = Vec::new();
                let mut pending = Vec::new();
                let both_bools = is_bool && values.contains(&0) && values.contains(&1);
                let listed: Vec<u128> = if both_bools { vec![1] } else { values.clone() };
                for v in &listed {
                    let bb = self.new_block(span);
                    arms.push((*v, bb));
                    pending.push((bb, specialize(&rows, Some(*v), self)));
                }
                let otherwise = self.new_block(span);
                let default_rows = if both_bools {
                    specialize(&rows, Some(0), self)
                } else {
                    specialize(&rows, None, self)
                };
                pending.push((otherwise, default_rows));
                self.terminate(
                    Terminator::Switch {
                        discr: Operand::Copy(test_place.clone()),
                        arms,
                        otherwise,
                    },
                    span,
                );
                for (bb, sub_rows) in pending {
                    self.cur = bb;
                    self.decide(sub_rows, match_arms, arm_blocks, arm_bindings, span);
                }
            }
            PatKind::ErrorArm { .. } => {
                let bool_ty = self.mono.module.tys.intern(TyKind::Bool);
                let tag = self.temp(bool_ty);
                self.assign(
                    Place::local(tag),
                    Rvalue::ErrorArm(test_place.clone()),
                    span,
                );
                let mut targets = Vec::new();
                let mut specialized = Vec::new();
                for arm_ok in [false, true] {
                    let mut sub_rows = Vec::new();
                    for row in &rows {
                        let mut row = row.clone();
                        match row.cols.iter().position(|(p, _)| *p == test_place) {
                            None => sub_rows.push(row),
                            Some(k) => {
                                let (_, pat) = row.cols.remove(k);
                                if let PatKind::ErrorArm { ok, inner } = &pat.kind
                                    && *ok == arm_ok
                                {
                                    let sub =
                                        test_place.clone().project(Projection::ErrorArm(arm_ok));
                                    row.cols.insert(k, (sub, inner.as_ref()));
                                    sub_rows.push(row);
                                }
                            }
                        }
                    }
                    let bb = self.new_block(span);
                    targets.push((u128::from(arm_ok), bb));
                    specialized.push((bb, sub_rows));
                }
                let unreachable = self.new_block(span);
                self.terminate(
                    Terminator::Switch {
                        discr: Operand::Copy(Place::local(tag)),
                        arms: targets,
                        otherwise: unreachable,
                    },
                    span,
                );
                for (bb, sub_rows) in specialized {
                    self.cur = bb;
                    self.decide(sub_rows, match_arms, arm_blocks, arm_bindings, span);
                }
            }
            PatKind::UnionArm { .. } => {
                let union_ty = self.closed(test_pat.ty);
                let TyData::Union(members) = self.mono.types.data(union_ty).clone() else {
                    self.failed = true;
                    self.terminate(Terminator::Unreachable, span);
                    return;
                };
                let Some((adt, member_ids)) = self.mono.union_adt(&members, span) else {
                    self.failed = true;
                    self.terminate(Terminator::Unreachable, span);
                    return;
                };
                let Some(empty_args) = self.mono.args_id(&[], span) else {
                    self.failed = true;
                    self.terminate(Terminator::Unreachable, span);
                    return;
                };
                let count = member_ids.len() as u32;
                let i32_ty = self.mono.module.tys.intern(TyKind::Int(IntTy::I32));
                let tag = self.temp(i32_ty);
                self.assign(
                    Place::local(tag),
                    Rvalue::Discriminant(test_place.clone()),
                    span,
                );
                let mut targets = Vec::new();
                let mut specialized = Vec::new();
                for v in 0..count {
                    let expected_member_id = member_ids[v as usize];
                    let mut sub_rows = Vec::new();
                    for row in &rows {
                        let mut row = row.clone();
                        match row.cols.iter().position(|(p, _)| *p == test_place) {
                            None => sub_rows.push(row),
                            Some(k) => {
                                let (_, pat) = row.cols.remove(k);
                                if let PatKind::UnionArm { member_ty, inner } = &pat.kind {
                                    let m_id = self.ty(*member_ty, pat.span);
                                    if m_id == expected_member_id {
                                        let mut sub = test_place.clone();
                                        sub.projections.push(Projection::Field(FieldRef {
                                            adt,
                                            variant: VariantIdx(v),
                                            field: FieldIdx(0),
                                            args: empty_args,
                                        }));
                                        row.cols.insert(k, (sub, inner.as_ref()));
                                        sub_rows.push(row);
                                    }
                                }
                            }
                        }
                    }
                    let bb = self.new_block(span);
                    targets.push((u128::from(v), bb));
                    specialized.push((bb, sub_rows));
                }
                let unreachable = self.new_block(span);
                self.terminate(
                    Terminator::Switch {
                        discr: Operand::Copy(Place::local(tag)),
                        arms: targets,
                        otherwise: unreachable,
                    },
                    span,
                );
                for (bb, sub_rows) in specialized {
                    self.cur = bb;
                    self.decide(sub_rows, match_arms, arm_blocks, arm_bindings, span);
                }
            }
            PatKind::Wild
            | PatKind::Binding(_)
            | PatKind::RefBinding { .. }
            | PatKind::Deref(_)
            | PatKind::Tuple(_) => {
                // `simplify` removed these columns.
                self.failed = true;
            }
        }
    }

    fn lit_bits(&self, lit: Lit, ty: TyId) -> u128 {
        match (lit, self.tys().kind(ty)) {
            (Lit::Bool(b), _) => u128::from(b),
            (Lit::Int(v), TyKind::Int(t)) => literal::int_bits(v, *t, POINTER_BITS),
            (Lit::Unit | Lit::Int(_) | Lit::Float(_), _) => 0,
        }
    }
}

/// Drops blocks unreachable from the entry and renumbers the rest.
fn prune(blocks: Vec<Block>) -> (Vec<Block>, Vec<BlockId>) {
    if blocks.is_empty() {
        return (blocks, Vec::new());
    }
    let mut reachable = vec![false; blocks.len()];
    let mut stack = vec![0usize];
    while let Some(b) = stack.pop() {
        if std::mem::replace(&mut reachable[b], true) {
            continue;
        }
        for (_, t) in blocks[b].terminator.edges() {
            if !reachable[t.index()] {
                stack.push(t.index());
            }
        }
    }
    let mut remap = vec![BlockId(u32::MAX); blocks.len()];
    let mut n = 0;
    for (i, r) in reachable.iter().enumerate() {
        if *r {
            remap[i] = BlockId::from_index(n);
            n += 1;
        }
    }
    let map = |b: BlockId| remap[b.index()];
    let pruned = blocks
        .into_iter()
        .enumerate()
        .filter(|(i, _)| reachable[*i])
        .map(|(_, mut block)| {
            block.terminator = match block.terminator {
                Terminator::Goto { target } => Terminator::Goto {
                    target: map(target),
                },
                Terminator::Switch {
                    discr,
                    arms,
                    otherwise,
                } => Terminator::Switch {
                    discr,
                    arms: arms.into_iter().map(|(v, b)| (v, map(b))).collect(),
                    otherwise: map(otherwise),
                },
                Terminator::Call {
                    callee,
                    args,
                    dest,
                    target,
                    cancel,
                } => Terminator::Call {
                    callee,
                    args,
                    dest,
                    target: map(target),
                    cancel: cancel.map(map),
                },
                Terminator::Assert {
                    cond,
                    expected,
                    target,
                    trap,
                } => Terminator::Assert {
                    cond,
                    expected,
                    target: map(target),
                    trap,
                },
                t @ (Terminator::Return { .. } | Terminator::Unreachable | Terminator::Trap(_)) => {
                    t
                }
            };
            block
        })
        .collect();
    (pruned, remap)
}

fn build_fn_source_map(
    function: FnId,
    decl_span: Span,
    body: &Body,
    eval_rels: Vec<SourceRelation>,
) -> FunctionSourceMap {
    let mut relations = eval_rels;
    let mut spans = Vec::new();
    let decl_start = u32::try_from(decl_span.start).unwrap_or(u32::MAX);
    for (idx, decl) in body.locals.iter().enumerate() {
        if decl.name.is_some() {
            relations.push(SourceRelation::Binding {
                node: NodeId(decl_start.saturating_add(idx as u32)),
                local: Local::from_index(idx),
            });
        }
    }
    for (bi, block) in body.blocks.iter().enumerate() {
        let block_id = BlockId::from_index(bi);
        for (ii, inst) in block.instructions.iter().enumerate() {
            let at = OpLocation {
                block: block_id,
                instruction: Some(ii as u32),
            };
            spans.push((at, inst.span));
            let inst_start = u32::try_from(inst.span.start).unwrap_or(u32::MAX);
            for access in inst.kind.accesses() {
                for proj in &access.place.projections {
                    if let Projection::Field(field) = *proj {
                        relations.push(SourceRelation::FieldSelect {
                            node: NodeId(inst_start),
                            field,
                            at,
                        });
                    }
                }
            }
            if matches!(inst.kind, InstructionKind::Drop { .. }) {
                relations.push(SourceRelation::ResourceOp {
                    node: NodeId(inst_start),
                    at,
                });
            }
        }
        let term_at = OpLocation {
            block: block_id,
            instruction: None,
        };
        spans.push((term_at, block.terminator_span));
        let term_start = u32::try_from(block.terminator_span.start).unwrap_or(u32::MAX);
        for access in block.terminator.accesses() {
            for proj in &access.place.projections {
                if let Projection::Field(field) = *proj {
                    relations.push(SourceRelation::FieldSelect {
                        node: NodeId(term_start),
                        field,
                        at: term_at,
                    });
                }
            }
        }
        if matches!(
            block.terminator,
            Terminator::Call {
                callee: Callee::Intrinsic(_),
                ..
            }
        ) {
            relations.push(SourceRelation::ResourceOp {
                node: NodeId(term_start),
                at: term_at,
            });
        }
    }
    FunctionSourceMap {
        function,
        declaration: NodeId(decl_start),
        relations,
        spans,
    }
}

/// Evaluates a compile-time expression of type `Type` through the full
/// `zil-solve -> zil-check -> zil-interp` pipeline and recovers its interned
/// `Ty` identity.
#[allow(clippy::too_many_arguments)]
pub fn eval_ct_type_expr<'a>(
    items: &Items<'a>,
    types: &mut Types,
    thir: &[Option<ThirFn>],
    owner_fn: FnRef,
    locals: &[LocalInfo],
    inner: &Expr,
    diags: &mut Vec<Diagnostic>,
    limits: Limits,
    ct: &mut dyn ConstEvaluator,
    span: Span,
) -> Option<Ty> {
    let mut ct_mono = Mono::new(items, types, thir, diags, limits, None);
    ct_mono.is_ct_eval = true;
    ct_mono.lower_foreigns();
    let result = ct_mono.lower_ty(inner.ty, span)?;
    let sig = FnSig {
        params: Vec::new(),
        result,
        type_params: 0,
        region_params: 0,
        role: FnRole::Normal,
        resume_mode: ResumeMode::Local,
        state_contracts: Vec::new(),
    };
    let synthetic_thir = ThirFn {
        locals: locals.to_vec(),
        params: Vec::new(),
        body: Expr {
            kind: ExprKind::Return(Some(Box::new(inner.clone()))),
            ty: inner.ty,
            span,
        },
        closures: Vec::new(),
        opaque_return: None,
    };
    let mut b = Builder::new(
        &mut ct_mono,
        owner_fn,
        &synthetic_thir,
        Vec::new(),
        sig.clone(),
    );
    b.function(&synthetic_thir);
    let failed = b.failed;
    let (body, _) = b.finish();
    if failed {
        return None;
    }
    let entry = FnId::from_index(ct_mono.module.functions.len());
    ct_mono.module.functions.push(Function {
        name: "$ct_type_eval".to_string(),
        sig,
        body,
    });
    ct_mono.run();
    let ct_module = std::mem::take(&mut ct_mono.module);
    drop(ct_mono);
    match ct.eval(ct_module, entry, span) {
        Ok(CtValue::Int {
            ty: IntTy::U32,
            bits,
        }) => {
            let idx = bits as usize;
            if idx < types.len() {
                Some(Ty(idx as u32))
            } else {
                diags.push(diag::error(
                    diag::TYPE_INFERENCE,
                    span,
                    "compile-time Type evaluation returned an invalid type identity",
                ));
                None
            }
        }
        Ok(_) => {
            diags.push(diag::error(
                diag::TYPE_MISMATCH,
                span,
                "compile-time type expression `#(...)` must evaluate to a `Type` value",
            ));
            None
        }
        Err(d) => {
            diags.push(d);
            None
        }
    }
}

/// Formats a deterministic, order-independent, bounded-length canonical tag for
/// a generic function's concrete type arguments (e.g. `::<g0.Rec0__h0123456789abcdef>`).
///
/// The 64-bit SHA-256 suffix is computed over the COMPLETE untruncated type
/// structure (never stopping at depth 16); only the short human-readable prefix
/// is bounded for symbol compactness.
pub(crate) fn format_mono_args_tag(types: &Types, items: &Items<'_>, args: &[Ty]) -> String {
    let mut hasher = zil_base::FingerprintBuilder::new();
    hasher.update(b"zil-mono-args-v1\0");
    hasher.update(&(args.len() as u64).to_le_bytes());
    for &arg in args {
        hash_canonical_ty(types, items, arg, &mut hasher, 0);
    }
    let hex = hasher.digest().to_hex();

    let mut display = String::with_capacity(64);
    for (idx, &arg) in args.iter().enumerate() {
        if idx > 0 {
            display.push(',');
        }
        format_mono_ty(types, items, arg, &mut display, 0, 8);
    }
    // Keep a short readable prefix (bounded to 40 chars) plus the full-structure
    // 64-bit SHA-256 suffix so LLVM/COFF/ELF symbol names stay compact and collision-free.
    let mut prefix = String::with_capacity(40);
    for ch in display.chars() {
        if prefix.len() >= 40 {
            break;
        }
        if ch.is_ascii_alphanumeric() || ch == '_' || ch == '.' {
            prefix.push(ch);
        } else {
            prefix.push('_');
        }
    }
    format!("::<{prefix}__h{}>", &hex[..16])
}

/// Formats an order-independent canonical symbol label for any concrete type
/// (used for non-ADT trait impl method names so interner `Ty(N)` IDs never leak).
pub(crate) fn format_canonical_ty_label(types: &Types, items: &Items<'_>, ty: Ty) -> String {
    let mut display = String::with_capacity(32);
    format_mono_ty(types, items, ty, &mut display, 0, 8);
    let mut hasher = zil_base::FingerprintBuilder::new();
    hasher.update(b"zil-impl-target-v1\0");
    hash_canonical_ty(types, items, ty, &mut hasher, 0);
    let hex = hasher.digest().to_hex();
    let mut clean = String::with_capacity(display.len().min(32));
    for ch in display.chars() {
        if clean.len() >= 32 {
            break;
        }
        if ch.is_ascii_alphanumeric() || ch == '_' || ch == '.' {
            clean.push(ch);
        } else {
            clean.push('_');
        }
    }
    if display.len() <= 32
        && display
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.')
    {
        clean
    } else {
        format!("{clean}__h{}", &hex[..12])
    }
}

fn hash_canonical_ty(
    types: &Types,
    items: &Items<'_>,
    ty: Ty,
    h: &mut zil_base::FingerprintBuilder,
    depth: u16,
) {
    // Generous structural ceiling (never truncates normal or deep generic chains).
    if depth >= 1024 {
        h.update(&[0xff]);
        return;
    }
    let next = depth + 1;
    match types.data(ty) {
        TyData::Unit => h.update(&[1]),
        TyData::Bool => h.update(&[2]),
        TyData::Never => h.update(&[3]),
        TyData::Int(i) => {
            h.update(&[4]);
            h.update(format!("{i:?}").as_bytes());
            h.update(&[0]);
        }
        TyData::Float(f) => {
            h.update(&[5]);
            h.update(format!("{f:?}").as_bytes());
            h.update(&[0]);
        }
        TyData::Adt { adt, args, regions } => {
            h.update(&[6]);
            let name = &items.adt(*adt).name;
            h.update(&(name.len() as u32).to_le_bytes());
            h.update(name.as_bytes());
            h.update(&(regions.len() as u32).to_le_bytes());
            for r in regions {
                match r {
                    Some(id) => {
                        h.update(&[1]);
                        h.update(&id.to_le_bytes());
                    }
                    None => h.update(&[0]),
                }
            }
            h.update(&(args.len() as u32).to_le_bytes());
            for a in args {
                hash_canonical_ty(types, items, *a, h, next);
            }
        }
        TyData::Tuple(elems) => {
            h.update(&[7]);
            h.update(&(elems.len() as u32).to_le_bytes());
            for e in elems {
                hash_canonical_ty(types, items, *e, h, next);
            }
        }
        TyData::Union(elems) => {
            h.update(&[8]);
            h.update(&(elems.len() as u32).to_le_bytes());
            for e in elems {
                hash_canonical_ty(types, items, *e, h, next);
            }
        }
        TyData::Array { element, length } => {
            h.update(&[9]);
            h.update(&length.to_le_bytes());
            hash_canonical_ty(types, items, *element, h, next);
        }
        TyData::Slice(elem) => {
            h.update(&[10]);
            hash_canonical_ty(types, items, *elem, h, next);
        }
        TyData::Ref {
            region,
            mutable,
            pointee,
        } => {
            h.update(&[11, u8::from(*mutable)]);
            match region {
                Some(r) => {
                    h.update(&[1]);
                    h.update(&r.to_le_bytes());
                }
                None => h.update(&[0]),
            }
            hash_canonical_ty(types, items, *pointee, h, next);
        }
        TyData::Managed(inner) => {
            h.update(&[12]);
            hash_canonical_ty(types, items, *inner, h, next);
        }
        TyData::Desc(inner) => {
            h.update(&[13]);
            hash_canonical_ty(types, items, *inner, h, next);
        }
        TyData::Opaque(op) => {
            h.update(&[14]);
            match op {
                OpaqueKind::ByteBuffer => h.update(&[1]),
                OpaqueKind::OwnedStore(e) => {
                    h.update(&[2]);
                    hash_canonical_ty(types, items, *e, h, next);
                }
                OpaqueKind::Mutex(e) => {
                    h.update(&[3]);
                    hash_canonical_ty(types, items, *e, h, next);
                }
                OpaqueKind::MutexGuard(e) => {
                    h.update(&[4]);
                    hash_canonical_ty(types, items, *e, h, next);
                }
                OpaqueKind::Task { result, context } => {
                    h.update(&[5]);
                    hash_canonical_ty(types, items, *result, h, next);
                    match context {
                        Some(ctx) => {
                            h.update(&[1]);
                            hash_canonical_ty(types, items, *ctx, h, next);
                        }
                        None => h.update(&[0]),
                    }
                }
                OpaqueKind::Registration(e) => {
                    h.update(&[6]);
                    hash_canonical_ty(types, items, *e, h, next);
                }
                OpaqueKind::AsyncRegistration(e) => {
                    h.update(&[7]);
                    hash_canonical_ty(types, items, *e, h, next);
                }
                OpaqueKind::ForeignHandle => h.update(&[8]),
                OpaqueKind::ForeignCallback { params, result } => {
                    h.update(&[9]);
                    h.update(&(params.len() as u32).to_le_bytes());
                    for p in params {
                        hash_canonical_ty(types, items, *p, h, next);
                    }
                    hash_canonical_ty(types, items, *result, h, next);
                }
                OpaqueKind::SyncCell(k) => {
                    h.update(&[10]);
                    h.update(format!("{k:?}").as_bytes());
                    h.update(&[0]);
                }
            }
        }
        TyData::ErrorUnion { ok, err } => {
            h.update(&[15]);
            hash_canonical_ty(types, items, *err, h, next);
            hash_canonical_ty(types, items, *ok, h, next);
        }
        TyData::FnPtr { params, result } => {
            h.update(&[16]);
            h.update(&(params.len() as u32).to_le_bytes());
            for p in params {
                hash_canonical_ty(types, items, *p, h, next);
            }
            hash_canonical_ty(types, items, *result, h, next);
        }
        TyData::Closure {
            owner,
            id,
            owner_args,
            kind,
            ..
        } => {
            h.update(&[17]);
            let name = &items.func(*owner).name;
            h.update(&(name.len() as u32).to_le_bytes());
            h.update(name.as_bytes());
            h.update(&id.0.to_le_bytes());
            h.update(format!("{kind:?}").as_bytes());
            h.update(&[0]);
            h.update(&(owner_args.len() as u32).to_le_bytes());
            for a in owner_args {
                hash_canonical_ty(types, items, *a, h, next);
            }
        }
        TyData::ImplCallable {
            owner,
            owner_args,
            kind,
            ..
        } => {
            h.update(&[18]);
            let name = &items.func(*owner).name;
            h.update(&(name.len() as u32).to_le_bytes());
            h.update(name.as_bytes());
            h.update(format!("{kind:?}").as_bytes());
            h.update(&[0]);
            h.update(&(owner_args.len() as u32).to_le_bytes());
            for a in owner_args {
                hash_canonical_ty(types, items, *a, h, next);
            }
        }
        TyData::Dyn {
            trait_ref,
            trait_args,
            ..
        } => {
            h.update(&[19]);
            let name = &items.traits[trait_ref.0 as usize].name;
            h.update(&(name.len() as u32).to_le_bytes());
            h.update(name.as_bytes());
            h.update(&(trait_args.len() as u32).to_le_bytes());
            for a in trait_args {
                hash_canonical_ty(types, items, *a, h, next);
            }
        }
        TyData::OwnedDyn {
            trait_ref,
            trait_args,
            context,
            ..
        } => {
            h.update(&[20]);
            let name = &items.traits[trait_ref.0 as usize].name;
            h.update(&(name.len() as u32).to_le_bytes());
            h.update(name.as_bytes());
            h.update(&(trait_args.len() as u32).to_le_bytes());
            for a in trait_args {
                hash_canonical_ty(types, items, *a, h, next);
            }
            match context {
                Some(ctx) => {
                    h.update(&[1]);
                    hash_canonical_ty(types, items, *ctx, h, next);
                }
                None => h.update(&[0]),
            }
        }
        TyData::Param(p) => {
            h.update(&[21]);
            h.update(&p.to_le_bytes());
        }
        TyData::SelfParam => h.update(&[22]),
        TyData::Assoc { base, name, .. } => {
            h.update(&[23]);
            hash_canonical_ty(types, items, *base, h, next);
            h.update(&(name.len() as u32).to_le_bytes());
            h.update(name.as_bytes());
        }
        TyData::ArrayConst { element, length } => {
            h.update(&[24]);
            hash_canonical_ty(types, items, *element, h, next);
            hash_canonical_ty(types, items, *length, h, next);
        }
        TyData::ConstParam(p) => {
            h.update(&[25]);
            h.update(&p.to_le_bytes());
        }
        TyData::ConstU32(v) => {
            h.update(&[26]);
            h.update(&v.to_le_bytes());
        }
        TyData::Type => h.update(&[27]),
        TyData::Var(v) => {
            h.update(&[28]);
            h.update(format!("{v:?}").as_bytes());
            h.update(&[0]);
        }
        TyData::Error => h.update(&[29]),
    }
}

fn format_mono_ty(
    types: &Types,
    items: &Items<'_>,
    ty: Ty,
    out: &mut String,
    depth: u8,
    max_depth: u8,
) {
    if depth > max_depth {
        out.push_str("...");
        return;
    }
    match types.data(ty) {
        TyData::Unit => out.push_str("unit"),
        TyData::Bool => out.push_str("bool"),
        TyData::Never => out.push_str("never"),
        TyData::Int(i) => out.push_str(&format!("{i:?}")),
        TyData::Float(f) => out.push_str(&format!("{f:?}")),
        TyData::Adt { adt, args, regions } => {
            out.push_str(&items.adt(*adt).name);
            if !args.is_empty() || !regions.is_empty() {
                out.push('<');
                for (i, r) in regions.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    match r {
                        Some(id) => out.push_str(&format!("'r{id}")),
                        None => out.push_str("'_"),
                    }
                }
                for (i, a) in args.iter().enumerate() {
                    if i > 0 || !regions.is_empty() {
                        out.push(',');
                    }
                    format_mono_ty(types, items, *a, out, depth + 1, max_depth);
                }
                out.push('>');
            }
        }
        TyData::Tuple(elems) => {
            out.push_str("tup(");
            for (i, e) in elems.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                format_mono_ty(types, items, *e, out, depth + 1, max_depth);
            }
            out.push(')');
        }
        TyData::Union(elems) => {
            out.push_str("union(");
            for (i, e) in elems.iter().enumerate() {
                if i > 0 {
                    out.push('|');
                }
                format_mono_ty(types, items, *e, out, depth + 1, max_depth);
            }
            out.push(')');
        }
        TyData::Array { element, length } => {
            out.push_str("arr[");
            format_mono_ty(types, items, *element, out, depth + 1, max_depth);
            out.push_str(&format!(";{length}]"));
        }
        TyData::Slice(elem) => {
            out.push_str("slice[");
            format_mono_ty(types, items, *elem, out, depth + 1, max_depth);
            out.push(']');
        }
        TyData::Ref {
            region,
            mutable,
            pointee,
        } => {
            out.push('&');
            if let Some(r) = region {
                out.push_str(&format!("r{r}_"));
            }
            if *mutable {
                out.push_str("mut_");
            }
            format_mono_ty(types, items, *pointee, out, depth + 1, max_depth);
        }
        TyData::Managed(inner) => {
            out.push_str("Managed<");
            format_mono_ty(types, items, *inner, out, depth + 1, max_depth);
            out.push('>');
        }
        TyData::Desc(inner) => {
            out.push_str("Desc<");
            format_mono_ty(types, items, *inner, out, depth + 1, max_depth);
            out.push('>');
        }
        TyData::Opaque(op) => match op {
            OpaqueKind::ByteBuffer => out.push_str("ByteBuffer"),
            OpaqueKind::OwnedStore(e) => {
                out.push_str("OwnedStore<");
                format_mono_ty(types, items, *e, out, depth + 1, max_depth);
                out.push('>');
            }
            OpaqueKind::Mutex(e) => {
                out.push_str("Mutex<");
                format_mono_ty(types, items, *e, out, depth + 1, max_depth);
                out.push('>');
            }
            OpaqueKind::MutexGuard(e) => {
                out.push_str("MutexGuard<");
                format_mono_ty(types, items, *e, out, depth + 1, max_depth);
                out.push('>');
            }
            OpaqueKind::Task { result, context } => {
                out.push_str("Task<");
                format_mono_ty(types, items, *result, out, depth + 1, max_depth);
                if let Some(ctx) = context {
                    out.push(',');
                    format_mono_ty(types, items, *ctx, out, depth + 1, max_depth);
                }
                out.push('>');
            }
            OpaqueKind::Registration(e) => {
                out.push_str("Registration<");
                format_mono_ty(types, items, *e, out, depth + 1, max_depth);
                out.push('>');
            }
            OpaqueKind::AsyncRegistration(e) => {
                out.push_str("AsyncRegistration<");
                format_mono_ty(types, items, *e, out, depth + 1, max_depth);
                out.push('>');
            }
            OpaqueKind::ForeignHandle => out.push_str("ForeignHandle"),
            OpaqueKind::ForeignCallback { params, result } => {
                out.push_str("ForeignCallback(");
                for (i, p) in params.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    format_mono_ty(types, items, *p, out, depth + 1, max_depth);
                }
                out.push_str(")->");
                format_mono_ty(types, items, *result, out, depth + 1, max_depth);
            }
            OpaqueKind::SyncCell(k) => out.push_str(&format!("SyncCell<{k:?}>")),
        },
        TyData::ErrorUnion { ok, err } => {
            out.push_str("err(");
            format_mono_ty(types, items, *err, out, depth + 1, max_depth);
            out.push('!');
            format_mono_ty(types, items, *ok, out, depth + 1, max_depth);
            out.push(')');
        }
        TyData::FnPtr { params, result } => {
            out.push_str("fn(");
            for (i, p) in params.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                format_mono_ty(types, items, *p, out, depth + 1, max_depth);
            }
            out.push_str(")->");
            format_mono_ty(types, items, *result, out, depth + 1, max_depth);
        }
        TyData::Closure {
            owner,
            id,
            owner_args,
            kind,
            ..
        } => {
            out.push_str(&format!(
                "cl({}#{}#{:?}",
                items.func(*owner).name,
                id.0,
                kind
            ));
            for a in owner_args {
                out.push(',');
                format_mono_ty(types, items, *a, out, depth + 1, max_depth);
            }
            out.push(')');
        }
        TyData::ImplCallable {
            owner,
            owner_args,
            kind,
            ..
        } => {
            out.push_str(&format!("impl_fn({}#{:?}", items.func(*owner).name, kind));
            for a in owner_args {
                out.push(',');
                format_mono_ty(types, items, *a, out, depth + 1, max_depth);
            }
            out.push(')');
        }
        TyData::Dyn {
            trait_ref,
            trait_args,
            ..
        } => {
            out.push_str("dyn<");
            out.push_str(&items.traits[trait_ref.0 as usize].name);
            for a in trait_args {
                out.push(',');
                format_mono_ty(types, items, *a, out, depth + 1, max_depth);
            }
            out.push('>');
        }
        TyData::OwnedDyn {
            trait_ref,
            trait_args,
            context,
            ..
        } => {
            out.push_str("OwnedDyn<");
            out.push_str(&items.traits[trait_ref.0 as usize].name);
            for a in trait_args {
                out.push(',');
                format_mono_ty(types, items, *a, out, depth + 1, max_depth);
            }
            if let Some(ctx) = context {
                out.push(';');
                format_mono_ty(types, items, *ctx, out, depth + 1, max_depth);
            }
            out.push('>');
        }
        TyData::Param(p) => out.push_str(&format!("P{p}")),
        TyData::SelfParam => out.push_str("Self"),
        TyData::Assoc { base, name, .. } => {
            format_mono_ty(types, items, *base, out, depth + 1, max_depth);
            out.push('.');
            out.push_str(name);
        }
        TyData::ArrayConst { element, length } => {
            out.push_str("arrc[");
            format_mono_ty(types, items, *element, out, depth + 1, max_depth);
            out.push(';');
            format_mono_ty(types, items, *length, out, depth + 1, max_depth);
            out.push(']');
        }
        TyData::ConstParam(p) => out.push_str(&format!("CP{p}")),
        TyData::ConstU32(v) => out.push_str(&format!("C{v}")),
        TyData::Type => out.push_str("Type"),
        TyData::Var(v) => out.push_str(&format!("{v:?}")),
        TyData::Error => out.push_str("Err"),
    }
}

#[cfg(test)]
mod mono_naming_tests {
    use super::*;
    use crate::ModuleInput;

    #[test]
    fn deep_nested_generics_beyond_depth_16_produce_distinct_mono_tags() {
        let mut types = Types::default();
        let items = Items {
            modules: Vec::new(),
            mod_by_path: HashMap::new(),
            fns: Vec::new(),
            adts: Vec::new(),
            foreigns: Vec::new(),
            traits: Vec::new(),
            trait_impls: Vec::new(),
            inherent_impls: Vec::new(),
            adt_kinds: Vec::new(),
            consts: Vec::new(),
        };
        let mut t_a = types.intern(TyData::Int(IntTy::I32));
        let mut t_b = types.intern(TyData::Int(IntTy::I64));
        for _ in 0..20 {
            t_a = types.intern(TyData::Tuple(vec![t_a]));
            t_b = types.intern(TyData::Tuple(vec![t_b]));
        }
        let tag_a = format_mono_args_tag(&types, &items, &[t_a]);
        let tag_b = format_mono_args_tag(&types, &items, &[t_b]);
        assert_ne!(
            tag_a, tag_b,
            "types differing only at depth 20 must hash to distinct mono tags"
        );
    }

    #[test]
    fn non_adt_trait_impl_uses_canonical_type_name_not_ty_debug_id() {
        let src = r#"
trait Marker {
  fn tag(self: Self) -> i32;
}
impl Marker for i64 {
  fn tag(self: i64) -> i32 { return 7; }
}
impl Marker for bool {
  fn tag(self: bool) -> i32 { return 9; }
}
fn main() -> i32 {
  let x: i64 = 1;
  let b: bool = true;
  return x.tag() + b.tag();
}
"#;
        let unit =
            zil_base::SourceUnit::new(zil_base::FileId(0), zil_base::ModuleId(0), "main.zil", src);
        let ast = zil_syntax::parse(&unit).into_result().expect("parse");
        let lowered = crate::lower_program(
            &[ModuleInput {
                path: "main".to_string(),
                ast: &ast,
            }],
            crate::Limits::default(),
            None,
        )
        .expect("lower");
        let impl_names: Vec<&str> = lowered
            .module
            .functions
            .iter()
            .map(|f| f.name.as_str())
            .filter(|n| n.contains("Marker::tag"))
            .collect();
        assert_eq!(
            impl_names.len(),
            2,
            "expected 2 trait impls: {impl_names:?}"
        );
        for name in &impl_names {
            assert!(
                !name.contains("Ty("),
                "non-ADT trait impl name must not leak internal Ty(N) ID: {name}"
            );
        }
        assert!(
            impl_names.iter().any(|n| n.contains("I64")),
            "expected canonical I64 impl name: {impl_names:?}"
        );
        assert!(
            impl_names.iter().any(|n| n.contains("bool")),
            "expected canonical bool impl name: {impl_names:?}"
        );
    }
}
