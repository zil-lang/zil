//! Drop elaboration: turns lexical-scope auto-drop placeholders into
//! definite `Drop`s, deletes drops of definitely-moved locals, and inserts
//! declared drop flags (`Body.drop_flags`) for maybe-initialized locals.
//!
//! This pass is purely a candidate-IR generator; it grants no safety
//! authority. The semantic kernel and `zil-check` independently verify every
//! `Drop`, every flag update and every exit obligation.

use std::collections::{HashMap, HashSet, VecDeque};
use zil_base::Span;
use zil_ir::body::{
    Block, BlockId, Body, ConstValue, DropFlag, Instruction, InstructionKind, IntTy, Local,
    LocalDecl, Operand, Place, PlaceAccess, Rvalue, Terminator, TyKind, TyTable,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Init {
    maybe_init: bool,
    maybe_uninit: bool,
}

impl Init {
    const INIT: Init = Init {
        maybe_init: true,
        maybe_uninit: false,
    };
    const UNINIT: Init = Init {
        maybe_init: false,
        maybe_uninit: true,
    };
    fn join(self, other: Init) -> Init {
        Init {
            maybe_init: self.maybe_init || other.maybe_init,
            maybe_uninit: self.maybe_uninit || other.maybe_uninit,
        }
    }
    fn is_maybe(self) -> bool {
        self.maybe_init && self.maybe_uninit
    }
    fn is_definitely_uninit(self) -> bool {
        !self.maybe_init && self.maybe_uninit
    }
}

fn step_instruction(
    state: &mut [Init],
    uninit_fields: &mut HashSet<Place>,
    kind: &InstructionKind,
) {
    match kind {
        InstructionKind::StorageLive(l) | InstructionKind::StorageDead(l) => {
            state[l.index()] = Init::UNINIT;
            uninit_fields.retain(|p| p.root != zil_ir::body::PlaceRoot::Local(*l));
            return;
        }
        InstructionKind::ScopeEnter(_) | InstructionKind::ScopeExit { .. } => return,
        InstructionKind::Assign { .. }
        | InstructionKind::Drop { .. }
        | InstructionKind::Replace { .. }
        | InstructionKind::RegisterDefer { .. } => {}
    }
    for a in kind.accesses() {
        if let Some(l) = a.place.as_local() {
            match a.access {
                PlaceAccess::Move | PlaceAccess::Drop => {
                    state[l.index()] = Init::UNINIT;
                    uninit_fields.retain(|p| p.root != zil_ir::body::PlaceRoot::Local(l));
                }
                PlaceAccess::Init | PlaceAccess::ReplaceInstance => {
                    state[l.index()] = Init::INIT;
                    uninit_fields.retain(|p| p.root != zil_ir::body::PlaceRoot::Local(l));
                }
                PlaceAccess::Copy
                | PlaceAccess::Borrow(_)
                | PlaceAccess::Describe
                | PlaceAccess::InspectTag
                | PlaceAccess::InspectLen
                | PlaceAccess::CallClosure(_) => {}
            }
        } else {
            match a.access {
                PlaceAccess::Move
                | PlaceAccess::Drop
                | PlaceAccess::CallClosure(zil_ir::body::ClosureCallMode::Once) => {
                    uninit_fields.insert(a.place.as_ref().clone());
                }
                PlaceAccess::Init | PlaceAccess::ReplaceInstance => {
                    let target = a.place.as_ref();
                    uninit_fields.retain(|p| {
                        !(p.root == target.root
                            && p.projections.len() >= target.projections.len()
                            && p.projections[..target.projections.len()] == target.projections[..])
                    });
                }
                PlaceAccess::Copy
                | PlaceAccess::Borrow(_)
                | PlaceAccess::Describe
                | PlaceAccess::InspectTag
                | PlaceAccess::InspectLen
                | PlaceAccess::CallClosure(
                    zil_ir::body::ClosureCallMode::Shared | zil_ir::body::ClosureCallMode::Mut,
                ) => {}
            }
        }
    }
}

fn step_terminator(state: &mut [Init], uninit_fields: &mut HashSet<Place>, term: &Terminator) {
    for a in term.accesses() {
        if let Some(l) = a.place.as_local() {
            if matches!(
                a.access,
                PlaceAccess::Move | PlaceAccess::CallClosure(zil_ir::body::ClosureCallMode::Once)
            ) {
                state[l.index()] = Init::UNINIT;
                uninit_fields.retain(|p| p.root != zil_ir::body::PlaceRoot::Local(l));
            }
        } else if matches!(
            a.access,
            PlaceAccess::Move | PlaceAccess::CallClosure(zil_ir::body::ClosureCallMode::Once)
        ) {
            uninit_fields.insert(a.place.as_ref().clone());
        }
    }
}

fn bool_assign(flag: Local, value: bool, span: Span) -> Instruction {
    Instruction {
        kind: InstructionKind::Assign {
            dest: Place::local(flag),
            value: Rvalue::Use(Operand::Const(ConstValue::Bool(value))),
        },
        span,
    }
}

/// If `ufields` records one or more moved fields under `place` belonging to a
/// single enum variant of `adt`, returns that variant index.
fn active_moved_variant(
    place: &Place,
    adt: zil_ir::body::AdtId,
    ufields: &HashSet<Place>,
) -> Option<zil_ir::body::VariantIdx> {
    let prefix_len = place.projections.len();
    let mut found: Option<zil_ir::body::VariantIdx> = None;
    for u in ufields {
        if u.root == place.root
            && u.projections.len() > prefix_len
            && u.projections.starts_with(&place.projections)
            && let zil_ir::body::Projection::Field(fref) = u.projections[prefix_len]
            && fref.adt == adt
        {
            match found {
                None => found = Some(fref.variant),
                Some(prev) if prev == fref.variant => {}
                Some(_) => return None,
            }
        }
    }
    found
}

/// If `ufields` records one or more moved places under `place` belonging to a
/// single ErrorUnion arm (`ok` or `err`), returns that arm (`true` = ok, `false` = err).
fn active_moved_error_arm(place: &Place, ufields: &HashSet<Place>) -> Option<bool> {
    let prefix_len = place.projections.len();
    let mut found: Option<bool> = None;
    for u in ufields {
        if u.root == place.root
            && u.projections.len() > prefix_len
            && u.projections.starts_with(&place.projections)
            && let zil_ir::body::Projection::ErrorArm(ok) = u.projections[prefix_len]
        {
            match found {
                None => found = Some(ok),
                Some(prev) if prev == ok => {}
                Some(_) => return None,
            }
        }
    }
    found
}

fn place_cleanup_uninit(
    place: &Place,
    ty: zil_ir::body::TyId,
    tys: &TyTable,
    ufields: &HashSet<Place>,
) -> bool {
    if !tys.needs_cleanup(ty, &zil_ir::body::ParamEnv::default()) {
        return true;
    }
    if ufields.contains(place) {
        return true;
    }
    match tys.kind(ty) {
        TyKind::Tuple(elems) => {
            let elems = elems.clone();
            if elems.is_empty() {
                return false;
            }
            elems.iter().enumerate().all(|(i, &ety)| {
                let mut sub = place.clone();
                sub.projections
                    .push(zil_ir::body::Projection::TupleField(i as u32));
                place_cleanup_uninit(&sub, ety, tys, ufields)
            })
        }
        TyKind::Adt { adt, args } => {
            let adt = *adt;
            let args = *args;
            let def = tys.adt(adt);
            if def.user_drop {
                return false;
            }
            let variant = if def.variants.len() == 1 {
                zil_ir::body::VariantIdx(0)
            } else if let Some(v) = active_moved_variant(place, adt, ufields) {
                v
            } else {
                return false;
            };
            def.variants[variant.0 as usize]
                .fields
                .iter()
                .enumerate()
                .all(|(fi, f)| {
                    let fidx = zil_ir::body::FieldIdx(fi as u32);
                    let fty = tys.field_ty(adt, variant, fidx, args).unwrap_or(f.ty);
                    let mut sub = place.clone();
                    sub.projections
                        .push(zil_ir::body::Projection::Field(zil_ir::body::FieldRef {
                            adt,
                            variant,
                            field: fidx,
                            args,
                        }));
                    place_cleanup_uninit(&sub, fty, tys, ufields)
                })
        }
        TyKind::ErrorUnion { ok, err } => {
            let ok_ty = *ok;
            let err_ty = *err;
            let Some(arm_ok) = active_moved_error_arm(place, ufields) else {
                return false;
            };
            let arm_ty = if arm_ok { ok_ty } else { err_ty };
            let sub = place
                .clone()
                .project(zil_ir::body::Projection::ErrorArm(arm_ok));
            place_cleanup_uninit(&sub, arm_ty, tys, ufields)
        }
        TyKind::Unit
        | TyKind::Bool
        | TyKind::Int(_)
        | TyKind::Float(_)
        | TyKind::Never
        | TyKind::Param(_)
        | TyKind::Array { .. }
        | TyKind::Slice(_)
        | TyKind::Ref { .. }
        | TyKind::FnPtr { .. }
        | TyKind::Closure { .. }
        | TyKind::Dyn(_)
        | TyKind::OwnedDyn { .. }
        | TyKind::Managed(_)
        | TyKind::Desc(_)
        | TyKind::Opaque(_) => false,
    }
}

fn all_cleanup_fields_uninit(
    l: Local,
    ty: zil_ir::body::TyId,
    tys: &TyTable,
    ufields: &HashSet<Place>,
) -> bool {
    if !tys.needs_cleanup(ty, &zil_ir::body::ParamEnv::default()) {
        return false;
    }
    place_cleanup_uninit(&Place::local(l), ty, tys, ufields)
}

/// When a composite local has some (but not all) fields moved out, expands a
/// whole-local drop into per-field drops for the surviving cleanup fields.
fn collect_live_cleanup_places(
    place: &Place,
    ty: zil_ir::body::TyId,
    tys: &TyTable,
    ufields: &HashSet<Place>,
    out: &mut Vec<Place>,
) {
    if !tys.needs_cleanup(ty, &zil_ir::body::ParamEnv::default()) || ufields.contains(place) {
        return;
    }
    let has_moved_descendant = ufields.iter().any(|u| {
        u.root == place.root
            && u.projections.len() > place.projections.len()
            && u.projections.starts_with(&place.projections)
    });
    if !has_moved_descendant {
        out.push(place.clone());
        return;
    }
    match tys.kind(ty) {
        TyKind::Tuple(elems) => {
            let elems = elems.clone();
            for (i, &ety) in elems.iter().enumerate() {
                let mut sub = place.clone();
                sub.projections
                    .push(zil_ir::body::Projection::TupleField(i as u32));
                collect_live_cleanup_places(&sub, ety, tys, ufields, out);
            }
        }
        TyKind::Adt { adt, args } => {
            let adt = *adt;
            let args = *args;
            let def = tys.adt(adt);
            if def.user_drop {
                out.push(place.clone());
                return;
            }
            let variant = if def.variants.len() == 1 {
                zil_ir::body::VariantIdx(0)
            } else if let Some(v) = active_moved_variant(place, adt, ufields) {
                v
            } else {
                out.push(place.clone());
                return;
            };
            for (fi, f) in def.variants[variant.0 as usize].fields.iter().enumerate() {
                let fidx = zil_ir::body::FieldIdx(fi as u32);
                let fty = tys.field_ty(adt, variant, fidx, args).unwrap_or(f.ty);
                let mut sub = place.clone();
                sub.projections
                    .push(zil_ir::body::Projection::Field(zil_ir::body::FieldRef {
                        adt,
                        variant,
                        field: fidx,
                        args,
                    }));
                collect_live_cleanup_places(&sub, fty, tys, ufields, out);
            }
        }
        TyKind::ErrorUnion { ok, err } => {
            let ok_ty = *ok;
            let err_ty = *err;
            if let Some(arm_ok) = active_moved_error_arm(place, ufields) {
                let arm_ty = if arm_ok { ok_ty } else { err_ty };
                let sub = place
                    .clone()
                    .project(zil_ir::body::Projection::ErrorArm(arm_ok));
                collect_live_cleanup_places(&sub, arm_ty, tys, ufields, out);
            } else {
                out.push(place.clone());
            }
        }
        TyKind::Unit
        | TyKind::Bool
        | TyKind::Int(_)
        | TyKind::Float(_)
        | TyKind::Never
        | TyKind::Param(_)
        | TyKind::Array { .. }
        | TyKind::Slice(_)
        | TyKind::Ref { .. }
        | TyKind::FnPtr { .. }
        | TyKind::Closure { .. }
        | TyKind::Dyn(_)
        | TyKind::OwnedDyn { .. }
        | TyKind::Managed(_)
        | TyKind::Desc(_)
        | TyKind::Opaque(_) => {
            out.push(place.clone());
        }
    }
}

fn alloc_drop_flag(
    locals: &mut Vec<LocalDecl>,
    drop_flags: &mut Vec<DropFlag>,
    tys: &mut TyTable,
    flagged: &mut HashMap<Place, Local>,
    place: &Place,
) -> Local {
    *flagged.entry(place.clone()).or_insert_with(|| {
        let bool_ty = tys.intern(TyKind::Bool);
        let flag = Local::from_index(locals.len());
        locals.push(LocalDecl {
            ty: bool_ty,
            name: None,
        });
        drop_flags.push(DropFlag {
            flag,
            place: place.clone(),
        });
        flag
    })
}

/// Expands `place` into per-field `(sub_place, opt_drop_flag)` cleanups when
/// one or more sub-fields of a product (`Tuple`, `Record`, `Package`,
/// `Resource`, or single-variant `Enum` without `user_drop`) are conditionally
/// or unconditionally moved out.
///
/// Returns `false` if a live sub-place with moved descendants cannot be
/// expanded as a flat product (e.g., `user_drop`, multi-variant `Enum`, or
/// `ErrorUnion`).
#[allow(clippy::too_many_arguments)]
fn collect_conditional_cleanup_places(
    place: &Place,
    ty: zil_ir::body::TyId,
    tys: &mut TyTable,
    locals: &mut Vec<LocalDecl>,
    drop_flags: &mut Vec<DropFlag>,
    ufields: &HashSet<Place>,
    may_ufields: &HashSet<Place>,
    ancestor_may_uninit: bool,
    flagged: &mut HashMap<Place, Local>,
    out: &mut Vec<(Place, Option<Local>)>,
) -> bool {
    if !tys.needs_cleanup(ty, &zil_ir::body::ParamEnv::default()) || ufields.contains(place) {
        return true;
    }
    let place_may_uninit = ancestor_may_uninit || may_ufields.contains(place);
    let has_moved_descendant = may_ufields.iter().any(|u| {
        u.root == place.root
            && u.projections.len() > place.projections.len()
            && u.projections.starts_with(&place.projections)
    });
    if !has_moved_descendant {
        let opt_flag = if place_may_uninit {
            Some(alloc_drop_flag(locals, drop_flags, tys, flagged, place))
        } else {
            None
        };
        out.push((place.clone(), opt_flag));
        return true;
    }
    match tys.kind(ty).clone() {
        TyKind::Tuple(elems) => {
            for (i, ety) in elems.into_iter().enumerate() {
                let mut sub = place.clone();
                sub.projections
                    .push(zil_ir::body::Projection::TupleField(i as u32));
                if !collect_conditional_cleanup_places(
                    &sub,
                    ety,
                    tys,
                    locals,
                    drop_flags,
                    ufields,
                    may_ufields,
                    place_may_uninit,
                    flagged,
                    out,
                ) {
                    return false;
                }
            }
            true
        }
        TyKind::Adt { adt, args } => {
            let (is_flat_product, field_tys) = {
                let def = tys.adt(adt);
                if def.user_drop || def.variants.len() != 1 {
                    (false, Vec::new())
                } else {
                    let variant = zil_ir::body::VariantIdx(0);
                    let ftys: Vec<zil_ir::body::TyId> = def.variants[0]
                        .fields
                        .iter()
                        .enumerate()
                        .map(|(fi, f)| {
                            tys.field_ty(adt, variant, zil_ir::body::FieldIdx(fi as u32), args)
                                .unwrap_or(f.ty)
                        })
                        .collect();
                    (true, ftys)
                }
            };
            if !is_flat_product {
                return false;
            }
            for (fi, fty) in field_tys.into_iter().enumerate() {
                let mut sub = place.clone();
                sub.projections
                    .push(zil_ir::body::Projection::Field(zil_ir::body::FieldRef {
                        adt,
                        variant: zil_ir::body::VariantIdx(0),
                        field: zil_ir::body::FieldIdx(fi as u32),
                        args,
                    }));
                if !collect_conditional_cleanup_places(
                    &sub,
                    fty,
                    tys,
                    locals,
                    drop_flags,
                    ufields,
                    may_ufields,
                    place_may_uninit,
                    flagged,
                    out,
                ) {
                    return false;
                }
            }
            true
        }
        TyKind::Unit
        | TyKind::Bool
        | TyKind::Int(_)
        | TyKind::Float(_)
        | TyKind::Never
        | TyKind::Param(_)
        | TyKind::Array { .. }
        | TyKind::Slice(_)
        | TyKind::Ref { .. }
        | TyKind::FnPtr { .. }
        | TyKind::Closure { .. }
        | TyKind::Dyn(_)
        | TyKind::OwnedDyn { .. }
        | TyKind::ErrorUnion { .. }
        | TyKind::Managed(_)
        | TyKind::Desc(_)
        | TyKind::Opaque(_) => false,
    }
}

/// Rewrites every `(block, inst_idx)` auto-drop marker in `body`.
pub fn elaborate(body: &mut Body, tys: &mut TyTable, auto_drops: &HashSet<(BlockId, usize)>) {
    if auto_drops.is_empty() || body.blocks.is_empty() {
        return;
    }
    let n_locals = body.locals.len();
    let n_blocks = body.blocks.len();

    // 1. Forward dataflow: maybe_init / maybe_uninit per local + must-uninit and may-uninit fields.
    type BlockEntry = (Vec<Init>, HashSet<Place>, HashSet<Place>);
    let mut entry: Vec<Option<BlockEntry>> = vec![None; n_blocks];
    let mut init0 = vec![Init::UNINIT; n_locals];
    for slot in init0.iter_mut().take(body.param_count as usize) {
        *slot = Init::INIT;
    }
    entry[0] = Some((init0, HashSet::new(), HashSet::new()));
    let mut queue = VecDeque::from([BlockId(0)]);
    let mut queued = vec![false; n_blocks];
    queued[0] = true;

    while let Some(b) = queue.pop_front() {
        queued[b.index()] = false;
        let Some((mut state, mut ufields, mut may_ufields)) = entry[b.index()].clone() else {
            continue;
        };
        let block = &body.blocks[b.index()];
        for inst in &block.instructions {
            let mut state_dup = state.clone();
            step_instruction(&mut state, &mut ufields, &inst.kind);
            step_instruction(&mut state_dup, &mut may_ufields, &inst.kind);
        }
        let mut state_dup = state.clone();
        step_terminator(&mut state, &mut ufields, &block.terminator);
        step_terminator(&mut state_dup, &mut may_ufields, &block.terminator);
        for (edge_kind, target) in block.terminator.edges() {
            let mut s = state.clone();
            let mut uf = ufields.clone();
            let mut muf = may_ufields.clone();
            for a in block.terminator.edge_accesses(edge_kind) {
                if a.access == PlaceAccess::Init {
                    if let Some(l) = a.place.as_local() {
                        s[l.index()] = Init::INIT;
                        uf.retain(|p| p.root != zil_ir::body::PlaceRoot::Local(l));
                        muf.retain(|p| p.root != zil_ir::body::PlaceRoot::Local(l));
                    } else {
                        let target = a.place.as_ref();
                        let clear = |set: &mut HashSet<Place>| {
                            set.retain(|p| {
                                !(p.root == target.root
                                    && p.projections.len() >= target.projections.len()
                                    && p.projections[..target.projections.len()]
                                        == target.projections[..])
                            });
                        };
                        clear(&mut uf);
                        clear(&mut muf);
                    }
                }
            }
            let changed = match &mut entry[target.index()] {
                None => {
                    entry[target.index()] = Some((s, uf, muf));
                    true
                }
                Some((old, old_uf, old_muf)) => {
                    let mut diff = false;
                    for (o, n) in old.iter_mut().zip(&s) {
                        let j = o.join(*n);
                        if j != *o {
                            *o = j;
                            diff = true;
                        }
                    }
                    let before_len = old_uf.len();
                    old_uf.retain(|p| uf.contains(p));
                    if old_uf.len() != before_len {
                        diff = true;
                    }
                    let before_muf = old_muf.len();
                    old_muf.extend(muf);
                    if old_muf.len() != before_muf {
                        diff = true;
                    }
                    diff
                }
            };
            if changed && !queued[target.index()] {
                queued[target.index()] = true;
                queue.push_back(target);
            }
        }
    }

    // 2. Decide which places need a drop flag (only those observed as Maybe
    //    at some reachable auto-drop point).
    type DropPlan = Vec<(Place, Option<Local>)>;
    #[derive(Clone)]
    enum PartialDropStep {
        Field {
            place: Place,
            flag: Option<Local>,
        },
        NestedEnum {
            place: Place,
            discr: Local,
            variants: Vec<Vec<PartialDropStep>>,
        },
    }
    struct EnumPartialDrop {
        discr: Local,
        outer_flag: Option<Local>,
        variants: Vec<Vec<PartialDropStep>>,
        local: Local,
    }
    struct ErrorUnionPartialDrop {
        discr: Local,
        outer_flag: Option<Local>,
        err_drops: DropPlan,
        ok_drops: DropPlan,
        local: Local,
    }
    #[allow(clippy::too_many_arguments)]
    fn plan_enum_partial_drop(
        lp: &Place,
        adt: zil_ir::body::AdtId,
        args: zil_ir::body::ArgsId,
        local: Local,
        outer_flag: Option<Local>,
        tys: &mut TyTable,
        locals: &mut Vec<LocalDecl>,
        drop_flags: &mut Vec<DropFlag>,
        ufields: &HashSet<Place>,
        may_ufields: &HashSet<Place>,
        flagged: &mut HashMap<Place, Local>,
        depth: u32,
    ) -> EnumPartialDrop {
        let variants_meta: Vec<Vec<zil_ir::body::TyId>> = {
            let def = tys.adt(adt);
            def.variants
                .iter()
                .enumerate()
                .map(|(vi, v)| {
                    let vidx = zil_ir::body::VariantIdx(vi as u32);
                    v.fields
                        .iter()
                        .enumerate()
                        .map(|(fi, f)| {
                            tys.field_ty(adt, vidx, zil_ir::body::FieldIdx(fi as u32), args)
                                .unwrap_or(f.ty)
                        })
                        .collect()
                })
                .collect()
        };
        let mut variants_plan = Vec::with_capacity(variants_meta.len());
        for (vi, ftys) in variants_meta.into_iter().enumerate() {
            let mut drops = Vec::new();
            for (fi, fty) in ftys.into_iter().enumerate() {
                if !tys.needs_cleanup(fty, &zil_ir::body::ParamEnv::default()) {
                    continue;
                }
                let mut sub = lp.clone();
                sub.projections
                    .push(zil_ir::body::Projection::Field(zil_ir::body::FieldRef {
                        adt,
                        variant: zil_ir::body::VariantIdx(vi as u32),
                        field: zil_ir::body::FieldIdx(fi as u32),
                        args,
                    }));
                if ufields.contains(&sub) {
                    continue;
                }
                let nested_enum = if depth < 8
                    && let TyKind::Adt {
                        adt: sub_adt,
                        args: sub_args,
                    } = *tys.kind(fty)
                {
                    let sub_def = tys.adt(sub_adt);
                    if !sub_def.user_drop
                        && sub_def.variants.len() > 1
                        && may_ufields.iter().any(|u| {
                            u.root == sub.root
                                && u.projections.len() > sub.projections.len()
                                && u.projections.starts_with(&sub.projections)
                        })
                    {
                        Some((sub_adt, sub_args))
                    } else {
                        None
                    }
                } else {
                    None
                };
                if let Some((sub_adt, sub_args)) = nested_enum {
                    let sub_epd = plan_enum_partial_drop(
                        &sub,
                        sub_adt,
                        sub_args,
                        local,
                        None,
                        tys,
                        locals,
                        drop_flags,
                        ufields,
                        may_ufields,
                        flagged,
                        depth + 1,
                    );
                    drops.push(PartialDropStep::NestedEnum {
                        place: sub,
                        discr: sub_epd.discr,
                        variants: sub_epd.variants,
                    });
                } else {
                    let mut field_drops = Vec::new();
                    if collect_conditional_cleanup_places(
                        &sub,
                        fty,
                        tys,
                        locals,
                        drop_flags,
                        ufields,
                        may_ufields,
                        false,
                        flagged,
                        &mut field_drops,
                    ) {
                        for (place, flag) in field_drops {
                            drops.push(PartialDropStep::Field { place, flag });
                        }
                    } else {
                        let opt_flag = if may_ufields.contains(&sub) {
                            Some(alloc_drop_flag(locals, drop_flags, tys, flagged, &sub))
                        } else {
                            None
                        };
                        drops.push(PartialDropStep::Field {
                            place: sub,
                            flag: opt_flag,
                        });
                    }
                }
            }
            variants_plan.push(drops);
        }
        let i32_ty = tys.intern(TyKind::Int(IntTy::I32));
        let discr = Local::from_index(locals.len());
        locals.push(LocalDecl {
            ty: i32_ty,
            name: None,
        });
        EnumPartialDrop {
            discr,
            outer_flag,
            variants: variants_plan,
            local,
        }
    }
    let mut flagged: HashMap<Place, Local> = HashMap::new();
    let mut per_point: HashMap<(BlockId, usize), Init> = HashMap::new();
    let mut partial_drops: HashMap<(BlockId, usize), DropPlan> = HashMap::new();
    let mut enum_partial_drops: HashMap<(BlockId, usize), EnumPartialDrop> = HashMap::new();
    let mut error_partial_drops: HashMap<(BlockId, usize), ErrorUnionPartialDrop> = HashMap::new();
    let mut replace_to_assign: HashSet<(BlockId, usize)> = HashSet::new();
    for (b_idx, block) in body.blocks.iter().enumerate() {
        let Some((mut state, mut ufields, mut may_ufields)) = entry[b_idx].clone() else {
            continue;
        };
        let bid = BlockId::from_index(b_idx);
        for (i, inst) in block.instructions.iter().enumerate() {
            if let InstructionKind::Replace { place, .. } = &inst.kind
                && (ufields.contains(place)
                    || place
                        .as_local()
                        .is_some_and(|l| state[l.index()].is_definitely_uninit()))
            {
                replace_to_assign.insert((bid, i));
            }
            if auto_drops.contains(&(bid, i))
                && let InstructionKind::Drop { place } = &inst.kind
            {
                if let Some(l) = place.as_local() {
                    let mut st = state[l.index()];
                    if !st.is_definitely_uninit()
                        && all_cleanup_fields_uninit(l, body.locals[l.index()].ty, tys, &ufields)
                    {
                        st = Init::UNINIT;
                    }
                    per_point.insert((bid, i), st);
                    let lp = Place::local(l);
                    let has_may_moved = may_ufields.iter().any(|u| {
                        u.root == zil_ir::body::PlaceRoot::Local(l) && !u.projections.is_empty()
                    });
                    if !st.is_definitely_uninit() && has_may_moved {
                        let local_ty = body.locals[l.index()].ty;
                        let is_multi_enum = if let TyKind::Adt { adt, args } = tys.kind(local_ty) {
                            let def = tys.adt(*adt);
                            if !def.user_drop && def.variants.len() > 1 {
                                Some((*adt, *args))
                            } else {
                                None
                            }
                        } else {
                            None
                        };
                        let is_error_union =
                            if let TyKind::ErrorUnion { ok, err } = tys.kind(local_ty) {
                                Some((*ok, *err))
                            } else {
                                None
                            };
                        let has_must_moved = ufields.iter().any(|u| {
                            u.root == zil_ir::body::PlaceRoot::Local(l) && !u.projections.is_empty()
                        });
                        let has_strictly_may_moved = may_ufields.iter().any(|u| {
                            u.root == zil_ir::body::PlaceRoot::Local(l)
                                && !u.projections.is_empty()
                                && !ufields.contains(u)
                        });
                        let use_simple_partial = !st.is_maybe()
                            && has_must_moved
                            && !has_strictly_may_moved
                            && is_multi_enum.is_none_or(|(adt, _)| {
                                active_moved_variant(&lp, adt, &ufields).is_some()
                            })
                            && is_error_union
                                .is_none_or(|_| active_moved_error_arm(&lp, &ufields).is_some());
                        if use_simple_partial {
                            let mut subs = Vec::new();
                            collect_live_cleanup_places(&lp, local_ty, tys, &ufields, &mut subs);
                            partial_drops
                                .insert((bid, i), subs.into_iter().map(|p| (p, None)).collect());
                        } else if let Some((ok_ty, err_ty)) = is_error_union {
                            let outer_flag = if st.is_maybe() {
                                Some(alloc_drop_flag(
                                    &mut body.locals,
                                    &mut body.drop_flags,
                                    tys,
                                    &mut flagged,
                                    &lp,
                                ))
                            } else {
                                None
                            };
                            let err_place = lp
                                .clone()
                                .project(zil_ir::body::Projection::ErrorArm(false));
                            let mut err_drops = Vec::new();
                            if !collect_conditional_cleanup_places(
                                &err_place,
                                err_ty,
                                tys,
                                &mut body.locals,
                                &mut body.drop_flags,
                                &ufields,
                                &may_ufields,
                                false,
                                &mut flagged,
                                &mut err_drops,
                            ) {
                                err_drops.clear();
                                if tys.needs_cleanup(err_ty, &zil_ir::body::ParamEnv::default())
                                    && !ufields.contains(&err_place)
                                {
                                    let opt_flag = if may_ufields.contains(&err_place) {
                                        Some(alloc_drop_flag(
                                            &mut body.locals,
                                            &mut body.drop_flags,
                                            tys,
                                            &mut flagged,
                                            &err_place,
                                        ))
                                    } else {
                                        None
                                    };
                                    err_drops.push((err_place, opt_flag));
                                }
                            }
                            let ok_place =
                                lp.clone().project(zil_ir::body::Projection::ErrorArm(true));
                            let mut ok_drops = Vec::new();
                            if !collect_conditional_cleanup_places(
                                &ok_place,
                                ok_ty,
                                tys,
                                &mut body.locals,
                                &mut body.drop_flags,
                                &ufields,
                                &may_ufields,
                                false,
                                &mut flagged,
                                &mut ok_drops,
                            ) {
                                ok_drops.clear();
                                if tys.needs_cleanup(ok_ty, &zil_ir::body::ParamEnv::default())
                                    && !ufields.contains(&ok_place)
                                {
                                    let opt_flag = if may_ufields.contains(&ok_place) {
                                        Some(alloc_drop_flag(
                                            &mut body.locals,
                                            &mut body.drop_flags,
                                            tys,
                                            &mut flagged,
                                            &ok_place,
                                        ))
                                    } else {
                                        None
                                    };
                                    ok_drops.push((ok_place, opt_flag));
                                }
                            }
                            let bool_ty = tys.intern(TyKind::Bool);
                            let discr = Local::from_index(body.locals.len());
                            body.locals.push(LocalDecl {
                                ty: bool_ty,
                                name: None,
                            });
                            error_partial_drops.insert(
                                (bid, i),
                                ErrorUnionPartialDrop {
                                    discr,
                                    outer_flag,
                                    err_drops,
                                    ok_drops,
                                    local: l,
                                },
                            );
                        } else if let Some((adt, args)) = is_multi_enum {
                            let outer_flag = if st.is_maybe() {
                                Some(alloc_drop_flag(
                                    &mut body.locals,
                                    &mut body.drop_flags,
                                    tys,
                                    &mut flagged,
                                    &lp,
                                ))
                            } else {
                                None
                            };
                            let epd = plan_enum_partial_drop(
                                &lp,
                                adt,
                                args,
                                l,
                                outer_flag,
                                tys,
                                &mut body.locals,
                                &mut body.drop_flags,
                                &ufields,
                                &may_ufields,
                                &mut flagged,
                                0,
                            );
                            enum_partial_drops.insert((bid, i), epd);
                        } else {
                            let mut subs = Vec::new();
                            if collect_conditional_cleanup_places(
                                &lp,
                                local_ty,
                                tys,
                                &mut body.locals,
                                &mut body.drop_flags,
                                &ufields,
                                &may_ufields,
                                st.is_maybe(),
                                &mut flagged,
                                &mut subs,
                            ) {
                                partial_drops.insert((bid, i), subs);
                            } else if st.is_maybe() {
                                alloc_drop_flag(
                                    &mut body.locals,
                                    &mut body.drop_flags,
                                    tys,
                                    &mut flagged,
                                    &lp,
                                );
                            }
                        }
                    } else if st.is_maybe() {
                        alloc_drop_flag(
                            &mut body.locals,
                            &mut body.drop_flags,
                            tys,
                            &mut flagged,
                            &lp,
                        );
                    }
                } else if ufields.contains(place) {
                    per_point.insert((bid, i), Init::UNINIT);
                } else if may_ufields.contains(place) {
                    per_point.insert((bid, i), Init::INIT.join(Init::UNINIT));
                    alloc_drop_flag(
                        &mut body.locals,
                        &mut body.drop_flags,
                        tys,
                        &mut flagged,
                        place,
                    );
                } else if place.projections.is_empty()
                    && let zil_ir::body::PlaceRoot::Capture(ci) = place.root
                    && let Some(&cap_ty) = body.captures.get(ci as usize)
                    && ufields
                        .iter()
                        .any(|u| u.root == place.root && !u.projections.is_empty())
                {
                    let mut subs = Vec::new();
                    collect_live_cleanup_places(place, cap_ty, tys, &ufields, &mut subs);
                    if subs.is_empty() {
                        per_point.insert((bid, i), Init::UNINIT);
                    } else {
                        per_point.insert((bid, i), Init::INIT);
                        partial_drops
                            .insert((bid, i), subs.into_iter().map(|p| (p, None)).collect());
                    }
                } else {
                    per_point.insert((bid, i), Init::INIT);
                }
            }
            let mut state_dup = state.clone();
            step_instruction(&mut state, &mut ufields, &inst.kind);
            step_instruction(&mut state_dup, &mut may_ufields, &inst.kind);
        }
    }

    let place_is_prefix = |prefix: &Place, full: &Place| -> bool {
        prefix.root == full.root
            && prefix.projections.len() <= full.projections.len()
            && full.projections[..prefix.projections.len()] == prefix.projections[..]
    };

    // 3. Rewrite blocks: drop definite-uninit auto-drops, maintain flags for
    //    flagged places, and split blocks for maybe-init auto-drops.
    let mut extra_blocks: Vec<Block> = Vec::new();
    let orig_len = body.blocks.len();
    let alloc_block = |extra: &mut Vec<Block>, block: Block| -> BlockId {
        let id = BlockId::from_index(orig_len + extra.len());
        extra.push(block);
        id
    };

    for b_idx in 0..orig_len {
        if entry[b_idx].is_none() {
            continue;
        }
        let bid = BlockId::from_index(b_idx);
        let orig = std::mem::replace(
            &mut body.blocks[b_idx],
            Block {
                instructions: Vec::new(),
                terminator: Terminator::Unreachable,
                terminator_span: Span::default(),
            },
        );

        let mut cur_id = bid;
        let mut cur_insts: Vec<Instruction> = Vec::new();

        // Initialize flags at entry: parameters and captures start initialized (`true`),
        // other flagged places start `false`.
        if b_idx == 0 {
            for (p, &flag) in &flagged {
                let starts_init = match p.root {
                    zil_ir::body::PlaceRoot::Local(l) => {
                        (l.0 as usize) < (body.param_count as usize)
                    }
                    zil_ir::body::PlaceRoot::Capture(_) => true,
                };
                cur_insts.push(bool_assign(flag, starts_init, orig.terminator_span));
            }
        }

        let flush_block = |body_blocks: &mut [Block],
                           extra: &mut Vec<Block>,
                           id: BlockId,
                           insts: Vec<Instruction>,
                           term: Terminator,
                           span: Span| {
            let blk = Block {
                instructions: insts,
                terminator: term,
                terminator_span: span,
            };
            if id.index() < orig_len {
                body_blocks[id.index()] = blk;
            } else {
                extra[id.index() - orig_len] = blk;
            }
        };

        let emit_drop_chain = |extra: &mut Vec<Block>,
                               drops: &[(Place, Option<Local>)],
                               end_target: BlockId,
                               trailing_dead: Option<Local>,
                               span: Span|
         -> BlockId {
            let mut next_target = if let Some(dead_local) = trailing_dead {
                alloc_block(
                    extra,
                    Block {
                        instructions: vec![Instruction {
                            kind: InstructionKind::StorageDead(dead_local),
                            span,
                        }],
                        terminator: Terminator::Goto { target: end_target },
                        terminator_span: span,
                    },
                )
            } else {
                end_target
            };
            for (sub, opt_flag) in drops.iter().rev() {
                if let Some(flag) = opt_flag {
                    let do_drop = alloc_block(
                        extra,
                        Block {
                            instructions: vec![
                                Instruction {
                                    kind: InstructionKind::Drop { place: sub.clone() },
                                    span,
                                },
                                bool_assign(*flag, false, span),
                            ],
                            terminator: Terminator::Goto {
                                target: next_target,
                            },
                            terminator_span: span,
                        },
                    );
                    next_target = alloc_block(
                        extra,
                        Block {
                            instructions: Vec::new(),
                            terminator: Terminator::Switch {
                                discr: Operand::Copy(Place::local(*flag)),
                                arms: vec![(0, next_target)],
                                otherwise: do_drop,
                            },
                            terminator_span: span,
                        },
                    );
                } else {
                    next_target = alloc_block(
                        extra,
                        Block {
                            instructions: vec![Instruction {
                                kind: InstructionKind::Drop { place: sub.clone() },
                                span,
                            }],
                            terminator: Terminator::Goto {
                                target: next_target,
                            },
                            terminator_span: span,
                        },
                    );
                }
            }
            next_target
        };

        let mut skip_drop_old: Option<Local> = None;
        let mut skip_storage_dead: Option<Local> = None;
        for (i, inst) in orig.instructions.into_iter().enumerate() {
            let span = inst.span;
            if let Some(dead_l) = skip_storage_dead.take()
                && matches!(&inst.kind, InstructionKind::StorageDead(l) if *l == dead_l)
            {
                continue;
            }
            if let Some(old_l) = skip_drop_old.take()
                && matches!(&inst.kind, InstructionKind::Drop { place } if place.as_local() == Some(old_l))
            {
                continue;
            }
            if replace_to_assign.contains(&(bid, i))
                && let InstructionKind::Replace { place, value, old } = inst.kind
            {
                skip_drop_old = Some(old);
                let mut set_flags = Vec::new();
                for (fp, &flag) in &flagged {
                    if place_is_prefix(&place, fp) {
                        set_flags.push(flag);
                    }
                }
                cur_insts.push(Instruction {
                    kind: InstructionKind::Assign {
                        dest: place,
                        value: Rvalue::Use(value),
                    },
                    span,
                });
                for flag in set_flags {
                    cur_insts.push(bool_assign(flag, true, span));
                }
                continue;
            }
            if let Some(&st) = per_point.get(&(bid, i)) {
                let InstructionKind::Drop { place } = &inst.kind else {
                    cur_insts.push(inst);
                    continue;
                };
                if st.is_definitely_uninit() {
                    // Already moved on every path: omit drop.
                    continue;
                }
                let drop_place = place.clone();
                if let Some(eupd) = error_partial_drops.get(&(bid, i)) {
                    let after = alloc_block(
                        &mut extra_blocks,
                        Block {
                            instructions: Vec::new(),
                            terminator: Terminator::Unreachable,
                            terminator_span: span,
                        },
                    );
                    let err_target = emit_drop_chain(
                        &mut extra_blocks,
                        &eupd.err_drops,
                        after,
                        Some(eupd.local),
                        span,
                    );
                    let ok_target = emit_drop_chain(
                        &mut extra_blocks,
                        &eupd.ok_drops,
                        after,
                        Some(eupd.local),
                        span,
                    );
                    skip_storage_dead = Some(eupd.local);
                    let switch_term = Terminator::Switch {
                        discr: Operand::Copy(Place::local(eupd.discr)),
                        arms: vec![(0, err_target)],
                        otherwise: ok_target,
                    };
                    if let Some(outer_flag) = eupd.outer_flag {
                        let inspect_bb = alloc_block(
                            &mut extra_blocks,
                            Block {
                                instructions: vec![
                                    Instruction {
                                        kind: InstructionKind::Assign {
                                            dest: Place::local(eupd.discr),
                                            value: Rvalue::ErrorArm(drop_place.clone()),
                                        },
                                        span,
                                    },
                                    bool_assign(outer_flag, false, span),
                                ],
                                terminator: switch_term,
                                terminator_span: span,
                            },
                        );
                        flush_block(
                            &mut body.blocks,
                            &mut extra_blocks,
                            cur_id,
                            std::mem::take(&mut cur_insts),
                            Terminator::Switch {
                                discr: Operand::Copy(Place::local(outer_flag)),
                                arms: vec![(0, after)],
                                otherwise: inspect_bb,
                            },
                            span,
                        );
                    } else {
                        cur_insts.push(Instruction {
                            kind: InstructionKind::Assign {
                                dest: Place::local(eupd.discr),
                                value: Rvalue::ErrorArm(drop_place.clone()),
                            },
                            span,
                        });
                        flush_block(
                            &mut body.blocks,
                            &mut extra_blocks,
                            cur_id,
                            std::mem::take(&mut cur_insts),
                            switch_term,
                            span,
                        );
                    }
                    cur_id = after;
                    continue;
                } else if let Some(epd) = enum_partial_drops.get(&(bid, i)) {
                    #[allow(clippy::too_many_arguments)]
                    fn emit_steps(
                        steps: &[PartialDropStep],
                        root_local: Option<Local>,
                        after: BlockId,
                        span: Span,
                        base_len: usize,
                        extra_blocks: &mut Vec<Block>,
                    ) -> BlockId {
                        let alloc = |extra: &mut Vec<Block>, blk: Block| -> BlockId {
                            let id = BlockId::from_index(base_len + extra.len());
                            extra.push(blk);
                            id
                        };
                        let Some((head, rest)) = steps.split_first() else {
                            let mut insts = Vec::new();
                            if let Some(rl) = root_local {
                                insts.push(Instruction {
                                    kind: InstructionKind::StorageDead(rl),
                                    span,
                                });
                            }
                            return alloc(
                                extra_blocks,
                                Block {
                                    instructions: insts,
                                    terminator: Terminator::Goto { target: after },
                                    terminator_span: span,
                                },
                            );
                        };
                        match head {
                            PartialDropStep::Field { place, flag } => {
                                let cont = emit_steps(
                                    rest,
                                    root_local,
                                    after,
                                    span,
                                    base_len,
                                    extra_blocks,
                                );
                                if let Some(fl) = flag {
                                    let do_drop = alloc(
                                        extra_blocks,
                                        Block {
                                            instructions: vec![
                                                Instruction {
                                                    kind: InstructionKind::Drop {
                                                        place: place.clone(),
                                                    },
                                                    span,
                                                },
                                                bool_assign(*fl, false, span),
                                            ],
                                            terminator: Terminator::Goto { target: cont },
                                            terminator_span: span,
                                        },
                                    );
                                    alloc(
                                        extra_blocks,
                                        Block {
                                            instructions: Vec::new(),
                                            terminator: Terminator::Switch {
                                                discr: Operand::Copy(Place::local(*fl)),
                                                arms: vec![(0, cont)],
                                                otherwise: do_drop,
                                            },
                                            terminator_span: span,
                                        },
                                    )
                                } else {
                                    alloc(
                                        extra_blocks,
                                        Block {
                                            instructions: vec![Instruction {
                                                kind: InstructionKind::Drop {
                                                    place: place.clone(),
                                                },
                                                span,
                                            }],
                                            terminator: Terminator::Goto { target: cont },
                                            terminator_span: span,
                                        },
                                    )
                                }
                            }
                            PartialDropStep::NestedEnum {
                                place,
                                discr,
                                variants,
                            } => {
                                let mut sub_targets = Vec::with_capacity(variants.len());
                                for vsteps in variants {
                                    let mut combined =
                                        Vec::with_capacity(vsteps.len() + rest.len());
                                    combined.extend_from_slice(vsteps);
                                    combined.extend_from_slice(rest);
                                    sub_targets.push(emit_steps(
                                        &combined,
                                        root_local,
                                        after,
                                        span,
                                        base_len,
                                        extra_blocks,
                                    ));
                                }
                                let last_idx = sub_targets.len() - 1;
                                alloc(
                                    extra_blocks,
                                    Block {
                                        instructions: vec![Instruction {
                                            kind: InstructionKind::Assign {
                                                dest: Place::local(*discr),
                                                value: Rvalue::Discriminant(place.clone()),
                                            },
                                            span,
                                        }],
                                        terminator: Terminator::Switch {
                                            discr: Operand::Copy(Place::local(*discr)),
                                            arms: sub_targets[..last_idx]
                                                .iter()
                                                .enumerate()
                                                .map(|(vi, &tgt)| (vi as u128, tgt))
                                                .collect(),
                                            otherwise: sub_targets[last_idx],
                                        },
                                        terminator_span: span,
                                    },
                                )
                            }
                        }
                    }

                    let after = alloc_block(
                        &mut extra_blocks,
                        Block {
                            instructions: Vec::new(),
                            terminator: Terminator::Unreachable,
                            terminator_span: span,
                        },
                    );
                    let mut arm_targets = Vec::with_capacity(epd.variants.len());
                    for vdrops in &epd.variants {
                        arm_targets.push(emit_steps(
                            vdrops,
                            Some(epd.local),
                            after,
                            span,
                            orig_len,
                            &mut extra_blocks,
                        ));
                    }
                    skip_storage_dead = Some(epd.local);
                    let last_idx = arm_targets.len() - 1;
                    let switch_term = Terminator::Switch {
                        discr: Operand::Copy(Place::local(epd.discr)),
                        arms: arm_targets[..last_idx]
                            .iter()
                            .enumerate()
                            .map(|(vi, &tgt)| (vi as u128, tgt))
                            .collect(),
                        otherwise: arm_targets[last_idx],
                    };
                    if let Some(outer_flag) = epd.outer_flag {
                        let inspect_bb = alloc_block(
                            &mut extra_blocks,
                            Block {
                                instructions: vec![
                                    Instruction {
                                        kind: InstructionKind::Assign {
                                            dest: Place::local(epd.discr),
                                            value: Rvalue::Discriminant(drop_place.clone()),
                                        },
                                        span,
                                    },
                                    bool_assign(outer_flag, false, span),
                                ],
                                terminator: switch_term,
                                terminator_span: span,
                            },
                        );
                        flush_block(
                            &mut body.blocks,
                            &mut extra_blocks,
                            cur_id,
                            std::mem::take(&mut cur_insts),
                            Terminator::Switch {
                                discr: Operand::Copy(Place::local(outer_flag)),
                                arms: vec![(0, after)],
                                otherwise: inspect_bb,
                            },
                            span,
                        );
                    } else {
                        cur_insts.push(Instruction {
                            kind: InstructionKind::Assign {
                                dest: Place::local(epd.discr),
                                value: Rvalue::Discriminant(drop_place.clone()),
                            },
                            span,
                        });
                        flush_block(
                            &mut body.blocks,
                            &mut extra_blocks,
                            cur_id,
                            std::mem::take(&mut cur_insts),
                            switch_term,
                            span,
                        );
                    }
                    cur_id = after;
                    continue;
                } else if let Some(sub_drops) = partial_drops.get(&(bid, i)) {
                    let has_flags = sub_drops.iter().any(|(_, f)| f.is_some());
                    if has_flags {
                        let after = alloc_block(
                            &mut extra_blocks,
                            Block {
                                instructions: Vec::new(),
                                terminator: Terminator::Unreachable,
                                terminator_span: span,
                            },
                        );
                        let chain_head =
                            emit_drop_chain(&mut extra_blocks, sub_drops, after, None, span);
                        flush_block(
                            &mut body.blocks,
                            &mut extra_blocks,
                            cur_id,
                            std::mem::take(&mut cur_insts),
                            Terminator::Goto { target: chain_head },
                            span,
                        );
                        cur_id = after;
                        continue;
                    }
                    for (sub, _) in sub_drops {
                        cur_insts.push(Instruction {
                            kind: InstructionKind::Drop { place: sub.clone() },
                            span,
                        });
                    }
                } else if st.is_maybe() {
                    let flag = flagged[place];
                    let after = alloc_block(
                        &mut extra_blocks,
                        Block {
                            instructions: Vec::new(),
                            terminator: Terminator::Unreachable,
                            terminator_span: span,
                        },
                    );
                    let do_drop = alloc_block(
                        &mut extra_blocks,
                        Block {
                            instructions: vec![
                                Instruction {
                                    kind: InstructionKind::Drop { place: drop_place },
                                    span,
                                },
                                bool_assign(flag, false, span),
                            ],
                            terminator: Terminator::Goto { target: after },
                            terminator_span: span,
                        },
                    );
                    let term = Terminator::Switch {
                        discr: Operand::Copy(Place::local(flag)),
                        arms: vec![(0, after)],
                        otherwise: do_drop,
                    };
                    flush_block(
                        &mut body.blocks,
                        &mut extra_blocks,
                        cur_id,
                        std::mem::take(&mut cur_insts),
                        term,
                        span,
                    );
                    cur_id = after;
                    continue;
                } else {
                    cur_insts.push(inst);
                }
                for (fp, &flag) in &flagged {
                    if place_is_prefix(&drop_place, fp) {
                        cur_insts.push(bool_assign(flag, false, span));
                    }
                }
                continue;
            }

            // Ordinary instruction: emit it and update any affected drop flags.
            let mut updates: Vec<(Local, bool)> = Vec::new();
            match &inst.kind {
                InstructionKind::StorageLive(l) => {
                    for (fp, &flag) in &flagged {
                        if fp.root == zil_ir::body::PlaceRoot::Local(*l) {
                            updates.push((flag, false));
                        }
                    }
                }
                InstructionKind::StorageDead(_)
                | InstructionKind::ScopeEnter(_)
                | InstructionKind::ScopeExit { .. } => {}
                InstructionKind::Assign { .. }
                | InstructionKind::Drop { .. }
                | InstructionKind::Replace { .. }
                | InstructionKind::RegisterDefer { .. } => {
                    for a in inst.kind.accesses() {
                        match a.access {
                            PlaceAccess::Move | PlaceAccess::Drop => {
                                for (fp, &flag) in &flagged {
                                    if place_is_prefix(&a.place, fp)
                                        || place_is_prefix(fp, &a.place)
                                    {
                                        updates.push((flag, false));
                                    }
                                }
                            }
                            PlaceAccess::Init | PlaceAccess::ReplaceInstance => {
                                for (fp, &flag) in &flagged {
                                    if place_is_prefix(&a.place, fp) {
                                        updates.push((flag, true));
                                    }
                                }
                            }
                            PlaceAccess::Copy
                            | PlaceAccess::Borrow(_)
                            | PlaceAccess::Describe
                            | PlaceAccess::InspectTag
                            | PlaceAccess::InspectLen
                            | PlaceAccess::CallClosure(_) => {}
                        }
                    }
                }
            }
            cur_insts.push(inst);
            for (flag, val) in updates {
                cur_insts.push(bool_assign(flag, val, span));
            }
        }

        // Terminator: if it moves or initializes a flagged place on an outgoing
        // edge (e.g. `Call`), insert a trampoline block to update the flags.
        let mut term = orig.terminator;
        let tspan = orig.terminator_span;
        if let Terminator::Call {
            args,
            dest,
            target,
            cancel,
            ..
        } = &mut term
        {
            let mut edge_updates: Vec<(Local, bool)> = Vec::new();
            for a in args.iter() {
                if let Operand::Move(p) = a {
                    for (fp, &flag) in &flagged {
                        if place_is_prefix(p, fp) || place_is_prefix(fp, p) {
                            edge_updates.push((flag, false));
                        }
                    }
                }
            }
            for (fp, &flag) in &flagged {
                if place_is_prefix(dest, fp) {
                    edge_updates.push((flag, true));
                }
            }
            if !edge_updates.is_empty() {
                let real_target = *target;
                let mut tramp_insts = Vec::new();
                for (flag, val) in edge_updates {
                    tramp_insts.push(bool_assign(flag, val, tspan));
                }
                let tramp = alloc_block(
                    &mut extra_blocks,
                    Block {
                        instructions: tramp_insts,
                        terminator: Terminator::Goto {
                            target: real_target,
                        },
                        terminator_span: tspan,
                    },
                );
                *target = tramp;
            }
            // Cancel edge does not initialize `dest`, only clears moved args.
            if let Some(cancel_target) = cancel {
                let mut cancel_updates: Vec<(Local, bool)> = Vec::new();
                for a in args.iter() {
                    if let Operand::Move(p) = a {
                        for (fp, &flag) in &flagged {
                            if place_is_prefix(p, fp) || place_is_prefix(fp, p) {
                                cancel_updates.push((flag, false));
                            }
                        }
                    }
                }
                if !cancel_updates.is_empty() {
                    let real_cancel = *cancel_target;
                    let mut tramp_insts = Vec::new();
                    for (flag, val) in cancel_updates {
                        tramp_insts.push(bool_assign(flag, val, tspan));
                    }
                    let tramp = alloc_block(
                        &mut extra_blocks,
                        Block {
                            instructions: tramp_insts,
                            terminator: Terminator::Goto {
                                target: real_cancel,
                            },
                            terminator_span: tspan,
                        },
                    );
                    *cancel_target = tramp;
                }
            }
        }

        flush_block(
            &mut body.blocks,
            &mut extra_blocks,
            cur_id,
            cur_insts,
            term,
            tspan,
        );
    }

    body.blocks.extend(extra_blocks);
}
