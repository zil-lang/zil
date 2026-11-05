//! Paid forward provenance, independent of resource proofs.
//! DAGs retain their original schedule; cycles use monotone input-state worklists.
use super::*;
use zil_ir::{BasicBlock, ClosureId, ValueId};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Origin {
    Empty,
    Input(usize),
    Mixed,
}
impl Origin {
    fn join(self, other: Self) -> Self {
        if self == other { self } else { Self::Mixed }
    }
}
#[derive(Clone, PartialEq, Eq)]
struct Frame {
    locals: Vec<Origin>,
    values: Vec<Origin>,
    activated: Vec<u8>,
    retired: Vec<bool>,
    done: Origin,
}
impl Frame {
    fn merge(&mut self, other: &Self, b: &mut Budget) -> CompileResult<bool> {
        b.spend(
            self.locals
                .len()
                .saturating_mul(3)
                .saturating_add(self.values.len())
                .saturating_add(1),
            "owning CFG frame merge",
        )?;
        let mut changed = false;
        for i in 0..self.locals.len() {
            changed |= self.locals[i] != self.locals[i].join(other.locals[i])
                || self.activated[i] != (self.activated[i] | other.activated[i])
                || self.retired[i] != (self.retired[i] | other.retired[i]);
            self.locals[i] = self.locals[i].join(other.locals[i]);
            self.activated[i] |= other.activated[i];
            self.retired[i] |= other.retired[i];
        }
        for (left, right) in self.values.iter_mut().zip(&other.values) {
            changed |= *left != left.join(*right);
            *left = left.join(*right);
        }
        changed |= self.done != self.done.join(other.done);
        self.done = self.done.join(other.done);
        Ok(changed)
    }
    fn copy(&self, b: &mut Budget) -> CompileResult<Self> {
        b.spend(
            self.locals
                .len()
                .saturating_mul(3)
                .saturating_add(self.values.len())
                .saturating_add(1),
            "owning CFG frame clone",
        )?;
        Ok(self.clone())
    }
}
fn edges(block: &BasicBlock) -> CompileResult<[Option<usize>; 2]> {
    Ok(match block.terminator {
        Terminator::Jump(id) => [Some(id.0 as usize), None],
        Terminator::Branch {
            then_block,
            else_block,
            ..
        } => [Some(then_block.0 as usize), Some(else_block.0 as usize)],
        Terminator::Return(_) | Terminator::Trap => [None, None],
        _ => return Err(closed()),
    })
}
fn closure_value(f: &TypedFunction, v: ValueId) -> bool {
    matches!(f.values.get(v.0 as usize), Some(Type::Closure(_)))
}
fn closure_local(f: &TypedFunction, l: zil_ir::LocalId) -> bool {
    matches!(
        f.locals.get(l.0 as usize).map(|l| &l.ty),
        Some(Type::Closure(_))
    )
}
fn scalar_operands(kind: &InstructionKind) -> [Option<ValueId>; 2] {
    match kind {
        InstructionKind::Unary { operand, .. } => [Some(*operand), None],
        InstructionKind::Binary { left, right, .. } => [Some(*left), Some(*right)],
        InstructionKind::ConvertInteger { value, .. }
        | InstructionKind::ConvertNumber { value, .. } => [Some(*value), None],
        InstructionKind::ReborrowAddress { address, .. } => [Some(*address), None],
        _ => [None, None],
    }
}

