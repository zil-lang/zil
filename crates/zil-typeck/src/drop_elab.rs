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
    Block, BlockId, Body, ConstValue, DropFlag, Instruction, InstructionKind, Local, LocalDecl,
    Operand, Place, PlaceAccess, Rvalue, Terminator, TyKind, TyTable,
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
                PlaceAccess::Init => {
                    state[l.index()] = Init::INIT;
                    uninit_fields.retain(|p| p.root != zil_ir::body::PlaceRoot::Local(l));
                }
                PlaceAccess::Copy
                | PlaceAccess::Borrow(_)
                | PlaceAccess::Describe
                | PlaceAccess::ReplaceInstance
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

fn all_cleanup_fields_uninit(
    l: Local,
    ty: zil_ir::body::TyId,
    tys: &TyTable,
    ufields: &HashSet<Place>,
) -> bool {
    let TyKind::Adt { adt, args } = *tys.kind(ty) else {
        return false;
    };
    let def = tys.adt(adt);
    if def.user_drop || def.variants.len() != 1 {
        return false;
    }
    let arg_tys = tys.args(args);
    let mut saw_cleanup = false;
    for (fi, f) in def.variants[0].fields.iter().enumerate() {
        let fty = if let TyKind::Param(idx) = *tys.kind(f.ty) {
            arg_tys.tys.get(idx as usize).copied().unwrap_or(f.ty)
        } else {
            f.ty
        };
        if tys.needs_cleanup(fty, &zil_ir::body::ParamEnv::default()) {
            saw_cleanup = true;
            let fp = Place {
                root: zil_ir::body::PlaceRoot::Local(l),
                projections: vec![zil_ir::body::Projection::Field(zil_ir::body::FieldRef {
                    adt,
                    variant: zil_ir::body::VariantIdx(0),
                    field: zil_ir::body::FieldIdx(fi as u32),
                    args,
                })],
            };
            if !ufields.contains(&fp) {
                return false;
            }
        }
    }
    saw_cleanup
}

