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

fn step_instruction(state: &mut [Init], kind: &InstructionKind) {
    match kind {
        InstructionKind::StorageLive(l) | InstructionKind::StorageDead(l) => {
            state[l.index()] = Init::UNINIT;
            return;
        }
        InstructionKind::ScopeEnter(_) | InstructionKind::ScopeExit { .. } => return,
        InstructionKind::Assign { .. }
        | InstructionKind::Drop { .. }
        | InstructionKind::Replace { .. }
        | InstructionKind::RegisterDefer { .. } => {}
    }
    for a in kind.accesses() {
        let Some(l) = a.place.as_local() else {
            continue;
        };
        match a.access {
            PlaceAccess::Move | PlaceAccess::Drop => state[l.index()] = Init::UNINIT,
            PlaceAccess::Init => state[l.index()] = Init::INIT,
            PlaceAccess::Copy
            | PlaceAccess::Borrow(_)
            | PlaceAccess::Describe
            | PlaceAccess::ReplaceInstance
            | PlaceAccess::InspectTag
            | PlaceAccess::InspectLen
            | PlaceAccess::CallClosure(_) => {}
        }
    }
}

fn step_terminator(state: &mut [Init], term: &Terminator) {
    for a in term.accesses() {
        if let (PlaceAccess::Move, Some(l)) = (a.access, a.place.as_local()) {
            state[l.index()] = Init::UNINIT;
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

/// Rewrites every `(block, inst_idx)` auto-drop marker in `body`.
pub fn elaborate(body: &mut Body, tys: &mut TyTable, auto_drops: &HashSet<(BlockId, usize)>) {
    if auto_drops.is_empty() || body.blocks.is_empty() {
        return;
    }
    let n_locals = body.locals.len();
    let n_blocks = body.blocks.len();

    // 1. Forward dataflow: maybe_init / maybe_uninit per local.
    let mut entry: Vec<Option<Vec<Init>>> = vec![None; n_blocks];
    let mut init0 = vec![Init::UNINIT; n_locals];
    for slot in init0.iter_mut().take(body.param_count as usize) {
        *slot = Init::INIT;
    }
    entry[0] = Some(init0);
    let mut queue = VecDeque::from([BlockId(0)]);
    let mut queued = vec![false; n_blocks];
    queued[0] = true;

    while let Some(b) = queue.pop_front() {
        queued[b.index()] = false;
        let Some(mut state) = entry[b.index()].clone() else {
            continue;
        };
        let block = &body.blocks[b.index()];
        for inst in &block.instructions {
            step_instruction(&mut state, &inst.kind);
        }
        step_terminator(&mut state, &block.terminator);
        for (edge_kind, target) in block.terminator.edges() {
            let mut s = state.clone();
            for a in block.terminator.edge_accesses(edge_kind) {
                if let (PlaceAccess::Init, Some(l)) = (a.access, a.place.as_local()) {
                    s[l.index()] = Init::INIT;
                }
            }
            let changed = match &mut entry[target.index()] {
                None => {
                    entry[target.index()] = Some(s);
                    true
                }
                Some(old) => {
                    let mut diff = false;
                    for (o, n) in old.iter_mut().zip(&s) {
                        let j = o.join(*n);
                        if j != *o {
                            *o = j;
                            diff = true;
                        }
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
    for (b_idx, block) in body.blocks.iter().enumerate() {
        let Some(mut state) = entry[b_idx].clone() else {
            continue;
        };
        let bid = BlockId::from_index(b_idx);
        for (i, inst) in block.instructions.iter().enumerate() {
            if auto_drops.contains(&(bid, i))
                && let InstructionKind::Drop { place } = &inst.kind
                && let Some(l) = place.as_local()
            {
                let st = state[l.index()];
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
            step_instruction(&mut state, &inst.kind);
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

        // Initialize flags for flagged parameters at entry.
        if b_idx == 0 {
            for p in 0..body.param_count {
                let l = Local(p);
                if let Some(&flag) = flagged.get(&l) {
                    cur_insts.push(bool_assign(flag, true, orig.terminator_span));
                }
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

        for (i, inst) in orig.instructions.into_iter().enumerate() {
            let span = inst.span;
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
                        let Some(l) = a.place.as_local() else {
                            continue;
                        };
                        let Some(&flag) = flagged.get(&l) else {
                            continue;
                        };
                        match a.access {
                            PlaceAccess::Move | PlaceAccess::Drop => updates.push((flag, false)),
                            PlaceAccess::Init => updates.push((flag, true)),
                            PlaceAccess::Copy
                            | PlaceAccess::Borrow(_)
                            | PlaceAccess::Describe
                            | PlaceAccess::ReplaceInstance
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
