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
                    if let (
                        zil_ir::body::PlaceRoot::Local(l),
                        [zil_ir::body::Projection::ErrorArm(_)],
                    ) = (a.place.root, a.place.projections.as_slice())
                    {
                        state[l.index()] = Init::UNINIT;
                    }
                    uninit_fields.insert(a.place.as_ref().clone());
                }
                PlaceAccess::Init | PlaceAccess::ReplaceInstance => {
                    uninit_fields.remove(a.place.as_ref());
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
            let arg_tys = tys.args(args);
            def.variants[variant.0 as usize]
                .fields
                .iter()
                .enumerate()
                .all(|(fi, f)| {
                    let fty = if let TyKind::Param(idx) = *tys.kind(f.ty) {
                        arg_tys.tys.get(idx as usize).copied().unwrap_or(f.ty)
                    } else {
                        f.ty
                    };
                    let mut sub = place.clone();
                    sub.projections
                        .push(zil_ir::body::Projection::Field(zil_ir::body::FieldRef {
                            adt,
                            variant,
                            field: zil_ir::body::FieldIdx(fi as u32),
                            args,
                        }));
                    place_cleanup_uninit(&sub, fty, tys, ufields)
                })
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
/// whole-local drop into per-field drops for the surviving cleanup fields in
/// reverse declaration order.
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
            let arg_tys = tys.args(args);
            for (fi, f) in def.variants[variant.0 as usize].fields.iter().enumerate() {
                let fty = if let TyKind::Param(idx) = *tys.kind(f.ty) {
                    arg_tys.tys.get(idx as usize).copied().unwrap_or(f.ty)
                } else {
                    f.ty
                };
                let mut sub = place.clone();
                sub.projections
                    .push(zil_ir::body::Projection::Field(zil_ir::body::FieldRef {
                        adt,
                        variant,
                        field: zil_ir::body::FieldIdx(fi as u32),
                        args,
                    }));
                collect_live_cleanup_places(&sub, fty, tys, ufields, out);
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
        | TyKind::ErrorUnion { .. }
        | TyKind::Managed(_)
        | TyKind::Desc(_)
        | TyKind::Opaque(_) => {
            out.push(place.clone());
        }
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
                if let (PlaceAccess::Init, Some(l)) = (a.access, a.place.as_local()) {
                    s[l.index()] = Init::INIT;
                    uf.retain(|p| p.root != zil_ir::body::PlaceRoot::Local(l));
                    muf.retain(|p| p.root != zil_ir::body::PlaceRoot::Local(l));
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
    struct EnumPartialDrop {
        discr: Local,
        variants: Vec<Vec<(Place, Option<Local>)>>,
    }
    let mut flagged: HashMap<Place, Local> = HashMap::new();
    let mut per_point: HashMap<(BlockId, usize), Init> = HashMap::new();
    let mut partial_drops: HashMap<(BlockId, usize), Vec<Place>> = HashMap::new();
    let mut enum_partial_drops: HashMap<(BlockId, usize), EnumPartialDrop> = HashMap::new();
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
                    if st.is_maybe() && !flagged.contains_key(&lp) {
                        let bool_ty = tys.intern(TyKind::Bool);
                        let flag = Local::from_index(body.locals.len());
                        body.locals.push(LocalDecl {
                            ty: bool_ty,
                            name: None,
                        });
                        body.drop_flags.push(DropFlag {
                            flag,
                            place: lp.clone(),
                        });
                        flagged.insert(lp, flag);
                    } else if !st.is_definitely_uninit() && !st.is_maybe() && has_may_moved {
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
                        let has_must_moved = ufields.iter().any(|u| {
                            u.root == zil_ir::body::PlaceRoot::Local(l) && !u.projections.is_empty()
                        });
                        let use_simple_partial = has_must_moved
                            && is_multi_enum.is_none_or(|(adt, _)| {
                                active_moved_variant(&lp, adt, &ufields).is_some()
                                    && !may_ufields.iter().any(|u| {
                                        u.root == zil_ir::body::PlaceRoot::Local(l)
                                            && !ufields.contains(u)
                                    })
                            });
                        if use_simple_partial {
                            let mut subs = Vec::new();
                            collect_live_cleanup_places(&lp, local_ty, tys, &ufields, &mut subs);
                            partial_drops.insert((bid, i), subs);
                        } else if let Some((adt, args)) = is_multi_enum {
                            let variants_meta: Vec<Vec<zil_ir::body::TyId>> = {
                                let def = tys.adt(adt);
                                def.variants
                                    .iter()
                                    .map(|v| {
                                        v.fields
                                            .iter()
                                            .map(|f| tys.subst_lookup(f.ty, args).unwrap_or(f.ty))
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
                                    sub.projections.push(zil_ir::body::Projection::Field(
                                        zil_ir::body::FieldRef {
                                            adt,
                                            variant: zil_ir::body::VariantIdx(vi as u32),
                                            field: zil_ir::body::FieldIdx(fi as u32),
                                            args,
                                        },
                                    ));
                                    if ufields.contains(&sub) {
                                        continue;
                                    }
                                    let opt_flag = if may_ufields.contains(&sub) {
                                        let flag =
                                            *flagged.entry(sub.clone()).or_insert_with(|| {
                                                let bool_ty = tys.intern(TyKind::Bool);
                                                let fl = Local::from_index(body.locals.len());
                                                body.locals.push(LocalDecl {
                                                    ty: bool_ty,
                                                    name: None,
                                                });
                                                body.drop_flags.push(DropFlag {
                                                    flag: fl,
                                                    place: sub.clone(),
                                                });
                                                fl
                                            });
                                        Some(flag)
                                    } else {
                                        None
                                    };
                                    drops.push((sub, opt_flag));
                                }
                                variants_plan.push(drops);
                            }
                            let i32_ty = tys.intern(TyKind::Int(IntTy::I32));
                            let discr = Local::from_index(body.locals.len());
                            body.locals.push(LocalDecl {
                                ty: i32_ty,
                                name: None,
                            });
                            enum_partial_drops.insert(
                                (bid, i),
                                EnumPartialDrop {
                                    discr,
                                    variants: variants_plan,
                                },
                            );
                        }
                    }
                } else if ufields.contains(place) {
                    per_point.insert((bid, i), Init::UNINIT);
                } else if may_ufields.contains(place) {
                    per_point.insert((bid, i), Init::INIT.join(Init::UNINIT));
                    if !flagged.contains_key(place) {
                        let bool_ty = tys.intern(TyKind::Bool);
                        let flag = Local::from_index(body.locals.len());
                        body.locals.push(LocalDecl {
                            ty: bool_ty,
                            name: None,
                        });
                        body.drop_flags.push(DropFlag {
                            flag,
                            place: place.clone(),
                        });
                        flagged.insert(place.clone(), flag);
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

        let mut skip_drop_old: Option<Local> = None;
        for (i, inst) in orig.instructions.into_iter().enumerate() {
            let span = inst.span;
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
                if st.is_maybe() {
                    let flag = flagged[place];
                    let drop_place = place.clone();
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
                }
                // Definitely initialized: keep the drop (or expand partial-move field drops).
                let drop_place = place.clone();
                if let Some(epd) = enum_partial_drops.get(&(bid, i)) {
                    cur_insts.push(Instruction {
                        kind: InstructionKind::Assign {
                            dest: Place::local(epd.discr),
                            value: Rvalue::Discriminant(drop_place.clone()),
                        },
                        span,
                    });
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
                        if vdrops.is_empty() {
                            arm_targets.push(after);
                            continue;
                        }
                        let mut next_target = after;
                        for (sub, opt_flag) in vdrops.iter().rev() {
                            if let Some(flag) = opt_flag {
                                let do_drop = alloc_block(
                                    &mut extra_blocks,
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
                                    &mut extra_blocks,
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
                                    &mut extra_blocks,
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
                        arm_targets.push(next_target);
                    }
                    let last_idx = arm_targets.len() - 1;
                    let term = Terminator::Switch {
                        discr: Operand::Copy(Place::local(epd.discr)),
                        arms: arm_targets[..last_idx]
                            .iter()
                            .enumerate()
                            .map(|(vi, &tgt)| (vi as u128, tgt))
                            .collect(),
                        otherwise: arm_targets[last_idx],
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
                } else if let Some(sub_places) = partial_drops.get(&(bid, i)) {
                    for sub in sub_places {
                        cur_insts.push(Instruction {
                            kind: InstructionKind::Drop { place: sub.clone() },
                            span,
                        });
                    }
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
                                if let (
                                    zil_ir::body::PlaceRoot::Local(l),
                                    [zil_ir::body::Projection::ErrorArm(_)],
                                ) = (a.place.root, a.place.projections.as_slice())
                                    && let Some(&flag) = flagged.get(&Place::local(l))
                                {
                                    updates.push((flag, false));
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