pub(super) fn parameter(
    f: &TypedFunction,
    call: &InstructionKind,
    id: ClosureId,
    b: &mut Budget,
) -> CompileResult<usize> {
    match solve(f, call, id, b, false) {
        Err(errors)
            if errors
                .iter()
                .all(|d| d.category != zil_base::ResultCategory::ResourceLimit) =>
        {
            solve(f, call, id, b, true)
        }
        result => result,
    }
}
fn solve(
    f: &TypedFunction,
    call: &InstructionKind,
    id: ClosureId,
    b: &mut Budget,
    cyclic: bool,
) -> CompileResult<usize> {
    let n = f.blocks.len();
    b.spend(
        n.saturating_mul(8)
            .saturating_add(f.locals.len())
            .saturating_add(f.values.len())
            .saturating_add(f.parameters.len()),
        "owning CFG graph and origin inventory",
    )?;
    let role = f
        .callable_contract
        .as_ref()
        .filter(|c| c.roles.len() == 1)
        .and_then(|c| c.roles.first())
        .ok_or_else(closed)?;
    let p = role.parameter as usize;
    if role.kind != zil_ir::CallableKind::FnOnce
        || role.cleanup != zil_ir::CallableCleanup::Owned
        || role.closure != id
    {
        return Err(closed());
    }
    let mut input = None;
    for (index, local) in f.parameters.iter().enumerate() {
        if closure_local(f, *local)
            && (input.replace(index).is_some()
                || index != p
                || f.locals.get(local.0 as usize).map(|l| &l.ty) != Some(&Type::Closure(id)))
        {
            return Err(closed());
        }
    }
    if input != Some(p) || n == 0 || f.entry.0 as usize >= n {
        return Err(closed());
    }
    let mut incoming = vec![0usize; n];
    let mut ready = Vec::with_capacity(n);
    let mut states: Vec<Option<Frame>> = vec![None; n];
    let mut defined = vec![false; f.values.len()];
    for (index, block) in f.blocks.iter().enumerate() {
        if block.id.0 as usize != index {
            return Err(closed());
        }
        for next in edges(block)?.into_iter().flatten() {
            if next >= n {
                return Err(closed());
            }
            incoming[next] = incoming[next].saturating_add(1);
        }
        b.spend(
            block.instructions.len(),
            "owning CFG SSA definition inventory",
        )?;
        for i in &block.instructions {
            if let Some(value) = i.result {
                let def = defined.get_mut(value.0 as usize).ok_or_else(closed)?;
                if *def {
                    return Err(closed());
                }
                *def = true;
            }
        }
    }
    let entry = f.entry.0 as usize;
    if !cyclic && incoming[entry] != 0 {
        return Err(closed());
    }
    b.spend(
        f.locals
            .len()
            .saturating_mul(3)
            .saturating_add(f.values.len()),
        "owning CFG initial frame",
    )?;
    let mut initial = Frame {
        locals: vec![Origin::Empty; f.locals.len()],
        values: vec![Origin::Empty; f.values.len()],
        activated: vec![1; f.locals.len()],
        retired: vec![false; f.locals.len()],
        done: Origin::Empty,
    };
    let local = f.parameters[p].0 as usize;
    initial.locals[local] = Origin::Input(p);
    initial.activated[local] = 2;
    states[entry] = Some(initial);
    let mut seen = if cyclic {
        b.spend(n, "cyclic owning CFG block activation inventory")?;
        vec![false; n]
    } else {
        Vec::new()
    };
    if cyclic {
        ready.push(entry);
    } else {
        for (index, count) in incoming.iter().enumerate() {
            if *count == 0 {
                ready.push(index);
            }
        }
    }
    let mut visited = 0usize;
    let mut answer = None;
    while let Some(index) = ready.pop() {
        b.spend(3, "owning CFG actual block visit and edges")?;
        if !cyclic || !seen[index] {
            visited += 1;
        }
        if cyclic {
            seen[index] = true;
        }
        let block = &f.blocks[index];
        let mut frame = if cyclic {
            states[index].as_ref().ok_or_else(closed)?.copy(b)?
        } else {
            states[index].take().ok_or_else(closed)?
        };
        b.spend(block.instructions.len(), "owning CFG linear transfers")?;
        for i in &block.instructions {
            let result = i.result;
            if result.is_some_and(|v| closure_value(f, v))
                && !matches!(i.kind, InstructionKind::TakeLocal { .. })
            {
                return Err(closed());
            }
            match &i.kind {
                InstructionKind::TakeLocal { local } if closure_local(f, *local) => {
                    let l = local.0 as usize;
                    if frame.retired[l] || frame.activated[l] != 2 || frame.done != Origin::Empty {
                        return Err(closed());
                    }
                    let origin = frame.locals[l];
                    if !matches!(origin, Origin::Input(_)) {
                        return Err(closed());
                    }
                    let value = result
                        .filter(|v| f.values.get(v.0 as usize) == Some(&Type::Closure(id)))
                        .ok_or_else(closed)?;
                    frame.values[value.0 as usize] = origin;
                    frame.locals[l] = Origin::Empty;
                }
                InstructionKind::Store { local, value } if closure_local(f, *local) => {
                    let l = local.0 as usize;
                    let v = value.0 as usize;
                    if frame.retired[l]
                        || frame.activated[l] != 2
                        || f.locals[l].ty != Type::Closure(id)
                        || frame.locals[l] != Origin::Empty
                        || frame.done != Origin::Empty
                        || !matches!(frame.values.get(v), Some(Origin::Input(_)))
                        || f.values.get(v) != Some(&Type::Closure(id))
                        || *local == f.parameters[p]
                    {
                        return Err(closed());
                    }
                    frame.locals[l] = frame.values[v];
                    frame.values[v] = Origin::Empty;
                }
                InstructionKind::CallClosureOnce { callee, arguments } => {
                    b.spend(arguments.len(), "owning CFG callback operands")?;
                    if arguments.iter().any(|v| closure_value(f, *v)) || frame.done != Origin::Empty
                    {
                        return Err(closed());
                    }
                    let value = frame.values.get_mut(callee.0 as usize).ok_or_else(closed)?;
                    let Origin::Input(actual) = *value else {
                        return Err(closed());
                    };
                    if actual != p {
                        return Err(closed());
                    }
                    if (std::ptr::eq(&i.kind, call) || i.kind == *call)
                        && answer.replace(actual).is_some_and(|old| old != actual)
                    {
                        return Err(closed());
                    }
                    *value = Origin::Empty;
                    frame.done = Origin::Input(actual);
                }
                InstructionKind::DropClosure { local } if closure_local(f, *local) => {
                    let l = local.0 as usize;
                    match frame.locals[l] {
                        Origin::Input(actual) if frame.done == Origin::Empty => {
                            frame.done = Origin::Input(actual);
                            frame.locals[l] = Origin::Empty;
                        }
                        Origin::Empty => {}
                        _ => return Err(closed()),
                    }
                    frame.retired[l] = true;
                    if cyclic {
                        frame.activated[l] = 1;
                    }
                }
                InstructionKind::StorageDead(local) if closure_local(f, *local) => {
                    let l = local.0 as usize;
                    if frame.locals[l] != Origin::Empty {
                        return Err(closed());
                    }
                    frame.retired[l] = true;
                    if cyclic {
                        frame.activated[l] = 1;
                    }
                }
                InstructionKind::StorageLive(local) if closure_local(f, *local) => {
                    let l = local.0 as usize;
                    if frame.activated[l] != 1
                        || (frame.retired[l] && !cyclic)
                        || (cyclic
                            && (frame.locals[l] != Origin::Empty || *local == f.parameters[p]))
                    {
                        return Err(closed());
                    }
                    frame.activated[l] = 2;
                    if cyclic {
                        frame.retired[l] = false;
                    }
                }
                InstructionKind::Load(local) | InstructionKind::TakeLocal { local }
                    if closure_local(f, *local) =>
                {
                    return Err(closed());
                }
                InstructionKind::Store { value, .. } if closure_value(f, *value) => {
                    return Err(closed());
                }
                InstructionKind::ScopeEnter { .. }
                | InstructionKind::ScopeExit { .. }
                | InstructionKind::StorageLive(_)
                | InstructionKind::StorageDead(_)
                | InstructionKind::Constant(_)
                | InstructionKind::Load(_)
                | InstructionKind::TakeLocal { .. }
                | InstructionKind::Store { .. }
                | InstructionKind::ReborrowAddress { mutable: false, .. }
                | InstructionKind::Unary { .. }
                | InstructionKind::Binary { .. }
                | InstructionKind::ConvertInteger { .. }
                | InstructionKind::ConvertNumber { .. } => {
                    if scalar_operands(&i.kind)
                        .into_iter()
                        .flatten()
                        .any(|v| closure_value(f, v))
                    {
                        return Err(closed());
                    }
                }
                _ => return Err(closed()),
            }
        }
        let terminal = match block.terminator {
            Terminator::Return(v) => {
                if v.is_some_and(|v| closure_value(f, v)) {
                    return Err(closed());
                }
                true
            }
            Terminator::Branch { condition, .. } => {
                if closure_value(f, condition) {
                    return Err(closed());
                }
                false
            }
            _ => false,
        };
        if terminal {
            b.spend(
                frame.locals.len().saturating_add(frame.values.len()),
                "owning CFG return debt inventory",
            )?;
            if frame.done != Origin::Input(p)
                || frame
                    .locals
                    .iter()
                    .chain(&frame.values)
                    .any(|o| *o != Origin::Empty)
            {
                return Err(closed());
            }
        }
        for next in edges(block)?.into_iter().flatten() {
            let changed = if let Some(previous) = &mut states[next] {
                previous.merge(&frame, b)?
            } else {
                states[next] = Some(frame.copy(b)?);
                true
            };
            if cyclic {
                if changed {
                    ready.push(next);
                }
            } else {
                incoming[next] -= 1;
                if incoming[next] == 0 {
                    ready.push(next);
                }
            }
        }
    }
    if visited != n {
        return Err(closed());
    }
    answer.ok_or_else(closed)
}