/// Rewrites every `(block, inst_idx)` auto-drop marker in `body`.
pub fn elaborate(body: &mut Body, tys: &mut TyTable, auto_drops: &HashSet<(BlockId, usize)>) {
    if auto_drops.is_empty() || body.blocks.is_empty() {
        return;
    }
    let n_locals = body.locals.len();
    let n_blocks = body.blocks.len();

    // 1. Forward dataflow: maybe_init / maybe_uninit per local + must-uninit fields.
    let mut entry: Vec<Option<(Vec<Init>, HashSet<Place>)>> = vec![None; n_blocks];
    let mut init0 = vec![Init::UNINIT; n_locals];
    for slot in init0.iter_mut().take(body.param_count as usize) {
        *slot = Init::INIT;
    }
    entry[0] = Some((init0, HashSet::new()));
    let mut queue = VecDeque::from([BlockId(0)]);
    let mut queued = vec![false; n_blocks];
    queued[0] = true;

    while let Some(b) = queue.pop_front() {
        queued[b.index()] = false;
        let Some((mut state, mut ufields)) = entry[b.index()].clone() else {
            continue;
        };
        let block = &body.blocks[b.index()];
        for inst in &block.instructions {
            step_instruction(&mut state, &mut ufields, &inst.kind);
        }
        step_terminator(&mut state, &mut ufields, &block.terminator);
        for (edge_kind, target) in block.terminator.edges() {
            let mut s = state.clone();
            let mut uf = ufields.clone();
            for a in block.terminator.edge_accesses(edge_kind) {
                if let (PlaceAccess::Init, Some(l)) = (a.access, a.place.as_local()) {
                    s[l.index()] = Init::INIT;
                    uf.retain(|p| p.root != zil_ir::body::PlaceRoot::Local(l));
                }
            }
            let changed = match &mut entry[target.index()] {
                None => {
                    entry[target.index()] = Some((s, uf));
                    true
                }
                Some((old, old_uf)) => {
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
                    diff
                }
            };
            if changed && !queued[target.index()] {
                queued[target.index()] = true;
                queue.push_back(target);
            }
        }
    }

    // 2. Decide which locals need a drop flag (only those observed as Maybe
    //    at some reachable auto-drop point).
    let mut flagged: HashMap<Local, Local> = HashMap::new();
    let mut per_point: HashMap<(BlockId, usize), Init> = HashMap::new();
    let mut replace_to_assign: HashSet<(BlockId, usize)> = HashSet::new();
    for (b_idx, block) in body.blocks.iter().enumerate() {
        let Some((mut state, mut ufields)) = entry[b_idx].clone() else {
            continue;
        };
        let bid = BlockId::from_index(b_idx);
        for (i, inst) in block.instructions.iter().enumerate() {
            if let InstructionKind::Replace { place, .. } = &inst.kind
                && ufields.contains(place)
            {
                replace_to_assign.insert((bid, i));
            }
            if auto_drops.contains(&(bid, i))
                && let InstructionKind::Drop { place } = &inst.kind
                && let Some(l) = place.as_local()
            {
                let mut st = state[l.index()];
                if !st.is_definitely_uninit()
                    && all_cleanup_fields_uninit(l, body.locals[l.index()].ty, tys, &ufields)
                {
                    st = Init::UNINIT;
                }
                per_point.insert((bid, i), st);
                if st.is_maybe() && !flagged.contains_key(&l) {
                    let bool_ty = tys.intern(TyKind::Bool);
                    let flag = Local::from_index(body.locals.len());
                    body.locals.push(LocalDecl {
                        ty: bool_ty,
                        name: None,
                    });
                    body.drop_flags.push(DropFlag {
                        flag,
                        place: Place::local(l),
                    });
                    flagged.insert(l, flag);
                }
            }
            step_instruction(&mut state, &mut ufields, &inst.kind);
        }
    }

    // 3. Rewrite blocks: drop definite-uninit auto-drops, maintain flags for
    //    flagged locals, and split blocks for maybe-init auto-drops.
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

        // Initialize flags at entry: parameters start initialized (`true`),
        // other flagged locals (including compiler temporaries) start `false`.
        if b_idx == 0 {
            for (&l, &flag) in &flagged {
                let is_param = (l.0 as usize) < (body.param_count as usize);
                cur_insts.push(bool_assign(flag, is_param, orig.terminator_span));
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
                cur_insts.push(Instruction {
                    kind: InstructionKind::Assign {
                        dest: place,
                        value: Rvalue::Use(value),
                    },
                    span,
                });
                continue;
            }
            if let Some(&st) = per_point.get(&(bid, i)) {
                let InstructionKind::Drop { place } = &inst.kind else {
                    cur_insts.push(inst);
                    continue;
                };
                let Some(l) = place.as_local() else {
                    cur_insts.push(inst);
                    continue;
                };
                if st.is_definitely_uninit() {
                    // Already moved on every path: omit drop.
                    continue;
                }
                if st.is_maybe() {
                    let flag = flagged[&l];
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
                                    kind: InstructionKind::Drop {
                                        place: Place::local(l),
                                    },
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
                // Definitely initialized: keep the drop (and clear flag if any).
                cur_insts.push(inst);
                if let Some(&flag) = flagged.get(&l) {
                    cur_insts.push(bool_assign(flag, false, span));
                }
                continue;
            }

            // Ordinary instruction: emit it and update any affected drop flags.
            let mut updates: Vec<(Local, bool)> = Vec::new();
            match &inst.kind {
                InstructionKind::StorageLive(l) => {
                    if let Some(&flag) = flagged.get(l) {
                        updates.push((flag, false));
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
                        if let Some(l) = a.place.as_local() {
                            let Some(&flag) = flagged.get(&l) else {
                                continue;
                            };
                            match a.access {
                                PlaceAccess::Move | PlaceAccess::Drop => {
                                    updates.push((flag, false))
                                }
                                PlaceAccess::Init => updates.push((flag, true)),
                                PlaceAccess::Copy
                                | PlaceAccess::Borrow(_)
                                | PlaceAccess::Describe
                                | PlaceAccess::ReplaceInstance
                                | PlaceAccess::InspectTag
                                | PlaceAccess::InspectLen
                                | PlaceAccess::CallClosure(_) => {}
                            }
                        } else if matches!(a.access, PlaceAccess::Move | PlaceAccess::Drop)
                            && let (
                                zil_ir::body::PlaceRoot::Local(l),
                                [zil_ir::body::Projection::ErrorArm(_)],
                            ) = (a.place.root, a.place.projections.as_slice())
                            && let Some(&flag) = flagged.get(&l)
                        {
                            updates.push((flag, false));
                        }
                    }
                }
            }
            cur_insts.push(inst);
            for (flag, val) in updates {
                cur_insts.push(bool_assign(flag, val, span));
            }
        }

        // Terminator: if it moves or initializes a flagged local on an outgoing
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
                if let Operand::Move(p) = a
                    && let Some(l) = p.as_local()
                    && let Some(&flag) = flagged.get(&l)
                {
                    edge_updates.push((flag, false));
                }
            }
            if let Some(l) = dest.as_local()
                && let Some(&flag) = flagged.get(&l)
            {
                edge_updates.push((flag, true));
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
                    if let Operand::Move(p) = a
                        && let Some(l) = p.as_local()
                        && let Some(&flag) = flagged.get(&l)
                    {
                        cancel_updates.push((flag, false));
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
